//! Textual fabric format: human-readable, line-oriented, diffable.
//!
//! ```text
//! fabric v0
//! region entry
//!   %0 = param i32
//!   %1 = const i32 42
//!   %2 = arith.add i32 %0, %1
//!   %5 = br %3, then, else
//! ```
//!
//! Every cell — terminators included — carries its explicit id, so parsing
//! re-creates the exact slab (holes included) and ids are stable across
//! text round-trips.
//!
//! Printing is canonical: regions in index order, cells in region order,
//! ids as written in the slab. Parsing re-places cells under their
//! explicit ids, so print(parse(print(f))) == print(f) even when the slab
//! has holes (a removed cell's id never comes back).

use crate::cell::{ArithOp, Cell, CellKind, CmpOp};
use crate::fabric::Fabric;
use crate::id::{CellId, RegionId};
use crate::ty::{ConstVal, Type};

pub const HEADER: &str = "fabric v0";

/// Render one cell as a source line (no leading indent).
pub fn render_cell(f: &Fabric, id: CellId) -> String {
    let c = match f.cell(id) {
        Some(c) => c,
        None => return format!("{} = <removed>", id),
    };
    let o = |i: usize| -> String {
        c.operands.get(i).map(|x| x.to_string()).unwrap_or("%<missing>".into())
    };
    match &c.kind {
        CellKind::Param { ty } => format!("{} = param {}", id, ty.name()),
        CellKind::Const { ty, val } => format!("{} = const {} {}", id, ty.name(), val.render()),
        CellKind::Arith { op, ty } => format!("{} = {} {} {}, {}", id, op.name(), ty.name(), o(0), o(1)),
        CellKind::Cmp { op } => format!("{} = {} {}, {}", id, op.name(), o(0), o(1)),
        CellKind::Branch { then_r, else_r } => format!(
            "{} = br {}, {}, {}",
            id,
            o(0),
            f.region_name(*then_r),
            f.region_name(*else_r)
        ),
        CellKind::Jump { target } => format!("{} = jump {}", id, f.region_name(*target)),
        CellKind::Phi { joins } => {
            let parts: Vec<String> = joins
                .iter()
                .zip(c.operands.iter())
                .map(|(r, v)| format!("[{}: {}]", f.region_name(*r), v))
                .collect();
            format!("{} = phi {}", id, parts.join(" "))
        }
        CellKind::Ret => {
            if c.operands.is_empty() {
                format!("{} = ret", id)
            } else {
                format!("{} = ret {}", id, o(0))
            }
        }
    }
}

/// Canonical print of a whole fabric.
pub fn print(f: &Fabric) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');
    for region in f.regions.iter() {
        out.push_str(&format!("region {}\n", region.name));
        for &id in &region.cells {
            out.push_str("  ");
            out.push_str(&render_cell(f, id));
            out.push('\n');
        }
    }
    out
}

#[derive(Debug, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

fn err(line: usize, message: impl Into<String>) -> ParseError {
    ParseError { line, message: message.into() }
}

fn strip_comment(s: &str) -> &str {
    match s.find(';') {
        Some(i) => &s[..i],
        None => s,
    }
}

fn parse_cell_id(tok: &str) -> Option<CellId> {
    let n = tok.strip_prefix('%')?;
    n.parse::<u32>().ok().map(CellId)
}


/// Parse the textual format back into a fabric. Cell ids in the text are
/// the slab ids; region order in the file is the region index order.
pub fn parse(text: &str) -> Result<Fabric, ParseError> {
    let mut f = Fabric::empty();
    let mut region_ids: Vec<RegionId> = vec![];
    let mut pending_cells: Vec<(usize, RegionId, Vec<String>)> = vec![]; // line, id, raw tokens
    let mut current: Option<RegionId> = None;
    let mut saw_header = false;

    for (no, raw) in text.lines().enumerate() {
        let no = no + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let toks: Vec<&str> = line
            .split_whitespace()
            .map(|t| t.trim_end_matches(','))
            .filter(|t| !t.is_empty())
            .collect();
        if !saw_header {
            if toks.concat() != HEADER.replace(' ', "") {
                return Err(err(no, format!("expected header '{}', got '{}'", HEADER, line)));
            }
            saw_header = true;
            continue;
        }
        match toks[0] {
            "region" => {
                if toks.len() != 2 {
                    return Err(err(no, "region line must be 'region NAME'"));
                }
                let name = toks[1];
                if !is_ident(name) {
                    return Err(err(no, format!("bad region name '{}'", name)));
                }
                if f.regions.iter().any(|r| r.name == name) {
                    return Err(err(no, format!("duplicate region name '{}'", name)));
                }
                region_ids.push(f.add_region(name));
                current = region_ids.last().copied();
            }
            _ => {
                let region = current.ok_or_else(|| err(no, "cell before any region declaration"))?;
                pending_cells.push((no, region, toks.iter().map(|s| s.to_string()).collect()));
            }
        }
    }

    if !saw_header {
        return Err(err(text.lines().count().max(1), "missing 'fabric v0' header"));
    }

    // Second pass: parse cell bodies (now all regions exist, so names resolve).
    for (no, region, toks) in pending_cells {
        let toks: Vec<&str> = toks.iter().map(|s| s.as_str()).collect();
        if toks.len() < 3 || toks[1] != "=" {
            return Err(err(no, "every cell line must be '%id = ...'"));
        }
        let id =
            parse_cell_id(toks[0]).ok_or_else(|| err(no, format!("bad cell id '{}'", toks[0])))?;
        let cell = parse_cell_line(&f, region, &toks[2..], no)?;
        f.place_cell(id, cell).map_err(|m| err(no, m))?;
    }
    Ok(f)
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn region_by_name(f: &Fabric, name: &str, line: usize) -> Result<RegionId, ParseError> {
    f.regions
        .iter()
        .position(|r| r.name == name)
        .map(|i| RegionId(i as u32))
        .ok_or_else(|| err(line, format!("unknown region '{}'", name)))
}

fn parse_cell_line(
    f: &Fabric,
    region: RegionId,
    rest: &[&str],
    line: usize,
) -> Result<Cell, ParseError> {
    let operand = |tok: &str| -> Result<CellId, ParseError> {
        parse_cell_id(tok).ok_or_else(|| err(line, format!("bad operand '{}'", tok)))
    };

    match rest.first().copied().unwrap_or("") {
        "param" => {
            if rest.len() != 2 {
                return Err(err(line, "param takes one type"));
            }
            let ty = Type::parse(rest[1]).ok_or_else(|| err(line, format!("bad type '{}'", rest[1])))?;
            Ok(Cell::new(region, CellKind::Param { ty }))
        }
        "const" => {
            if rest.len() != 3 {
                return Err(err(line, "const takes TYPE VALUE"));
            }
            let ty = Type::parse(rest[1]).ok_or_else(|| err(line, format!("bad type '{}'", rest[1])))?;
            let val = ConstVal::parse(ty, rest[2])
                .ok_or_else(|| err(line, format!("bad constant '{}' for {}", rest[2], ty.name())))?;
            Ok(Cell::new(region, CellKind::Const { ty, val }))
        }
        op if op.starts_with("arith.") => {
            let op = ArithOp::parse(op).ok_or_else(|| err(line, format!("bad arith op '{}'", op)))?;
            if rest.len() != 4 {
                return Err(err(line, "arith takes TYPE A B"));
            }
            let ty = Type::parse(rest[1]).ok_or_else(|| err(line, format!("bad type '{}'", rest[1])))?;
            let mut c = Cell::new(region, CellKind::Arith { op, ty });
            c.operands = vec![operand(rest[2])?, operand(rest[3])?];
            Ok(c)
        }
        op if op.starts_with("cmp.") => {
            let op = match op {
                "cmp.eq" => CmpOp::Eq,
                "cmp.ne" => CmpOp::Ne,
                "cmp.lt" => CmpOp::Lt,
                "cmp.le" => CmpOp::Le,
                "cmp.gt" => CmpOp::Gt,
                "cmp.ge" => CmpOp::Ge,
                _ => return Err(err(line, format!("bad cmp op '{}'", op))),
            };
            if rest.len() != 3 {
                return Err(err(line, "cmp takes A B"));
            }
            let mut c = Cell::new(region, CellKind::Cmp { op });
            c.operands = vec![operand(rest[1])?, operand(rest[2])?];
            Ok(c)
        }
        "br" => {
            if rest.len() != 4 {
                return Err(err(line, "br takes COND THEN ELSE"));
            }
            let cond = operand(rest[1])?;
            let then_r = region_by_name(f, rest[2], line)?;
            let else_r = region_by_name(f, rest[3], line)?;
            let mut c = Cell::new(region, CellKind::Branch { then_r, else_r });
            c.operands = vec![cond];
            Ok(c)
        }
        "jump" => {
            if rest.len() != 2 {
                return Err(err(line, "jump takes TARGET"));
            }
            let target = region_by_name(f, rest[1], line)?;
            Ok(Cell::new(region, CellKind::Jump { target }))
        }
        "phi" => {
            let mut joins = vec![];
            let mut ops = vec![];
            let body = &rest[1..];
            let mut i = 0;
            while i < body.len() {
                // Accept both "[r:%v]" and "[r: %v]" (printer uses the latter).
                let mut tok = body[i].to_string();
                i += 1;
                if !tok.ends_with(']') {
                    if i >= body.len() {
                        return Err(err(line, format!("bad phi join '{}'", tok)));
                    }
                    tok.push(' ');
                    tok.push_str(body[i]);
                    i += 1;
                }
                let inner = tok
                    .strip_prefix('[')
                    .and_then(|t| t.strip_suffix(']'))
                    .ok_or_else(|| err(line, format!("bad phi join '{}'", tok)))?;
                let (rname, vid) = inner
                    .split_once(':')
                    .ok_or_else(|| err(line, format!("bad phi join '{}'", tok)))?;
                joins.push(region_by_name(f, rname.trim(), line)?);
                ops.push(operand(vid.trim())?);
            }
            if joins.is_empty() {
                return Err(err(line, "phi needs at least one join"));
            }
            let mut c = Cell::new(region, CellKind::Phi { joins });
            c.operands = ops;
            Ok(c)
        }
        "ret" => {
            let mut c = Cell::new(region, CellKind::Ret);
            if rest.len() == 2 {
                c.operands = vec![operand(rest[1])?];
            } else if rest.len() != 1 {
                return Err(err(line, "ret takes zero or one operand"));
            }
            Ok(c)
        }
        other => Err(err(line, format!("unknown instruction '{}'", other))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Fabric {
        let text = r#"fabric v0
region entry
  %0 = param i32
  %1 = const i32 42
  %2 = arith.add i32 %0, %1
  %3 = cmp.lt %2, %1
  %5 = br %3, then, else
region then
  %4 = const i64 1i64
  %6 = jump join
region else
  %7 = const i64 2i64
  %8 = jump join
region join
  %9 = phi [then: %4] [else: %7]
  %10 = ret %9
"#;
        parse(text).expect("sample must parse")
    }

    #[test]
    fn print_then_parse_then_print_is_identity() {
        let f = sample();
        let once = print(&f);
        let f2 = parse(&once).expect("reparsed");
        let twice = print(&f2);
        assert_eq!(once, twice, "canonical print must roundtrip");
    }

    #[test]
    fn parse_reports_line_numbers() {
        let bad = "fabric v0\nregion entry\n  %0 = bogus i32\n";
        let e = parse(bad).unwrap_err();
        assert_eq!(e.line, 3, "error must point at the offending line: {}", e);
        assert!(e.message.contains("bogus"), "message names the problem: {}", e);
    }

    #[test]
    fn parse_rejects_missing_header() {
        let e = parse("region entry\n").unwrap_err();
        assert!(e.message.contains("header"));
    }

    #[test]
    fn parse_rejects_unknown_region_ref() {
        let bad = "fabric v0\nregion entry\n  %0 = const i32 1\n  %1 = jump nowhere\n";
        let e = parse(bad).unwrap_err();
        assert!(e.message.contains("nowhere"));
    }

    #[test]
    fn ids_survive_holes_on_roundtrip() {
        // Build a fabric with a hole: place %0 and %2, skip %1.
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        f.place_cell(CellId(0), Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) })).unwrap();
        f.place_cell(CellId(2), Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(2) })).unwrap();
        let t1 = print(&f);
        assert!(t1.contains("%0"));
        assert!(t1.contains("%2"));
        assert!(!t1.contains("%1"), "hole must not be printed: {}", t1);
        let f2 = parse(&t1).unwrap();
        assert!(f2.cell(CellId(1)).is_none(), "hole must come back as a hole");
        assert_eq!(print(&f2), t1);
    }
}

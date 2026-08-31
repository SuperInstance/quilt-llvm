//! Programs: named function fabrics + the program-level verifier.
//!
//! v1 text format (a sibling of the single-fabric `fabric v0` format):
//!
//! ```text
//! fabric v1
//! fn main
//! region entry
//!   %0 = param i32
//!   %1 = const i32 2
//!   %2 = call i32 add2 %0, %1
//!   %3 = ret %2
//! fn add2
//! region entry
//!   %0 = param i32
//!   %1 = param i32
//!   %2 = arith.add i32 %0, %1
//!   %3 = ret %2
//! ```
//!
//! Each fn body is delegated to the single-fabric parser (header trick:
//! the body is re-prefixed with `fabric v0` internally), so per-fn errors
//! carry line numbers RELATIVE TO THE FN BODY — the offset is reported
//! alongside. Round-trip: print(parse(print(p))) == print(p).
//!
//! Program verification (codes P01–P04) checks what a lone fabric cannot:
//! callee existence, arity, argument types, and return-type agreement.

use crate::cell::CellKind;
use crate::fabric::Fabric;
use crate::verify::{VerifyError, verify};
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Program {
    /// declaration order (canonical print order)
    pub order: Vec<String>,
    pub funcs: BTreeMap<String, Fabric>,
}

impl Program {
    pub fn new() -> Program {
        Program::default()
    }

    pub fn add(&mut self, name: impl Into<String>, f: Fabric) {
        let name = name.into();
        if !self.funcs.contains_key(&name) {
            self.order.push(name.clone());
        }
        self.funcs.insert(name, f);
    }

    pub fn get(&self, name: &str) -> Option<&Fabric> {
        self.funcs.get(name)
    }
}

pub const HEADER: &str = "fabric v1";

#[derive(Debug, PartialEq)]
pub struct ProgramParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ProgramParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Parse a `fabric v1` program. Fn bodies delegate to text::parse; body
/// line numbers are offset-adjusted into whole-file coordinates.
pub fn parse(text: &str) -> Result<Program, ProgramParseError> {
    let mut p = Program::new();
    let mut saw_header = false;
    let mut current_fn: Option<(String, Vec<String>, usize)> = None; // name, body lines, fn start line
    for (no, raw) in text.lines().enumerate() {
        let no = no + 1;
        let line = crate::text::strip_comment_pub(raw).trim();
        if line.is_empty() {
            if let Some((_, body, _)) = &mut current_fn {
                body.push(String::new());
            }
            continue;
        }
        if !saw_header {
            if line.replace(' ', "") != HEADER.replace(' ', "") {
                return Err(ProgramParseError {
                    line: no,
                    message: format!("expected header '{}', got '{}'", HEADER, line),
                });
            }
            saw_header = true;
            continue;
        }
        if let Some(fname) = line.strip_prefix("fn ") {
            if let Some((name, body, start)) = current_fn.take() {
                if let Err(e) = add_fn(&mut p, &name, &body, start) {
                    return Err(e);
                }
            }
            let fname = fname.trim().to_string();
            if fname.is_empty() {
                return Err(ProgramParseError { line: no, message: "fn needs a name".into() });
            }
            current_fn = Some((fname, vec![], no));
            continue;
        }
        match &mut current_fn {
            Some((_, body, _)) => body.push(line.to_string()),
            None => {
                return Err(ProgramParseError {
                    line: no,
                    message: format!("expected 'fn NAME' before '{}'", line),
                })
            }
        }
    }
    if let Some((name, body, start)) = current_fn.take() {
        if let Err(e) = add_fn(&mut p, &name, &body, start) {
            return Err(e);
        }
    }
    if !saw_header {
        return Err(ProgramParseError { line: 0, message: "missing 'fabric v1' header".into() });
    }
    if p.funcs.is_empty() {
        return Err(ProgramParseError { line: 0, message: "program has no functions".into() });
    }
    Ok(p)
}

fn add_fn(p: &mut Program, name: &str, body: &[String], fn_line: usize) -> Result<(), ProgramParseError> {
    // delegate to the single-fabric parser via the v0 header trick
    let mut text = String::from("fabric v0\n");
    for l in body {
        text.push_str(l);
        text.push('\n');
    }
    match crate::text::parse(&text) {
        Ok(f) => {
            p.add(name, f);
            Ok(())
        }
        Err(e) => Err(ProgramParseError {
            // -1: the delegated parse prepends a synthetic header line
            line: fn_line + e.line.saturating_sub(1),
            message: format!("fn '{}': {}", name, e.message),
        }),
    }
}

/// Canonical print.
pub fn print(p: &Program) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');
    for name in &p.order {
        out.push_str(&format!("fn {}\n", name));
        let body = crate::text::print(&p.funcs[name]);
        // strip the single-fabric header line
        for l in body.lines().skip(1) {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

/// Program-level verification. Every fn must verify on its own (V-codes),
/// and every call must resolve and type-check against its callee (P-codes).
pub fn verify_program(p: &Program) -> Result<(), VerifyError> {
    fn perr(code: &'static str, detail: impl Into<String>) -> VerifyError {
        VerifyError { code, detail: detail.into() }
    }
    for name in &p.order {
        let f = &p.funcs[name];
        verify(f).map_err(|e| perr("P00", format!("fn '{}': {}", name, e)))?;
    }
    for name in &p.order {
        let f = &p.funcs[name];
        for id in f.cells() {
            let c = f.cell(id).expect("present");
            if let CellKind::Call { name: callee, ret_ty } = &c.kind {
                let g = p
                    .get(callee)
                    .ok_or_else(|| perr("P01", format!("call {} to unknown fn '{}'", id, callee)))?;
                // callee signature: params of the entry region, in order
                let params: Vec<crate::id::CellId> = f_entry_params(g);
                if c.operands.len() != params.len() {
                    return Err(perr(
                        "P02",
                        format!(
                            "call {} to '{}' passes {} args but callee takes {}",
                            id,
                            callee,
                            c.operands.len(),
                            params.len()
                        ),
                    ));
                }
                for (slot, (&arg, &param)) in c.operands.iter().zip(params.iter()).enumerate() {
                    let want = g.ty_of(param);
                    let got = f.ty_of(arg);
                    if want != got {
                        return Err(perr(
                            "P03",
                            format!(
                                "call {} arg {} is {} but callee '{}' wants {}",
                                id,
                                slot,
                                got.map(|t| t.name().to_string()).unwrap_or_else(|| "<no-type>".into()),
                                callee,
                                want.map(|t| t.name().to_string()).unwrap_or_else(|| "<no-type>".into())
                            ),
                        ));
                    }
                }
                // callee return type = type of its entry ret operand
                let ret_cell = g
                    .cells()
                    .find(|&rid| matches!(g.cell(rid).map(|rc| &rc.kind), Some(CellKind::Ret)))
                    .expect("verified fabric has a ret");
                let ret_op = g.cell(ret_cell).unwrap().operands.first().copied();
                let callee_ret = match ret_op {
                    Some(op) => g.ty_of(op),
                    None => {
                        return Err(perr(
                            "P04",
                            format!("call {} expects a value but fn '{}' returns void", id, callee),
                        ))
                    }
                };
                if callee_ret != Some(*ret_ty) {
                    return Err(perr(
                        "P04",
                        format!(
                            "call {} declares {} but fn '{}' returns {}",
                            id,
                            ret_ty.name(),
                            callee,
                            callee_ret.map(|t| t.name().to_string()).unwrap_or_else(|| "void".into())
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn f_entry_params(f: &Fabric) -> Vec<crate::id::CellId> {
    let entry = f.entry().expect("nonempty");
    f.region(entry)
        .map(|r| r.cells.iter().copied().collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|&id| matches!(f.cell(id).map(|c| &c.kind), Some(CellKind::Param { .. })))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Program {
        let text = "fabric v1\n\
fn main\n\
region entry\n\
  %0 = param i32\n\
  %1 = const i32 2\n\
  %2 = call i32 add2 %0, %1\n\
  %3 = ret %2\n\
fn add2\n\
region entry\n\
  %0 = param i32\n\
  %1 = param i32\n\
  %2 = arith.add i32 %0, %1\n\
  %3 = ret %2\n";
        parse(text).expect("sample program parses")
    }

    #[test]
    fn print_parse_print_roundtrip() {
        let p = sample();
        let once = print(&p);
        let p2 = parse(&once).expect("reparse");
        assert_eq!(print(&p2), once);
    }

    #[test]
    fn sample_verifies() {
        let p = sample();
        assert!(verify_program(&p).is_ok());
    }

    #[test]
    fn p01_unknown_callee() {
        let mut p = sample();
        let f = p.funcs.get_mut("main").unwrap();
        if let Some(c) = f.cell_mut(crate::id::CellId(2)) {
            if let CellKind::Call { name, .. } = &mut c.kind {
                name.push_str("_nope");
            }
        }
        let e = verify_program(&p).unwrap_err();
        assert_eq!(e.code, "P01");
    }

    #[test]
    fn p02_arity_mismatch() {
        // main calls add2 with one arg
        let text = "fabric v1\n\
fn main\n\
region entry\n\
  %0 = param i32\n\
  %1 = call i32 add2 %0\n\
  %2 = ret %1\n\
fn add2\n\
region entry\n\
  %0 = param i32\n\
  %1 = param i32\n\
  %2 = arith.add i32 %0, %1\n\
  %3 = ret %2\n";
        let p = parse(text).unwrap();
        let e = verify_program(&p).unwrap_err();
        assert_eq!(e.code, "P02");
    }

    #[test]
    fn p03_arg_type_mismatch() {
        let text = "fabric v1\n\
fn main\n\
region entry\n\
  %0 = param i64\n\
  %1 = call i32 add2 %0\n\
  %2 = ret %1\n\
fn add2\n\
region entry\n\
  %0 = param i32\n\
  %1 = const i32 7\n\
  %2 = arith.add i32 %0, %1\n\
  %3 = ret %2\n";
        let p = parse(text).unwrap();
        let e = verify_program(&p).unwrap_err();
        assert_eq!(e.code, "P03");
    }

    #[test]
    fn p04_return_type_disagreement() {
        let text = "fabric v1\n\
fn main\n\
region entry\n\
  %0 = param i32\n\
  %1 = call i64 addone %0\n\
  %2 = ret %1\n\
fn addone\n\
region entry\n\
  %0 = param i32\n\
  %1 = const i32 1\n\
  %2 = arith.add i32 %0, %1\n\
  %3 = ret %2\n";
        let p = parse(text).unwrap();
        let e = verify_program(&p).unwrap_err();
        assert_eq!(e.code, "P04");
    }

    #[test]
    fn fn_body_errors_carry_file_line_numbers() {
        let text = "fabric v1\n\
fn main\n\
region entry\n\
  %0 = bogus\n";
        let e = parse(text).unwrap_err();
        assert_eq!(e.line, 4, "offset-adjusted: {}", e);
    }
}

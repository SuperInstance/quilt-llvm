//! Provenance: from any value, reconstruct its full def chain by walking
//! operand wires backwards — and, with history, the transform story of
//! the cell (including cells the fabric no longer contains).
//!
//! This is experiment (a). The honest claim structure:
//! - use-def walking is ALSO easy in LLVM (operands() in C++); we do not
//!   claim otherwise;
//! - provenance of a DEAD value, and per-cell transform history, are
//!   things textual LLVM IR and llvm::Value do not carry at all.

use crate::cell::CellKind;
use crate::diff::History;
use crate::fabric::Fabric;
use crate::id::CellId;

#[derive(Clone, Debug, PartialEq)]
pub struct ProvNode {
    pub id: CellId,
    pub line: String,
    pub children: Vec<ProvNode>,
}

/// Walk the def chain of `target`. Every leaf is a param or a const (the
/// fabric's roots). Cycles are impossible in a verified fabric but are
/// detected anyway (defense in depth: Err, not a hang).
pub fn provenance(f: &Fabric, target: CellId) -> Result<ProvNode, String> {
    let mut visiting: Vec<CellId> = vec![];
    walk(f, target, &mut visiting, 0)
}

fn walk(f: &Fabric, id: CellId, visiting: &mut Vec<CellId>, depth: u32) -> Result<ProvNode, String> {
    let c = f.cell(id).ok_or_else(|| format!("no such cell {}", id))?;
    if depth > 100_000 {
        return Err(format!("provenance depth guard at {}", id));
    }
    if visiting.contains(&id) {
        return Err(format!("cycle in provenance through {}", id));
    }
    let line = crate::text::render_cell(f, id);
    match &c.kind {
        CellKind::Param { .. } | CellKind::Const { .. } => Ok(ProvNode { id, line, children: vec![] }),
        _ => {
            visiting.push(id);
            let mut children = vec![];
            for &op in &c.operands {
                children.push(walk(f, op, visiting, depth + 1)?);
            }
            visiting.pop();
            Ok(ProvNode { id, line, children })
        }
    }
}

/// Render as an indented tree. The root first, wires indented under users.
pub fn render(node: &ProvNode) -> String {
    let mut out = String::new();
    render_into(node, 0, &mut out);
    out
}

fn render_into(node: &ProvNode, depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str(&node.line);
    out.push('\n');
    for ch in &node.children {
        render_into(ch, depth + 1, out);
    }
}

/// Correctness check used by the corpus: the walk terminates, every leaf
/// is a root (param/const), and the node multiset stays within the fabric
/// (no invented cells).
pub fn check_prov(f: &Fabric, id: CellId) -> Result<(), String> {
    let node = provenance(f, id)?;
    let n_cells = f.cells().count();
    let mut leaves_ok = true;
    let mut count = 0usize;
    // The walk ROOT may itself be a terminator with no data wires (e.g. a
    // bare jump): its provenance is legitimately empty. Only nodes BELOW
    // the root must be value cells ending at param/const roots.
    let mut stack: Vec<&ProvNode> = node.children.iter().collect();
    count += 1;
    if f.cell(node.id).is_none() {
        return Err(format!("provenance invented cell {}", node.id));
    }
    while let Some(n) = stack.pop() {
        count += 1;
        if n.children.is_empty() {
            let is_root = f
                .cell(n.id)
                .map(|c| matches!(c.kind, CellKind::Param { .. } | CellKind::Const { .. }))
                .unwrap_or(false);
            if !is_root {
                leaves_ok = false;
            }
        } else if f.cell(n.id).map(|c| c.produces_value()) != Some(true) {
            return Err(format!("non-value cell {} inside provenance tree", n.id));
        }
        if f.cell(n.id).is_none() {
            return Err(format!("provenance invented cell {}", n.id));
        }
        stack.extend(n.children.iter());
    }
    if !leaves_ok {
        return Err(format!("provenance of {} has non-root leaves", id));
    }
    // tree expansion can revisit shared defs; bound is generous but finite
    if count > n_cells * n_cells.max(1) {
        return Err(format!("provenance of {} exploded: {} nodes", id, count));
    }
    Ok(())
}

/// Transform provenance: every history edit mentioning this cell, with
/// pass and epoch. Works for dropped cells — that is the point.
pub fn prov_history(h: &History, id: CellId) -> Vec<(u64, &'static str, String)> {
    h.mentions_of(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{ArithOp, Cell, CellKind};
    use crate::passes::{constfold::const_fold, dce::dce};
    use crate::ty::{ConstVal, Type};
    use crate::diff::History;

    fn diamond() -> Fabric {
        // like the text.rs sample: param + const feed cmp -> branch; two
        // arms; phi join; ret
        let text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = const i32 42\n\
  %2 = arith.add i32 %0, %1\n\
  %3 = cmp.lt %2, %1\n\
  %5 = br %3, then, else\n\
region then\n\
  %4 = const i64 1i64\n\
  %6 = jump join\n\
region else\n\
  %7 = const i64 2i64\n\
  %8 = jump join\n\
region join\n\
  %9 = phi [then: %4] [else: %7]\n\
  %10 = ret %9\n";
        crate::text::parse(text).expect("sample parses")
    }

    #[test]
    fn provenance_of_phi_reaches_all_roots() {
        let f = diamond();
        let node = provenance(&f, CellId(9)).expect("walk");
        let r = render(&node);
        assert!(r.contains("phi"), "{}", r);
        assert!(r.contains("const i64 1i64"), "must reach then-arm const: {}", r);
        assert!(r.contains("const i64 2i64"), "must reach else-arm const: {}", r);
        // Data provenance does NOT cross control edges: the param feeds the
        // branch (a control wire), not the phi. Booked as a v0 limitation.
        assert!(!r.contains("param i32"), "data walk must not invent control deps: {}", r);
        assert!(check_prov(&f, CellId(9)).is_ok());
    }

    #[test]
    fn provenance_of_const_is_itself() {
        let f = diamond();
        let node = provenance(&f, CellId(1)).expect("walk");
        assert!(node.children.is_empty());
        assert!(node.line.contains("42"));
    }

    #[test]
    fn missing_cell_is_an_error_not_a_panic() {
        let f = diamond();
        assert!(provenance(&f, CellId(99)).is_err());
    }

    #[test]
    fn history_provenance_tracks_dropped_cells() {
        // fold + dce a fabric, then ask history about a cell that no
        // longer exists
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let c1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(2) }));
        let c2 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(3) }));
        let dead = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I64, val: ConstVal::I64(9) }));
        let mut a = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a.operands = vec![c1, c2];
        let a = f.add_cell(e, a);
        let mut a2 = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a2.operands = vec![p, a];
        let a2 = f.add_cell(e, a2);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![a2];
        f.add_cell(e, r);

        let mut h = History::new();
        let (g1, rec1) = const_fold(&f).unwrap();
        h.push(rec1);
        let (g2, rec2) = dce(&g1).unwrap();
        h.push(rec2);

        // %3 (a, the folded add) is gone from the final fabric
        assert!(g2.cell(a).is_none());
        // ...but history tells its full story
        let story = prov_history(&h, a);
        let text = format!("{:?}", story);
        assert!(text.contains("removed"), "{}", text);
        assert!(text.contains("folded into"), "{}", text);
        // and the dead const's ledger entry is queryable the same way
        let dead_story = prov_history(&h, dead);
        let dt = format!("{:?}", dead_story);
        assert!(dt.contains("dead"), "{}", dt);
        // prov walk in the FINAL fabric from ret reaches the folded const
        let ret_id = g2.cells().find(|&id| matches!(g2.cell(id).map(|c| &c.kind), Some(CellKind::Ret))).unwrap();
        let node = provenance(&g2, g2.cell(ret_id).unwrap().operands[0]).unwrap();
        let rendered = render(&node);
        assert!(rendered.contains("const i32 5"), "2+3 folded to 5: {}", rendered);
        assert!(rendered.contains("param i32"), "chain still reaches param: {}", rendered);
    }

    #[test]
    fn cmp_provenance_shows_both_operands() {
        let f = diamond();
        let node = provenance(&f, CellId(3)).unwrap();
        assert_eq!(node.children.len(), 2);
        assert!(check_prov(&f, CellId(3)).is_ok());
    }
}

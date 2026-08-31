//! Provenance: from any value, reconstruct its full def chain by walking
//! operand wires backwards — and, with history, the transform story of
//! the cell (including cells the fabric no longer contains).
//!
//! This is experiment (a). The honest claim structure:
//! - use-def walking is ALSO easy in LLVM (operands() in C++); we do not
//!   claim otherwise;
//! - provenance of a DEAD value, and per-cell transform history, are
//!   things textual LLVM IR and llvm::Value do not carry at all.
//!
//! M4.1 (tit-quilt retrofit): the walk resolves THROUGH TOMBSTONES. A
//! forgotten cell renders as its hash-only record (`<forgotten …>`)
//! and the walk continues down the tombstone's witness list — the
//! provenance-integrity law: nothing witness-referenced is ever
//! destroyed, so no walk ever dead-ends at a decay kill.

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
/// fabric's roots) — a tombstoned param/const is an equally good leaf.
/// Cycles are impossible in a verified fabric but are detected anyway
/// (defense in depth: Err, not a hang).
pub fn provenance(f: &Fabric, target: CellId) -> Result<ProvNode, String> {
    let mut visiting: Vec<CellId> = vec![];
    walk(f, target, &mut visiting, 0)
}

fn walk(f: &Fabric, id: CellId, visiting: &mut Vec<CellId>, depth: u32) -> Result<ProvNode, String> {
    if depth > 100_000 {
        return Err(format!("provenance depth guard at {}", id));
    }
    if visiting.contains(&id) {
        return Err(format!("cycle in provenance through {}", id));
    }
    if f.cell(id).is_none() {
        // M4.1: the graveyard answers before the walk dead-ends
        if let Some(tb) = f.tombstone_of(id) {
            visiting.push(id);
            let mut children = vec![];
            for &w in &tb.witness {
                children.push(walk(f, w, visiting, depth + 1)?);
            }
            visiting.pop();
            return Ok(ProvNode {
                id,
                line: format!(
                    "{} = {} <forgotten tick={} vhash={:#018x}>",
                    tb.cell, tb.kind, tb.tick, tb.vhash
                ),
                children,
            });
        }
        return Err(format!("no such cell {}", id));
    }
    let c = f.cell(id).expect("just checked");
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
    let n_cells = f.cells().count() + f.tombstones.len();
    let mut leaves_ok = true;
    let mut count = 0usize;
    // The walk ROOT may itself be a terminator with no data wires (e.g. a
    // bare jump): its provenance is legitimately empty. Only nodes BELOW
    // the root must be value cells ending at param/const roots. M4.1: a
    // tombstoned leaf is an equally good root iff it is a root KIND with
    // an empty witness list (a forgotten const/param).
    let mut stack: Vec<&ProvNode> = node.children.iter().collect();
    count += 1;
    if f.cell(node.id).is_none() && f.tombstone_of(node.id).is_none() {
        return Err(format!("provenance invented cell {}", node.id));
    }
    while let Some(n) = stack.pop() {
        count += 1;
        let tombed = f.tombstone_of(n.id);
        if f.cell(n.id).is_none() && tombed.is_none() {
            return Err(format!("provenance invented cell {}", n.id));
        }
        if n.children.is_empty() {
            let is_root = match (&f.cell(n.id), &tombed) {
                (Some(c), _) => matches!(c.kind, CellKind::Param { .. } | CellKind::Const { .. }),
                (None, Some(tb)) => {
                    tb.witness.is_empty() && (tb.kind == "param" || tb.kind == "const")
                }
                (None, None) => false,
            };
            if !is_root {
                leaves_ok = false;
            }
        } else if let Some(c) = f.cell(n.id) {
            if !c.produces_value() {
                return Err(format!("non-value cell {} inside provenance tree", n.id));
            }
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

#[cfg(test)]
mod tombstone_walk_tests {
    //! M4.1 — provenance resolves through the graveyard (tit-quilt
    //! retrofit): a forgotten cell's walk renders its hash-only record
    //! and follows the tombstone witness list to the roots.

    use super::*;
    use crate::cell::{ArithOp, Cell, CellKind};
    use crate::decay::{dce_decay, DeathCert};
    use crate::manager::TickCtx;
    use crate::ty::{ConstVal, Type};

    /// entry: %0=param ; %1=arith.add %0,%0 ; %2=ret %1 ; then a dead
    /// island: %3=const i32 7 ; %4=arith.add %3,%0 (no users, not
    /// foldable — only decay can kill it)
    fn dead_island() -> (Fabric, CellId, CellId, CellId) {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let mut live = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        live.operands = vec![p, p];
        let live = f.add_cell(e, live);
        let d1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(7) }));
        let mut da = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        da.operands = vec![d1, p];
        let da = f.add_cell(e, da);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![live];
        f.add_cell(e, r);
        (f, p, d1, da)
    }

    #[test]
    fn green_provenance_of_a_forgotten_value_resolves_through_tombstones() {
        let (f, p, d1, da) = dead_island();
        let (g, _rec) = dce_decay(&f, &TickCtx { tick: 1 }).expect("decay");
        assert!(g.cell(da).is_none(), "the add is gone from the slab");
        // pre-M4.1 this walk dead-ended ("no such cell")
        let node = provenance(&g, da).expect("the graveyard answers");
        let text = render(&node);
        assert!(
            text.contains("<forgotten tick=1"),
            "the tombstone record renders: {}",
            text
        );
        assert!(text.contains("arith"), "{}", text);
        // the walk follows the witness list: the dead const (itself a
        // tombstone) and the LIVE param both appear
        assert!(text.contains("const"), "{}", text);
        assert!(text.contains("param i32"), "{}", text);
        assert_eq!(node.children.len(), 2, "witness = [dead const, live param]");
        assert!(check_prov(&g, da).is_ok(), "leaves are roots, live or tombstoned");
        // the const's tombstone is a leaf: hash-only, no children
        let cn = provenance(&g, d1).expect("const tombstone");
        assert!(cn.children.is_empty());
        assert!(cn.line.contains("<forgotten"), "{}", cn.line);
        let _ = p;
    }

    #[test]
    fn green_certificates_and_graveyard_agree() {
        // the tombstone's hash IS the certificate's hash: the ledger line,
        // the graveyard, and the pre-tick fabric all describe one cell
        let (f, _p, d1, da) = dead_island();
        let (g, rec) = dce_decay(&f, &TickCtx { tick: 0 }).expect("decay");
        for e in &rec.edits {
            if let crate::diff::Edit::RemoveCell { id, ledger, .. } = e {
                let cert = DeathCert::parse(ledger, *id).expect("certified");
                let tb = g.tombstone_of(*id).expect("tombstoned");
                assert_eq!(cert.vhash, tb.vhash, "ledger and graveyard agree");
                assert_eq!(cert.witness, tb.witness);
                assert_eq!(cert.vhash, crate::sign::fnv1a64(crate::text::render_cell(&f, *id).as_bytes()));
            }
        }
        let _ = (d1, da);
    }
}

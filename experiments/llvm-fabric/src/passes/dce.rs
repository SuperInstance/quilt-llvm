//! Dead code elimination: a cell is live if it is a terminator or feeds
//! one (transitively through operand wires). Everything else is removed
//! WITH a ledger entry.
//!
//! v0 scope (honest): unreachable REGIONS are not removed (their
//! terminators count as roots); that's region-DCE, deferred to v1.

use crate::cell::CellKind;
use crate::diff::{DiffRecord, Edit};
use crate::fabric::Fabric;
use crate::id::CellId;
use crate::verify::verify;
use std::collections::HashSet;

pub fn dce(f: &Fabric) -> Result<(Fabric, DiffRecord), String> {
    if let Err(e) = verify(f) {
        return Err(format!("dce refuses unverified input: {}", e));
    }
    let mut g = f.clone();
    let mut rec = DiffRecord::new("dce");

    // 1. Roots: all terminators (branch/jump/ret), everywhere.
    let mut live: HashSet<CellId> = HashSet::new();
    let mut work: Vec<CellId> = vec![];
    for id in g.cells() {
        if g.cell(id).expect("present").is_terminator() {
            live.insert(id);
            work.push(id);
        }
    }
    // 2. Backward closure along operand wires.
    while let Some(id) = work.pop() {
        if let Some(c) = g.cell(id) {
            for &op in &c.operands {
                if live.insert(op) {
                    work.push(op);
                }
            }
        }
    }
    // 3. Everything else is dead. Deterministic removal order:
    //    region order, then cell order.
    for ri in 0..g.regions.len() {
        let ids: Vec<CellId> = g.regions[ri].cells.clone();
        for id in ids {
            if live.contains(&id) {
                continue;
            }
            let summary = crate::text::render_cell(&g, id);
            let region = g.cell(id).expect("present").region;
            let cells = &mut g.regions[region.0 as usize].cells;
            let pos = cells.iter().position(|&c| c == id).expect("listed");
            cells.remove(pos);
            g.slab[id.0 as usize] = None;
            rec.edits.push(Edit::RemoveCell {
                id,
                ledger: "dead: no path to a terminator".into(),
                summary,
            });
        }
    }
    Ok((g, rec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{ArithOp, Cell};
    use crate::conserve;
    use crate::ty::{ConstVal, Type};

    fn fabric_with_dead_code() -> (Fabric, CellId, CellId) {
        // %0=param ; %1=const(dead) ; %2=add(%0,%1)(dead) ; %3=add(%0,%0)(live) ; %4=ret(%3)
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let c = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let mut dead_add = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        dead_add.operands = vec![p, c];
        let dead_add = f.add_cell(e, dead_add);
        let mut live_add = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        live_add.operands = vec![p, p];
        let live_add = f.add_cell(e, live_add);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![live_add];
        f.add_cell(e, r);
        (f, c, dead_add)
    }

    #[test]
    fn green_removes_dead_keeps_live() {
        let (f, dead_const, dead_add) = fabric_with_dead_code();
        assert!(verify(&f).is_ok());
        let (g, rec) = dce(&f).expect("dce");
        // red condition: identity output fails these
        assert_ne!(g, f, "dce must change this fabric (red without the pass)");
        assert!(g.cell(dead_const).is_none(), "dead const must go");
        assert!(g.cell(dead_add).is_none(), "dead add must go");
        assert!(g.cell(CellId(3)).is_some(), "live add stays");
        assert!(verify(&g).is_ok(), "post-dce fabric must verify");
        assert!(conserve::check(&f, &g, &rec).is_ok(), "conservation must hold");
        let ledgers: Vec<&String> = rec
            .edits
            .iter()
            .filter_map(|e| match e {
                Edit::RemoveCell { ledger, .. } => Some(ledger),
                _ => None,
            })
            .collect();
        assert_eq!(ledgers.len(), 2, "exactly the two dead cells, both ledgered");
        assert!(ledgers.iter().all(|l| l.contains("dead")));
    }

    #[test]
    fn red_no_dead_code_is_identity() {
        // everything live: output identical, empty diff
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let mut a = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a.operands = vec![p, p];
        let a = f.add_cell(e, a);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![a];
        f.add_cell(e, r);
        let (g, rec) = dce(&f).expect("dce");
        assert_eq!(g, f, "all-live fabric must come back identical");
        assert!(rec.is_empty(), "diff must be empty");
    }

    #[test]
    fn dead_phi_goes_its_operands_follow() {
        // phi with no users: phi + its operands are all dead
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let b = f.add_region("b");
        let bi1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }));
        let v1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let mut br = Cell::new(e, CellKind::Branch { then_r: b, else_r: b });
        br.operands = vec![bi1];
        f.add_cell(e, br);
        let mut phi = Cell::new(b, CellKind::Phi { joins: vec![e] });
        phi.operands = vec![v1];
        let phi = f.add_cell(b, phi);
        f.add_cell(b, Cell::new(b, CellKind::Ret)); // ret void — phi unused
        assert!(verify(&f).is_ok());
        let (g, rec) = dce(&f).expect("dce");
        assert!(g.cell(phi).is_none(), "dead phi removed");
        assert!(g.cell(v1).is_none(), "phi's dead operand removed too");
        assert!(verify(&g).is_ok());
        assert!(conserve::check(&f, &g, &rec).is_ok());
    }

    #[test]
    fn refuses_unverified_input() {
        let f = Fabric::empty(); // V00
        let err = dce(&f).unwrap_err();
        assert!(err.contains("refuses unverified"));
    }

    #[test]
    fn terminators_are_never_dead() {
        let (f, _, _) = fabric_with_dead_code();
        let (g, _) = dce(&f).expect("dce");
        let ret = g.cell(CellId(4)).expect("ret is a root");
        assert!(matches!(ret.kind, CellKind::Ret));
        assert_eq!(g.regions[0].cells.last(), Some(&CellId(4)), "terminator stays last");
    }
}

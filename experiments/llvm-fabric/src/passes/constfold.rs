//! Constant folding: arith/cmp cells whose operands are all constants
//! are replaced by a const cell in the same region, at the folded cell's
//! position; every use is retargeted; the folded cell is removed WITH a
//! ledger entry (conservation law).
//!
//! v0 scope (honest): arith and cmp only. Folding a const branch into a
//! jump requires phi-join maintenance (the not-taken region loses a
//! predecessor edge, so phis there and downstream must drop joins), and
//! that CFG surgery is deferred to v1. Booked in docs/EXPERIMENTS.md.
//!
//! Integer folding is checked: overflow or INT_MIN / -1 skips the fold
//! (the cell stays, honestly unfused). Float folds that would produce
//! NaN are skipped too (NaN is not representable in the v0 text format).

use crate::cell::{ArithOp, Cell, CellKind, CmpOp};
use crate::diff::{DiffRecord, Edit};
use crate::fabric::Fabric;
use crate::id::CellId;
use crate::ty::{ConstVal, Type};
use crate::verify::verify;

/// Fold one step of arithmetic. None = do not fold (overflow, div/0, NaN).
fn eval_arith(op: ArithOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
    use ConstVal::*;
    match (op, a, b) {
        (ArithOp::Add, I32(x), I32(y)) => x.checked_add(y).map(I32),
        (ArithOp::Add, I64(x), I64(y)) => x.checked_add(y).map(I64),
        (ArithOp::Add, F64(x), F64(y)) => {
            let r = x + y;
            if r.is_nan() { None } else { Some(F64(r)) }
        }
        (ArithOp::Sub, I32(x), I32(y)) => x.checked_sub(y).map(I32),
        (ArithOp::Sub, I64(x), I64(y)) => x.checked_sub(y).map(I64),
        (ArithOp::Sub, F64(x), F64(y)) => {
            let r = x - y;
            if r.is_nan() { None } else { Some(F64(r)) }
        }
        (ArithOp::Mul, I32(x), I32(y)) => x.checked_mul(y).map(I32),
        (ArithOp::Mul, I64(x), I64(y)) => x.checked_mul(y).map(I64),
        (ArithOp::Mul, F64(x), F64(y)) => {
            let r = x * y;
            if r.is_nan() { None } else { Some(F64(r)) }
        }
        (ArithOp::Div, I32(x), I32(y)) => x.checked_div(y).map(I32),
        (ArithOp::Div, I64(x), I64(y)) => x.checked_div(y).map(I64),
        (ArithOp::Div, F64(x), F64(y)) => {
            let r = x / y;
            if r.is_nan() { None } else { Some(F64(r)) }
        }
        _ => None, // type mismatch should not reach here (verified input)
    }
}

fn eval_cmp(op: CmpOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
    use ConstVal::*;
    let r = match (op, a, b) {
        (CmpOp::Eq, I1(x), I1(y)) => x == y,
        (CmpOp::Ne, I1(x), I1(y)) => x != y,
        (CmpOp::Eq, I32(x), I32(y)) => x == y,
        (CmpOp::Ne, I32(x), I32(y)) => x != y,
        (CmpOp::Lt, I32(x), I32(y)) => x < y,
        (CmpOp::Le, I32(x), I32(y)) => x <= y,
        (CmpOp::Gt, I32(x), I32(y)) => x > y,
        (CmpOp::Ge, I32(x), I32(y)) => x >= y,
        (CmpOp::Eq, I64(x), I64(y)) => x == y,
        (CmpOp::Ne, I64(x), I64(y)) => x != y,
        (CmpOp::Lt, I64(x), I64(y)) => x < y,
        (CmpOp::Le, I64(x), I64(y)) => x <= y,
        (CmpOp::Gt, I64(x), I64(y)) => x > y,
        (CmpOp::Ge, I64(x), I64(y)) => x >= y,
        (CmpOp::Eq, F64(x), F64(y)) => x == y,
        (CmpOp::Ne, F64(x), F64(y)) => x != y,
        (CmpOp::Lt, F64(x), F64(y)) => x < y,
        (CmpOp::Le, F64(x), F64(y)) => x <= y,
        (CmpOp::Gt, F64(x), F64(y)) => x > y,
        (CmpOp::Ge, F64(x), F64(y)) => x >= y,
        _ => return None,
    };
    Some(I1(r))
}

fn const_val(f: &Fabric, id: CellId) -> Option<ConstVal> {
    match f.cell(id)?.kind {
        CellKind::Const { val, .. } => Some(val),
        _ => None,
    }
}

/// Pure transform. Requires a verified fabric; returns the folded fabric
/// and the diff record (possibly empty when nothing was foldable).
pub fn const_fold(f: &Fabric) -> Result<(Fabric, DiffRecord), String> {
    if let Err(e) = verify(f) {
        return Err(format!("const_fold refuses unverified input: {}", e));
    }
    let mut g = f.clone();
    let mut rec = DiffRecord::new("constfold");
    let mut changed = true;
    let mut guard = 0u32;
    while changed {
        changed = false;
        guard += 1;
        if guard > 10_000 {
            return Err("constfold exceeded fixpoint guard (10k sweeps)".into());
        }
        // Deterministic order: region order, then cell order.
        for ri in 0..g.regions.len() as u32 {
            let ids: Vec<CellId> = g.regions[ri as usize].cells.clone();
            for id in ids {
                let (kind, operands) = match g.cell(id) {
                    Some(c) => (c.kind.clone(), c.operands.clone()),
                    None => continue,
                };
                let folded: Option<ConstVal> = match &kind {
                    CellKind::Arith { op, .. } => {
                        let (a, b) = match (const_val(&g, operands[0]), const_val(&g, operands[1])) {
                            (Some(a), Some(b)) => (a, b),
                            _ => continue,
                        };
                        eval_arith(*op, a, b)
                    }
                    CellKind::Cmp { op } => {
                        let (a, b) = match (const_val(&g, operands[0]), const_val(&g, operands[1])) {
                            (Some(a), Some(b)) => (a, b),
                            _ => continue,
                        };
                        eval_cmp(*op, a, b)
                    }
                    _ => continue,
                };
                let val = match folded {
                    Some(v) => v,
                    None => continue, // not foldable (overflow/div0/NaN) — stays
                };
                let region = g.cell(id).expect("present").region;
                let ty = val.ty();
                let index = g
                    .index_in_region(id)
                    .expect("verified fabric lists its cells");
                // 1. add the new const at the folded cell's position
                let new_id = g.insert_cell(
                    region,
                    index,
                    Cell::new(region, CellKind::Const { ty, val }),
                );
                rec.edits.push(Edit::AddCell {
                    id: new_id,
                    index,
                    cell: Cell::new(region, CellKind::Const { ty, val }),
                });
                // 2. retarget every use (deterministic: uses_of scans asc)
                for (user, slot) in g.uses_of(id) {
                    let c = g.cell_mut(user).expect("present");
                    let from = c.operands[slot as usize];
                    c.operands[slot as usize] = new_id;
                    rec.edits.push(Edit::Retarget { cell: user, slot, from, to: new_id });
                }
                // 3. remove the folded cell, with ledger
                let summary = crate::text::render_cell(&g, id);
                let cells = &mut g.regions[region.0 as usize].cells;
                let pos = cells.iter().position(|&c| c == id).expect("listed");
                cells.remove(pos);
                g.slab[id.0 as usize] = None;
                rec.edits.push(Edit::RemoveCell {
                    id,
                    ledger: format!("folded into {}", new_id),
                    summary,
                });
                changed = true;
            }
        }
    }
    Ok((g, rec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conserve;
    use crate::id::RegionId;

    fn entry_fabric() -> (Fabric, RegionId) {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        (f, e)
    }

    fn consts_and_ret(f: &mut Fabric, e: RegionId) -> (CellId, CellId) {
        let a = f.add_cell(
            e,
            Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(2) }),
        );
        let b = f.add_cell(
            e,
            Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(3) }),
        );
        (a, b)
    }

    #[test]
    fn green_folds_add_and_cascades() {
        // %0=2 %1=3 %2=add(%0,%1) %3=add(%2,%1) %4=ret(%3)  =>  5 then 8
        let (mut f, e) = entry_fabric();
        let (a, b) = consts_and_ret(&mut f, e);
        let mut add1 = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        add1.operands = vec![a, b];
        let add1 = f.add_cell(e, add1);
        let mut add2 = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        add2.operands = vec![add1, b];
        let add2 = f.add_cell(e, add2);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![add2];
        f.add_cell(e, r);
        assert!(verify(&f).is_ok(), "test fabric must verify first");

        let (g, rec) = const_fold(&f).expect("fold");
        // red condition: identity output fails all of these
        assert_ne!(g, f, "const_fold must change this fabric (red without the pass)");
        // find the final const feeding ret
        let ret_id = g
            .cells()
            .find(|&id| matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Ret)))
            .expect("ret present");
        let ret_cell = g.cell(ret_id).expect("ret still present");
        let fed = ret_cell.operands[0];
        match &g.cell(fed).expect("present").kind {
            CellKind::Const { ty: Type::I32, val: ConstVal::I32(8) } => {}
            other => panic!("ret must be fed by const 8, got {:?}", other),
        }
        assert!(g.cell(add1).is_none() && g.cell(add2).is_none(), "folded cells removed");
        // conservation + ledger + verified output
        assert!(conserve::check(&f, &g, &rec).is_ok(), "conservation must hold");
        assert!(verify(&g).is_ok(), "folded fabric must verify");
        let ledgered = rec.edits.iter().any(|e| matches!(e, Edit::RemoveCell { ledger, .. } if ledger.contains("folded into")));
        assert!(ledgered, "removals must carry ledger entries");
        // both original adds removed with ledger
        let removals = rec.edits.iter().filter(|e| matches!(e, Edit::RemoveCell { .. })).count();
        assert_eq!(removals, 2);
    }

    #[test]
    fn green_folds_cmp() {
        let (mut f, e) = entry_fabric();
        let (a, b) = consts_and_ret(&mut f, e); // 2, 3
        let mut cmp = Cell::new(e, CellKind::Cmp { op: CmpOp::Lt });
        cmp.operands = vec![a, b];
        let cmp = f.add_cell(e, cmp);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![cmp];
        f.add_cell(e, r);
        let (g, _rec) = const_fold(&f).expect("fold");
        let fed = g.cell(CellId(3)).unwrap().operands[0];
        assert_eq!(g.cell(fed).unwrap().kind, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) });
        assert!(g.cell(cmp).is_none());
    }

    #[test]
    fn red_param_chain_is_untouched() {
        // nothing foldable: pass must return identical fabric + empty diff
        let (mut f, e) = entry_fabric();
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let c = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let mut a = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a.operands = vec![p, c];
        let a = f.add_cell(e, a);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![a];
        f.add_cell(e, r);
        let (g, rec) = const_fold(&f).expect("fold");
        assert_eq!(g, f, "no foldable cells => identical output");
        assert!(rec.is_empty(), "diff must be empty");
    }

    #[test]
    fn div_by_zero_const_is_left_alone() {
        let (mut f, e) = entry_fabric();
        let a = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(5) }));
        let z = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(0) }));
        let mut d = Cell::new(e, CellKind::Arith { op: ArithOp::Div, ty: Type::I32 });
        d.operands = vec![a, z];
        let d = f.add_cell(e, d);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![d];
        f.add_cell(e, r);
        let (g, rec) = const_fold(&f).expect("fold");
        assert_eq!(g, f, "5/0 must NOT fold in v0");
        assert!(rec.is_empty());
        assert!(verify(&g).is_ok());
    }

    #[test]
    fn overflow_skips_fold() {
        let (mut f, e) = entry_fabric();
        let a = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(i32::MAX) }));
        let b = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let mut d = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        d.operands = vec![a, b];
        let d = f.add_cell(e, d);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![d];
        f.add_cell(e, r);
        let (g, _rec) = const_fold(&f).expect("fold");
        assert!(g.cell(d).is_some(), "i32::MAX + 1 must not fold (checked)");
    }

    #[test]
    fn refuses_unverified_input() {
        let mut f = Fabric::empty(); // no regions -> V00
        let err = const_fold(&f).unwrap_err();
        assert!(err.contains("refuses unverified"));
    }

    #[test]
    fn folds_inside_phi_operands_stay_in_scope() {
        // phi joins value from a region where a fold happens; the folded
        // const must be placed in the folded cell's region so V07 holds.
        let (mut f, e) = entry_fabric();
        let b = f.add_region("b");
        let (a, c) = consts_and_ret(&mut f, e); // 2,3 in entry
        let mut add1 = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        add1.operands = vec![a, c];
        let add1 = f.add_cell(e, add1);
        let mut br = Cell::new(e, CellKind::Branch { then_r: b, else_r: b });
        br.operands = vec![f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }))];
        f.add_cell(e, br);
        let vb = f.add_cell(b, Cell::new(b, CellKind::Const { ty: Type::I32, val: ConstVal::I32(9) }));
        let mut phi = Cell::new(b, CellKind::Phi { joins: vec![e] });
        phi.operands = vec![add1];
        let phi = f.add_cell(b, phi);
        let _ = vb;
        let mut r = Cell::new(b, CellKind::Ret);
        r.operands = vec![phi];
        f.add_cell(b, r);
        assert!(verify(&f).is_ok());
        let (g, rec) = const_fold(&f).expect("fold");
        assert!(verify(&g).is_ok(), "folded fabric must still verify (V07 scope)");
        assert!(conserve::check(&f, &g, &rec).is_ok());
        // phi now fed by const 5 in entry
        let phi_cell = g.cell(phi).expect("phi survives");
        let fed = phi_cell.operands[0];
        assert_eq!(
            g.cell(fed).unwrap().kind,
            CellKind::Const { ty: Type::I32, val: ConstVal::I32(5) }
        );
    }
}

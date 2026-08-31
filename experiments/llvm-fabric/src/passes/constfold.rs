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
#[cfg(test)]
use crate::ty::Type;
use crate::ty::ConstVal;
use crate::verify::verify;

/// Fold one step of arithmetic. None = do not fold (overflow, div/0, NaN).
pub(crate) fn eval_arith(op: ArithOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
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

pub(crate) fn eval_cmp(op: CmpOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
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
                // 2. retarget every use (deterministic: the users row is
                //    user-asc, slot-asc — same order the scan produced)
                for (user, slot) in g.uses_of(id).to_vec() {
                    let from = g.retarget(user, slot, new_id).expect("present user");
                    rec.edits.push(Edit::Retarget { cell: user, slot, from, to: new_id });
                }
                // 3. remove the folded cell, with ledger
                let summary = crate::text::render_cell(&g, id);
                g.remove_cell(id).expect("folded cell listed");
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
        let f = Fabric::empty(); // no regions -> V00
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

/// The fold-table oracle (NEXT-PHASE.md R1 lane 1).
///
/// The pass suite proves the *machinery* around folding — conservation,
/// replay, death certificates. It does not judge the arithmetic itself:
/// a sabotage battery measured at `90d38b0` showed **5 of 7** one-line
/// fold-table corruptions surviving the entire 121-test suite, the
/// 10,000-fabric corpus included (docs/phase/NEXT-PHASE.md §2).
///
/// This module supplies the missing judge. It is deliberately NOT a
/// second fold table: it compares `eval_arith` / `eval_cmp` against
/// **Rust's own checked arithmetic and comparison operators** over a
/// swept operand grid. The oracle is the language's semantics, not a
/// restatement of ours.
///
/// The audit functions take the table under test as a parameter so the
/// oracle can be pointed at deliberately corrupted tables. That is what
/// `the_seven_sabotage_battery_is_caught` does: it is the D1 red/green
/// proof that this oracle detects what it claims to detect.
#[cfg(test)]
mod oracle {
    use super::*;
    use crate::ty::ConstVal::{self, *};

    pub type ArithFn = fn(ArithOp, ConstVal, ConstVal) -> Option<ConstVal>;
    pub type CmpFn = fn(CmpOp, ConstVal, ConstVal) -> Option<ConstVal>;

    /// Strict equality for expected-vs-actual fold results.
    ///
    /// f64 is compared by BIT PATTERN, not `PartialEq`. `ty.rs` warns
    /// that `0.0 == -0.0` under `PartialEq`; a fold that lost the sign
    /// of zero would slip past a `==` oracle. This closes the booked
    /// float-equality caveat (EXPERIMENTS.md §5.9) for the fold table.
    fn same(a: Option<ConstVal>, b: Option<ConstVal>) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(F64(x)), Some(F64(y))) => x.to_bits() == y.to_bits(),
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }

    fn grid_i32() -> Vec<i32> {
        vec![0, 1, -1, 2, -2, 3, -3, 7, 42, -42, 1000, i32::MAX, i32::MIN, i32::MAX - 1, i32::MIN + 1]
    }
    fn grid_i64() -> Vec<i64> {
        vec![0, 1, -1, 2, -2, 3, -3, 7, 42, -42, 1000, i64::MAX, i64::MIN, i64::MAX - 1, i64::MIN + 1]
    }
    /// Every value here is representable in the text format. NaN is NOT
    /// an input (unparseable by design, EXPERIMENTS.md §5.8) — but NaN
    /// *results* are reachable from these inputs (inf + -inf, inf * 0,
    /// 0/0) and the oracle requires the fold to refuse them.
    fn grid_f64() -> Vec<f64> {
        vec![
            0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 2.0, -3.5, 1e308, -1e308,
            f64::INFINITY, f64::NEG_INFINITY, f64::MAX, f64::MIN, f64::EPSILON,
        ]
    }
    fn grid_i1() -> Vec<bool> {
        vec![false, true]
    }

    /// The independent expectation for float folding: IEEE arithmetic,
    /// except that a NaN result is refused (the format cannot print it).
    /// Honest caveat: the NaN-refusal half restates a documented policy
    /// rather than deriving it, so the f64 leg is weaker evidence than
    /// the integer legs, where `checked_*` is a genuinely separate
    /// implementation.
    fn expect_f64(r: f64) -> Option<ConstVal> {
        if r.is_nan() { None } else { Some(F64(r)) }
    }

    fn disagree(
        op: &str,
        a: ConstVal,
        b: ConstVal,
        got: Option<ConstVal>,
        want: Option<ConstVal>,
    ) -> String {
        format!("{op} {a:?} {b:?}: table says {got:?}, Rust says {want:?}")
    }

    /// Judge an arith table against Rust. `Err` names the first
    /// disagreement.
    pub fn audit_arith(f: ArithFn) -> Result<(), String> {
        let ops = [
            (ArithOp::Add, "arith.add"),
            (ArithOp::Sub, "arith.sub"),
            (ArithOp::Mul, "arith.mul"),
            (ArithOp::Div, "arith.div"),
        ];
        for (op, name) in ops {
            for &x in &grid_i32() {
                for &y in &grid_i32() {
                    let want = match op {
                        ArithOp::Add => x.checked_add(y),
                        ArithOp::Sub => x.checked_sub(y),
                        ArithOp::Mul => x.checked_mul(y),
                        ArithOp::Div => x.checked_div(y),
                    }
                    .map(I32);
                    let got = f(op, I32(x), I32(y));
                    if !same(got, want) {
                        return Err(disagree(name, I32(x), I32(y), got, want));
                    }
                }
            }
            for &x in &grid_i64() {
                for &y in &grid_i64() {
                    let want = match op {
                        ArithOp::Add => x.checked_add(y),
                        ArithOp::Sub => x.checked_sub(y),
                        ArithOp::Mul => x.checked_mul(y),
                        ArithOp::Div => x.checked_div(y),
                    }
                    .map(I64);
                    let got = f(op, I64(x), I64(y));
                    if !same(got, want) {
                        return Err(disagree(name, I64(x), I64(y), got, want));
                    }
                }
            }
            for &x in &grid_f64() {
                for &y in &grid_f64() {
                    let want = expect_f64(match op {
                        ArithOp::Add => x + y,
                        ArithOp::Sub => x - y,
                        ArithOp::Mul => x * y,
                        ArithOp::Div => x / y,
                    });
                    let got = f(op, F64(x), F64(y));
                    if !same(got, want) {
                        return Err(disagree(name, F64(x), F64(y), got, want));
                    }
                }
            }
            // i1 has no arithmetic, and mixed widths never fold.
            for &x in &grid_i1() {
                for &y in &grid_i1() {
                    let got = f(op, I1(x), I1(y));
                    if !same(got, None) {
                        return Err(disagree(name, I1(x), I1(y), got, None));
                    }
                }
            }
            for (a, b) in [
                (I32(1), I64(1)),
                (I64(1), I32(1)),
                (I32(1), F64(1.0)),
                (F64(1.0), I32(1)),
                (I1(true), I32(1)),
                (I32(1), I1(true)),
                (I64(1), F64(1.0)),
            ] {
                let got = f(op, a, b);
                if !same(got, None) {
                    return Err(disagree(name, a, b, got, None));
                }
            }
        }
        Ok(())
    }

    /// Judge a cmp table against Rust. `Err` names the first
    /// disagreement.
    pub fn audit_cmp(f: CmpFn) -> Result<(), String> {
        let ops = [
            (CmpOp::Eq, "cmp.eq"),
            (CmpOp::Ne, "cmp.ne"),
            (CmpOp::Lt, "cmp.lt"),
            (CmpOp::Le, "cmp.le"),
            (CmpOp::Gt, "cmp.gt"),
            (CmpOp::Ge, "cmp.ge"),
        ];
        for (op, name) in ops {
            for &x in &grid_i32() {
                for &y in &grid_i32() {
                    let want = Some(I1(apply_ord(op, x, y)));
                    let got = f(op, I32(x), I32(y));
                    if !same(got, want) {
                        return Err(disagree(name, I32(x), I32(y), got, want));
                    }
                }
            }
            for &x in &grid_i64() {
                for &y in &grid_i64() {
                    let want = Some(I1(apply_ord(op, x, y)));
                    let got = f(op, I64(x), I64(y));
                    if !same(got, want) {
                        return Err(disagree(name, I64(x), I64(y), got, want));
                    }
                }
            }
            for &x in &grid_f64() {
                for &y in &grid_f64() {
                    // IEEE comparison, including the inf cases. No NaN
                    // inputs exist, so no unordered-comparison surprises.
                    let want = Some(I1(match op {
                        CmpOp::Eq => x == y,
                        CmpOp::Ne => x != y,
                        CmpOp::Lt => x < y,
                        CmpOp::Le => x <= y,
                        CmpOp::Gt => x > y,
                        CmpOp::Ge => x >= y,
                    }));
                    let got = f(op, F64(x), F64(y));
                    if !same(got, want) {
                        return Err(disagree(name, F64(x), F64(y), got, want));
                    }
                }
            }
            // i1 supports equality only; ordering booleans is refused.
            for &x in &grid_i1() {
                for &y in &grid_i1() {
                    let want = match op {
                        CmpOp::Eq => Some(I1(x == y)),
                        CmpOp::Ne => Some(I1(x != y)),
                        _ => None,
                    };
                    let got = f(op, I1(x), I1(y));
                    if !same(got, want) {
                        return Err(disagree(name, I1(x), I1(y), got, want));
                    }
                }
            }
            for (a, b) in [
                (I32(1), I64(1)),
                (I64(1), I32(1)),
                (I32(1), F64(1.0)),
                (F64(1.0), I32(1)),
                (I1(true), I32(1)),
                (I32(1), I1(true)),
                (I64(1), F64(1.0)),
            ] {
                let got = f(op, a, b);
                if !same(got, None) {
                    return Err(disagree(name, a, b, got, None));
                }
            }
        }
        Ok(())
    }

    fn apply_ord<T: PartialOrd + PartialEq>(op: CmpOp, x: T, y: T) -> bool {
        match op {
            CmpOp::Eq => x == y,
            CmpOp::Ne => x != y,
            CmpOp::Lt => x < y,
            CmpOp::Le => x <= y,
            CmpOp::Gt => x > y,
            CmpOp::Ge => x >= y,
        }
    }

    // ---- the seven saboteurs (mirrors docs/phase/NEXT-PHASE.md §2) ----

    pub fn sab_add_i32(op: ArithOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
        if let (ArithOp::Add, I32(x), I32(y)) = (op, a, b) {
            return x.checked_mul(y).and_then(|v| v.checked_add(1)).map(I32);
        }
        eval_arith(op, a, b)
    }
    pub fn sab_add_i64(op: ArithOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
        if let (ArithOp::Add, I64(x), I64(y)) = (op, a, b) {
            return x.checked_mul(y).and_then(|v| v.checked_add(1)).map(I64);
        }
        eval_arith(op, a, b)
    }
    pub fn sab_sub_swapped(op: ArithOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
        if let (ArithOp::Sub, I32(x), I32(y)) = (op, a, b) {
            return y.checked_sub(x).map(I32);
        }
        eval_arith(op, a, b)
    }
    pub fn sab_mul_is_add(op: ArithOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
        if let (ArithOp::Mul, I32(x), I32(y)) = (op, a, b) {
            return x.checked_add(y).map(I32);
        }
        eval_arith(op, a, b)
    }
    pub fn sab_div_is_mul(op: ArithOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
        if let (ArithOp::Div, I32(x), I32(y)) = (op, a, b) {
            return x.checked_mul(y).map(I32);
        }
        eval_arith(op, a, b)
    }
    /// f64 saboteur: keeps a NaN result instead of refusing it. Only the
    /// f64 leg can catch this one.
    pub fn sab_f64_keeps_nan(op: ArithOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
        if let (ArithOp::Add, F64(x), F64(y)) = (op, a, b) {
            return Some(F64(x + y));
        }
        eval_arith(op, a, b)
    }
    pub fn sab_lt_is_le(op: CmpOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
        if let (CmpOp::Lt, I32(x), I32(y)) = (op, a, b) {
            return Some(I1(x <= y));
        }
        eval_cmp(op, a, b)
    }
    pub fn sab_ge_is_gt(op: CmpOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
        if let (CmpOp::Ge, I64(x), I64(y)) = (op, a, b) {
            return Some(I1(x > y));
        }
        eval_cmp(op, a, b)
    }
    /// i1 saboteur: invents an ordering for booleans the table refuses.
    pub fn sab_i1_invents_ordering(op: CmpOp, a: ConstVal, b: ConstVal) -> Option<ConstVal> {
        if let (CmpOp::Lt, I1(x), I1(y)) = (op, a, b) {
            return Some(I1(!x & y));
        }
        eval_cmp(op, a, b)
    }
}

#[cfg(test)]
mod oracle_tests {
    use super::oracle::*;

    #[test]
    fn eval_arith_matches_rust_checked_arithmetic() {
        audit_arith(super::eval_arith).expect("fold table disagrees with Rust");
    }

    #[test]
    fn eval_cmp_matches_rust_comparison() {
        audit_cmp(super::eval_cmp).expect("cmp table disagrees with Rust");
    }

    /// Assert every saboteur is rejected, AND rejected for the right
    /// reason — the reported disagreement must name the operation that
    /// was corrupted. An oracle that errors on the wrong arm would pass
    /// a bare `is_err()` check while being broken.
    fn expect_caught<F: Copy>(
        cases: &[(&str, F, &str)],
        audit: impl Fn(F) -> Result<(), String>,
    ) {
        let mut bad = vec![];
        for &(name, f, want_op) in cases {
            match audit(f) {
                Ok(()) => bad.push(format!("{name}: ESCAPED — oracle is blind to it")),
                Err(msg) if !msg.contains(want_op) => {
                    bad.push(format!("{name}: caught, but on the wrong arm: {msg}"))
                }
                Err(_) => {}
            }
        }
        assert!(bad.is_empty(), "sabotage battery: {bad:#?}");
    }

    /// D1 red/green: exactly the seven fold-table corruptions measured
    /// in docs/phase/NEXT-PHASE.md §2, five of which survived the whole
    /// 121-test suite (the 10,000-fabric corpus included) before this
    /// oracle existed. Without this fixture the oracle could be vacuous
    /// — a check that passes everything tests nothing.
    #[test]
    fn the_seven_documented_sabotages_are_caught() {
        expect_caught(
            &[
                ("i32 add -> x*y+1", sab_add_i32 as ArithFn, "arith.add"),
                ("i64 add -> x*y+1", sab_add_i64 as ArithFn, "arith.add"),
                ("i32 sub -> y-x (swapped)", sab_sub_swapped as ArithFn, "arith.sub"),
                ("i32 mul -> x+y", sab_mul_is_add as ArithFn, "arith.mul"),
                ("i32 div -> x*y", sab_div_is_mul as ArithFn, "arith.div"),
            ],
            audit_arith,
        );
        expect_caught(
            &[
                ("cmp i32 Lt -> <=", sab_lt_is_le as CmpFn, "cmp.lt"),
                ("cmp i64 Ge -> >", sab_ge_is_gt as CmpFn, "cmp.ge"),
            ],
            audit_cmp,
        );
    }

    /// The f64 and i1 legs are new in R1 (the grid widening). They must
    /// earn their place: each catches a corruption the integer legs
    /// structurally cannot see.
    #[test]
    fn the_widened_f64_and_i1_legs_catch_their_own_sabotages() {
        expect_caught(
            &[("f64 add keeps NaN", sab_f64_keeps_nan as ArithFn, "arith.add")],
            audit_arith,
        );
        expect_caught(
            &[("cmp i1 invents ordering", sab_i1_invents_ordering as CmpFn, "cmp.lt")],
            audit_cmp,
        );
    }
}

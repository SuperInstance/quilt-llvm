// R1 lane 3 (tier S2) — the property-oracle JUDGE FIXTURE.
//
// This is the ~55-line oracle from NEXT-PHASE §2, in test-fixture form.
// R1 lane 1 lands the in-tree version; this copy exists ONLY so the
// sabotage battery can measure the "suite + property oracle" column
// without waiting for lane 1 to merge. It is appended to
// src/passes/constfold.rs (as a #[cfg(test)] module) by
// scripts/sem_mutants_driver.py during battery runs and restored after.
//
// Basis: eval_arith / eval_cmp are compared against Rust's own checked
// arithmetic and comparisons over a 15x15 operand grid of boundary
// values. Rust's primitive ops are the ground truth — a corrupted fold
// table cannot agree with them everywhere on the grid.

#[cfg(test)]
mod sem_oracle_fixture {
    use super::*;
    use crate::cell::{ArithOp, CmpOp};
    use crate::ty::ConstVal::*;
    use ArithOp::*;

    fn i32_grid() -> Vec<i32> {
        vec![
            0, 1, -1, 2, -2, 3, -3, 7, -7, i32::MAX, i32::MIN, i32::MAX - 1,
            i32::MIN + 1, i32::MAX / 2, i32::MIN / 2 + 1,
        ]
    }
    fn i64_grid() -> Vec<i64> {
        vec![
            0, 1, -1, 2, -2, 3, -3, 7, -7, i64::MAX, i64::MIN, i64::MAX - 1,
            i64::MIN + 1, i64::MAX / 2, i64::MIN / 2 + 1,
        ]
    }
    fn f64_grid() -> Vec<f64> {
        vec![
            0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 2.5, -2.5, 3.0, f64::INFINITY,
            f64::NEG_INFINITY, 1e308, -1e308, 1e-300, 5.0,
        ]
    }

    fn gt_arith_i32(op: ArithOp, x: i32, y: i32) -> Option<ConstVal> {
        match op {
            Add => x.checked_add(y).map(I32),
            Sub => x.checked_sub(y).map(I32),
            Mul => x.checked_mul(y).map(I32),
            Div => x.checked_div(y).map(I32),
        }
    }
    fn gt_arith_i64(op: ArithOp, x: i64, y: i64) -> Option<ConstVal> {
        match op {
            Add => x.checked_add(y).map(I64),
            Sub => x.checked_sub(y).map(I64),
            Mul => x.checked_mul(y).map(I64),
            Div => x.checked_div(y).map(I64),
        }
    }
    fn gt_arith_f64(op: ArithOp, x: f64, y: f64) -> Option<ConstVal> {
        // spec: fold only skipped when the result would be NaN
        let r = match op {
            Add => x + y,
            Sub => x - y,
            Mul => x * y,
            Div => x / y,
        };
        if r.is_nan() { None } else { Some(F64(r)) }
    }

    #[test]
    fn oracle_arith_i32_matches_rust_checked() {
        for op in [Add, Sub, Mul, Div] {
            for &x in &i32_grid() {
                for &y in &i32_grid() {
                    let got = eval_arith(op, I32(x), I32(y));
                    let want = gt_arith_i32(op, x, y);
                    assert_eq!(got, want, "arith {:?} i32 ({}, {})", op, x, y);
                }
            }
        }
    }

    #[test]
    fn oracle_arith_i64_matches_rust_checked() {
        for op in [Add, Sub, Mul, Div] {
            for &x in &i64_grid() {
                for &y in &i64_grid() {
                    let got = eval_arith(op, I64(x), I64(y));
                    let want = gt_arith_i64(op, x, y);
                    assert_eq!(got, want, "arith {:?} i64 ({}, {})", op, x, y);
                }
            }
        }
    }

    #[test]
    fn oracle_arith_f64_matches_rust_spec() {
        for op in [Add, Sub, Mul, Div] {
            for &x in &f64_grid() {
                for &y in &f64_grid() {
                    let got = eval_arith(op, F64(x), F64(y));
                    let want = gt_arith_f64(op, x, y);
                    assert_eq!(got, want, "arith {:?} f64 ({:?}, {:?})", op, x, y);
                }
            }
        }
    }

    fn gt_cmp_i32(op: CmpOp, x: i32, y: i32) -> ConstVal {
        let r = match op {
            CmpOp::Eq => x == y,
            CmpOp::Ne => x != y,
            CmpOp::Lt => x < y,
            CmpOp::Le => x <= y,
            CmpOp::Gt => x > y,
            CmpOp::Ge => x >= y,
        };
        I1(r)
    }

    #[test]
    fn oracle_cmp_i32_matches_rust() {
        for op in [CmpOp::Eq, CmpOp::Ne, CmpOp::Lt, CmpOp::Le, CmpOp::Gt, CmpOp::Ge] {
            for &x in &i32_grid() {
                for &y in &i32_grid() {
                    assert_eq!(
                        eval_cmp(op, I32(x), I32(y)),
                        Some(gt_cmp_i32(op, x, y)),
                        "cmp {:?} i32 ({}, {})",
                        op,
                        x,
                        y
                    );
                }
            }
        }
    }

    #[test]
    fn oracle_cmp_i64_matches_rust() {
        for op in [CmpOp::Eq, CmpOp::Ne, CmpOp::Lt, CmpOp::Le, CmpOp::Gt, CmpOp::Ge] {
            for &x in &i64_grid() {
                for &y in &i64_grid() {
                    let r = match op {
                        CmpOp::Eq => x == y,
                        CmpOp::Ne => x != y,
                        CmpOp::Lt => x < y,
                        CmpOp::Le => x <= y,
                        CmpOp::Gt => x > y,
                        CmpOp::Ge => x >= y,
                    };
                    assert_eq!(
                        eval_cmp(op, I64(x), I64(y)),
                        Some(I1(r)),
                        "cmp {:?} i64 ({}, {})",
                        op,
                        x,
                        y
                    );
                }
            }
        }
    }

    #[test]
    fn oracle_cmp_f64_matches_rust() {
        for op in [CmpOp::Eq, CmpOp::Ne, CmpOp::Lt, CmpOp::Le, CmpOp::Gt, CmpOp::Ge] {
            for &x in &f64_grid() {
                for &y in &f64_grid() {
                    let r = match op {
                        CmpOp::Eq => x == y,
                        CmpOp::Ne => x != y,
                        CmpOp::Lt => x < y,
                        CmpOp::Le => x <= y,
                        CmpOp::Gt => x > y,
                        CmpOp::Ge => x >= y,
                    };
                    assert_eq!(
                        eval_cmp(op, F64(x), F64(y)),
                        Some(I1(r)),
                        "cmp {:?} f64 ({:?}, {:?})",
                        op,
                        x,
                        y
                    );
                }
            }
        }
    }

    #[test]
    fn oracle_cmp_i1_eq_ne_defined() {
        // spec: i1 folds Eq/Ne only (no ordering on booleans in v0)
        for x in [true, false] {
            for y in [true, false] {
                assert_eq!(eval_cmp(CmpOp::Eq, I1(x), I1(y)), Some(I1(x == y)));
                assert_eq!(eval_cmp(CmpOp::Ne, I1(x), I1(y)), Some(I1(x != y)));
            }
        }
    }
}

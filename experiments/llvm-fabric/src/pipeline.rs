//! The pass pipeline used by the experiments: constfold -> dce ->
//! constfold -> dce, with history appended per pass. Returns every
//! actual intermediate fabric alongside the history that should
//! reproduce them (replay experiment compares the two).

use crate::diff::History;
use crate::fabric::Fabric;
use std::collections::BTreeMap;

pub const PIPELINE: &[&str] = &["constfold", "dce", "constfold", "dce"];

/// v1 pipeline: the third pass (inline) slots between dce and the second
/// fold/dce pair, per the scout build order (largest diffs ship last).
pub const PIPELINE_V1: &[&str] = &["constfold", "dce", "inline", "constfold", "dce"];

/// Run the pipeline. Returns (final, history, stages) where stages[0] is
/// the input and stages[i] is the fabric after the i-th pass.
pub fn run(f: &Fabric) -> Result<(Fabric, History, Vec<Fabric>), String> {
    run_named(f, PIPELINE, &BTreeMap::new())
}

/// Run the v1 pipeline over a program's main fabric with its callees.
pub fn run_v1(
    f: &Fabric,
    funcs: &BTreeMap<String, Fabric>,
) -> Result<(Fabric, History, Vec<Fabric>), String> {
    run_named(f, PIPELINE_V1, funcs)
}

fn run_named(
    f: &Fabric,
    pipeline: &[&str],
    funcs: &BTreeMap<String, Fabric>,
) -> Result<(Fabric, History, Vec<Fabric>), String> {
    let mut h = History::new();
    let mut stages = vec![f.clone()];
    let mut cur = f.clone();
    let mut prev = f.clone();
    for name in pipeline {
        let rec = match *name {
            "constfold" => {
                let (next, rec) = crate::passes::constfold::const_fold(&cur)?;
                cur = next;
                rec
            }
            "dce" => {
                let (next, rec) = crate::passes::dce::dce(&cur)?;
                cur = next;
                rec
            }
            "inline" => {
                let (next, rec) = crate::passes::inline::inline_calls(&cur, funcs)?;
                cur = next;
                rec
            }
            other => return Err(format!("unknown pass {}", other)),
        };
        crate::conserve::population_audit(&prev, &cur, &rec)
            .map_err(|e| format!("pipeline pass {}: population audit: {}", name, e))?;
        h.push_tick(rec, &cur);
        stages.push(cur.clone());
        prev = cur.clone();
    }
    Ok((cur, h, stages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, CellKind};
    use crate::conserve;
    use crate::replay;
    use crate::ty::{ConstVal, Type};
    use crate::verify::verify;

    fn mix() -> Fabric {
        // params keep some cells live; consts + dead code keep passes busy
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let c1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(20) }));
        let c2 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(22) }));
        let mut a = Cell::new(e, CellKind::Arith { op: crate::cell::ArithOp::Add, ty: Type::I32 });
        a.operands = vec![c1, c2];
        let a = f.add_cell(e, a);
        let dead = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I64, val: ConstVal::I64(7) }));
        let mut a2 = Cell::new(e, CellKind::Arith { op: crate::cell::ArithOp::Add, ty: Type::I32 });
        a2.operands = vec![p, a];
        let a2 = f.add_cell(e, a2);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![a2];
        f.add_cell(e, r);
        let _ = dead;
        f
    }

    #[test]
    fn pipeline_conserves_and_verifies() {
        let f = mix();
        let (final_f, history, stages) = run(&f).unwrap();
        assert_eq!(stages.len(), 5, "4 passes = 5 stages");
        assert!(verify(&final_f).is_ok());
        assert!(conserve::check_pipeline(&f, &final_f, &history).is_ok());
        // fold happened (20+22 -> 42)
        let ret_id = final_f
            .cells()
            .find(|&id| matches!(final_f.cell(id).map(|c| &c.kind), Some(CellKind::Ret)))
            .unwrap();
        let fed = final_f.cell(ret_id).unwrap().operands[0];
        let fed2 = final_f.cell(fed).unwrap().operands[1];
        assert_eq!(
            final_f.cell(fed2).unwrap().kind,
            CellKind::Const { ty: Type::I32, val: ConstVal::I32(42) }
        );
    }

    #[test]
    fn replay_reproduces_every_stage_bit_identically() {
        let f = mix();
        let (final_f, history, stages) = run(&f).unwrap();
        let (replayed, final_r) = replay::replay(&f, &history).unwrap();
        assert_eq!(replayed.len(), stages.len());
        for (i, (actual, replayed_stage)) in stages.iter().zip(replayed.iter()).enumerate() {
            assert_eq!(actual, replayed_stage, "stage {} must match structurally", i);
            assert_eq!(
                crate::text::print(actual),
                crate::text::print(replayed_stage),
                "stage {} must match as canonical text",
                i
            );
        }
        assert_eq!(final_f, final_r);
        assert_eq!(crate::text::print(&final_f), crate::text::print(&final_r));
    }
}

#[cfg(test)]
mod v1_tests {
    use super::*;
    use crate::cell::CellKind;
    use crate::conserve;
    use crate::id::CellId;
    use crate::replay;
    use crate::verify::verify;
    use std::collections::BTreeMap;

    fn prog() -> (Fabric, BTreeMap<String, Fabric>) {
        let main_text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = const i32 20\n\
  %2 = const i32 22\n\
  %3 = const i64 9i64\n\
  %4 = call i32 add2 %1, %2\n\
  %5 = ret %4\n";
        let callee_text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = param i32\n\
  %2 = arith.add i32 %0, %1\n\
  %3 = ret %2\n";
        let mut funcs = BTreeMap::new();
        funcs.insert("add2".to_string(), crate::text::parse(callee_text).unwrap());
        (crate::text::parse(main_text).unwrap(), funcs)
    }

    #[test]
    fn v1_pipeline_folds_through_the_inline_boundary() {
        let (f, funcs) = prog();
        let (final_f, history, stages) = run_v1(&f, &funcs).unwrap();
        assert_eq!(stages.len(), 6, "5 passes = 6 stages");
        assert!(verify(&final_f).is_ok());
        assert!(conserve::check_pipeline(&f, &final_f, &history).is_ok());
        // the dead i64 const is DCE'd; the call is inlined; 20+22 folds
        // THROUGH the graft: ret is fed by const 42
        assert!(final_f.cell(CellId(4)).is_none(), "call gone");
        assert!(final_f.cell(CellId(3)).is_none(), "dead const gone");
        let fed = final_f.cell(CellId(5)).unwrap().operands[0];
        assert_eq!(
            final_f.cell(fed).unwrap().kind,
            CellKind::Const { ty: crate::ty::Type::I32, val: crate::ty::ConstVal::I32(42) },
            "fold crossed the inline boundary"
        );
        // replay still reproduces every stage bit-identically
        let (replayed, final_r) = replay::replay(&f, &history).unwrap();
        assert_eq!(replayed.len(), stages.len());
        for (i, (a, b)) in stages.iter().zip(replayed.iter()).enumerate() {
            assert_eq!(a, b, "stage {}", i);
            assert_eq!(crate::text::print(a), crate::text::print(b), "stage {} text", i);
        }
        assert_eq!(final_f, final_r);
        // the inline epoch is in the history with its ledger entry
        let inline_rec = history
            .records
            .iter()
            .find(|r| r.pass == "inline")
            .expect("inline epoch recorded");
        assert!(inline_rec.edits.iter().any(|e| matches!(e,
            crate::diff::Edit::RemoveCell { ledger, .. } if ledger.contains("inlined 'add2'"))));
    }
}

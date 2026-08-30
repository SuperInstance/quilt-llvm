//! The pass pipeline used by the experiments: constfold -> dce ->
//! constfold -> dce, with history appended per pass. Returns every
//! actual intermediate fabric alongside the history that should
//! reproduce them (replay experiment compares the two).

use crate::diff::History;
use crate::fabric::Fabric;

pub const PIPELINE: &[&str] = &["constfold", "dce", "constfold", "dce"];

/// Run the pipeline. Returns (final, history, stages) where stages[0] is
/// the input and stages[i] is the fabric after the i-th pass.
pub fn run(f: &Fabric) -> Result<(Fabric, History, Vec<Fabric>), String> {
    let mut h = History::new();
    let mut stages = vec![f.clone()];
    let mut cur = f.clone();
    for name in PIPELINE {
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
            other => return Err(format!("unknown pass {}", other)),
        };
        h.push(rec);
        stages.push(cur.clone());
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

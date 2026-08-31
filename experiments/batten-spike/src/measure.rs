//! Pipeline candidate set + cost/benefit measurement.
//!
//! TOY, LABELED: "cost" is cells processed, approximated without
//! instrumenting the passes: the cell count of each pass's input fabric,
//! summed over the pipeline. "Benefit" is relative size reduction,
//! zeroed if the output fails the verifier. This is a proxy for compile
//! time / code-quality, not a measurement of either.

use llvm_fabric::fabric::Fabric;
use llvm_fabric::passes::{constfold, dce};
use llvm_fabric::verify::verify;

pub const PIPELINES: &[&str] = &[
    "none",          // no passes (baseline)
    "fold",          // const-fold only
    "fold>dce",      // fold then DCE
    "dce>fold",      // DCE then fold
    "full",          // fold>dce>fold>dce (the llvm-fabric default pipeline)
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outcome {
    pub cells_in: usize,
    pub cells_out: usize,
    pub cost: usize, // sum of input cell counts per pass run
    pub verify_ok: bool,
}

impl Outcome {
    /// Relative size reduction in [0,1]; 0 if not verify-clean.
    pub fn utility(&self) -> f64 {
        if !self.verify_ok || self.cells_in == 0 {
            return 0.0;
        }
        (self.cells_in - self.cells_out) as f64 / self.cells_in as f64
    }

    /// Cost normalized by input size (scale-free "how much work per cell").
    pub fn rel_cost(&self) -> f64 {
        self.cost as f64 / self.cells_in.max(1) as f64
    }
}

/// LAMBDA: tradeoff between utility and relative cost in the routing
/// score  score = utility - LAMBDA * rel_cost.  TOY choice: each unit of
/// per-cell work costs 5% of a utility point.
pub const LAMBDA: f64 = 0.05;

impl Outcome {
    pub fn score(&self) -> f64 {
        self.utility() - LAMBDA * self.rel_cost()
    }
}

pub fn run_pipeline(f: &Fabric, name: &str) -> Result<(Fabric, Outcome), String> {
    let cells_in = f.cells().count();
    let mut cur = f.clone();
    let mut cost = 0usize;
    let steps: &[&str] = match name {
        "none" => &[],
        "fold" => &["fold"],
        "fold>dce" => &["fold", "dce"],
        "dce>fold" => &["dce", "fold"],
        "full" => &["fold", "dce", "fold", "dce"],
        other => return Err(format!("unknown pipeline {}", other)),
    };
    for s in steps {
        cost += cur.cells().count();
        let (next, _rec) = match *s {
            "fold" => constfold::const_fold(&cur)?,
            "dce" => dce::dce(&cur)?,
            other => return Err(format!("unknown pass {}", other)),
        };
        cur = next;
    }
    let verify_ok = verify(&cur).is_ok();
    let cells_out = cur.cells().count();
    Ok((
        cur,
        Outcome { cells_in, cells_out, cost, verify_ok },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use llvm_fabric::cell::{Cell, CellKind};
    use llvm_fabric::fuzz::Rng;
    use llvm_fabric::ty::{ConstVal, Type};

    fn foldable() -> Fabric {
        // (const 20) + (const 22), returned: folds to const 42
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let c1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(20) }));
        let c2 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(22) }));
        let mut a = Cell::new(e, CellKind::Arith { op: llvm_fabric::cell::ArithOp::Add, ty: Type::I32 });
        a.operands = vec![c1, c2];
        let a = f.add_cell(e, a);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![a];
        f.add_cell(e, r);
        f
    }

    #[test]
    fn none_pipeline_costs_nothing_and_changes_nothing() {
        let f = foldable();
        let (out, o) = run_pipeline(&f, "none").unwrap();
        assert_eq!(o.cost, 0);
        assert_eq!(o.cells_out, o.cells_in);
        assert_eq!(out.cells().count(), f.cells().count());
        assert_eq!(o.utility(), 0.0);
    }

    #[test]
    fn fold_dce_shrinks_foldable_fabric() {
        let f = foldable();
        let (_, o) = run_pipeline(&f, "fold>dce").unwrap();
        assert!(o.verify_ok);
        assert!(o.utility() > 0.0, "expected reduction, got {:?}", o);
    }

    #[test]
    fn fold_alone_does_not_change_cell_count() {
        // v0 fold replaces cells in place; only DCE removes them.
        let f = foldable();
        let (_, o) = run_pipeline(&f, "fold").unwrap();
        assert_eq!(o.cells_out, o.cells_in);
        assert_eq!(o.utility(), 0.0);
    }

    #[test]
    fn full_is_most_expensive_on_foldable() {
        let f = foldable();
        let costs: Vec<usize> = PIPELINES
            .iter()
            .map(|p| run_pipeline(&f, p).unwrap().1.cost)
            .collect();
        assert!(costs[4] >= *costs.iter().max().unwrap());
    }

    #[test]
    fn every_pipeline_is_verify_clean_on_fuzz_corpus() {
        // conservation/verify sanity over a small corpus slice
        let mut rng = Rng::new(42);
        for _ in 0..50 {
            let f = llvm_fabric::fuzz::gen_fabric(&mut rng);
            for p in PIPELINES {
                let (_, o) = run_pipeline(&f, p).unwrap();
                assert!(o.verify_ok, "pipeline {} broke the verifier", p);
            }
        }
    }
}

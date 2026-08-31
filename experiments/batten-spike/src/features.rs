//! Cheap fabric features that key the battens.
//!
//! Deliberately cheap (one walk, no pass runs): cell count, op mix
//! (arith fraction, const fraction), and dependency depth. These are the
//! "fabric signature" stand-ins REVERSE-ACTUALIZATION.md §3(a) asks the
//! experiments lane to mint and measure.

use llvm_fabric::cell::CellKind;
use llvm_fabric::fabric::Fabric;
use llvm_fabric::id::CellId;
use std::collections::BTreeMap;

/// Raw feature vector (before standardization):
///   [ ln(live cells), arith_frac, const_frac, depth / cells ]
pub fn raw_features(f: &Fabric) -> Vec<f64> {
    let ids: Vec<CellId> = f.cells().collect();
    let n = ids.len().max(1) as f64;
    let (mut arith, mut consts) = (0usize, 0usize);
    for &id in &ids {
        if let Some(c) = f.cell(id) {
            match c.kind {
                CellKind::Arith { .. } | CellKind::Cmp { .. } => arith += 1,
                CellKind::Const { .. } => consts += 1,
                _ => {}
            }
        }
    }
    let depth = max_depth(f);
    vec![
        (ids.len() as f64 + 1.0).ln(),
        arith as f64 / n,
        consts as f64 / n,
        depth as f64 / n,
    ]
}

/// Longest operand chain (params/consts = 0, users = 1 + max of operands).
fn max_depth(f: &Fabric) -> usize {
    let mut memo: BTreeMap<CellId, usize> = BTreeMap::new();
    let ids: Vec<CellId> = f.cells().collect();
    let mut best = 0;
    for id in ids {
        best = best.max(depth_of(f, id, &mut memo));
    }
    best
}

fn depth_of(f: &Fabric, id: CellId, memo: &mut BTreeMap<CellId, usize>) -> usize {
    if let Some(&d) = memo.get(&id) {
        return d;
    }
    memo.insert(id, 0); // cycle guard (shouldn't occur in valid SSA fabrics)
    let d = match f.cell(id) {
        None => 0,
        Some(c) => match c.operands.iter().try_fold(0usize, |acc, &op| {
            let od = depth_of(f, op, memo);
            Ok::<usize, ()>(acc.max(od + 1))
        }) {
            Ok(d) => d,
            Err(()) => 0,
        }
    };
    memo.insert(id, d);
    d
}

/// Standardize features by training-set mean/std so kernel distances treat
/// each dimension comparably. Returns (mean, std) plus the transform.
pub struct Standardizer {
    mean: Vec<f64>,
    std: Vec<f64>,
}

impl Standardizer {
    pub fn fit(rows: &[Vec<f64>]) -> Standardizer {
        let d = rows[0].len();
        let n = rows.len() as f64;
        let mut mean = vec![0.0; d];
        for r in rows {
            for (i, v) in r.iter().enumerate() {
                mean[i] += v;
            }
        }
        for m in &mut mean {
            *m /= n;
        }
        let mut std = vec![0.0; d];
        for r in rows {
            for (i, v) in r.iter().enumerate() {
                std[i] += (v - mean[i]) * (v - mean[i]);
            }
        }
        for s in &mut std {
            *s = (*s / n).sqrt().max(1e-9);
        }
        Standardizer { mean, std }
    }

    pub fn transform(&self, x: &[f64]) -> Vec<f64> {
        x.iter()
            .zip(&self.mean)
            .zip(&self.std)
            .map(|((v, m), s)| (v - m) / s)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llvm_fabric::cell::{ArithOp, Cell, CellKind};
    use llvm_fabric::fuzz::Rng;
    use llvm_fabric::ty::{ConstVal, Type};

    fn chain() -> Fabric {
        // param -> +c1 -> +c2 : depth 2, one arith? no: two ariths, two consts
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let c1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let c2 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(2) }));
        let mut a1 = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a1.operands = vec![p, c1];
        let a1 = f.add_cell(e, a1);
        let mut a2 = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a2.operands = vec![a1, c2];
        let _a2 = f.add_cell(e, a2);
        f
    }

    #[test]
    fn depth_counts_operand_chain() {
        assert_eq!(max_depth(&chain()), 2);
    }

    #[test]
    fn features_shape_and_ranges() {
        let f = chain();
        let x = raw_features(&f);
        assert_eq!(x.len(), 4);
        assert!(x[0] > 0.0);
        assert!(x[1] > 0.0 && x[1] <= 1.0); // 2 ariths of 5 cells
        assert!(x[2] > 0.0 && x[2] <= 1.0);
        assert!(x[3] > 0.0 && x[3] <= 1.0);
    }

    #[test]
    fn fuzz_fabrics_have_sane_features() {
        let mut rng = Rng::new(7);
        for _ in 0..20 {
            let f = llvm_fabric::fuzz::gen_fabric(&mut rng);
            let x = raw_features(&f);
            assert!(x.iter().all(|v| v.is_finite()), "non-finite feature {:?}", x);
            assert!(x[1] >= 0.0 && x[1] <= 1.0);
            assert!(x[2] >= 0.0 && x[2] <= 1.0);
        }
    }

    #[test]
    fn standardizer_zeroes_mean() {
        let rows: Vec<Vec<f64>> = (0..50)
            .map(|s| {
                let f = llvm_fabric::fuzz::gen_fabric(&mut Rng::new(s));
                raw_features(&f)
            })
            .collect();
        let st = Standardizer::fit(&rows);
        let mut mean = vec![0.0; 4];
        for r in &rows {
            let t = st.transform(r);
            for i in 0..4 {
                mean[i] += t[i];
            }
        }
        for m in &mean {
            assert!((m / 50.0).abs() < 1e-9);
        }
    }
}

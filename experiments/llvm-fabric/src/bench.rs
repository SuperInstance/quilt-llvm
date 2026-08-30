//! Experiment (c): IR size and serialize/parse time, fabric text vs an
//! equivalent plain SSA text baseline, on toy programs of growing size.
//!
//! Honesty notes (repeated in EXPERIMENTS.md):
//! - the baseline is OUR minimal LLVM-.ll-like printer, not LLVM's own
//!   writer; we compare FORMAT overhead, not implementations;
//! - timings are wall-clock medians on one machine (WSL2), unoptimized
//!   debug build unless stated — order-of-magnitude evidence, not a
//!   performance claim.

use crate::cell::{ArithOp, Cell, CellKind, CmpOp};
use crate::fabric::Fabric;
use crate::id::{CellId, RegionId};
use crate::ty::{ConstVal, Type};
use std::time::Instant;

// ---------- toy program shapes ----------

/// Straight-line add chain fed by a param (not foldable — size-honest).
pub fn chain(n: u64) -> Fabric {
    let mut f = Fabric::empty();
    let e = f.add_region("entry");
    let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
    let one = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
    let mut prev = p;
    for _ in 0..n {
        let mut a = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a.operands = vec![prev, one];
        prev = f.add_cell(e, a);
    }
    let mut r = Cell::new(e, CellKind::Ret);
    r.operands = vec![prev];
    f.add_cell(e, r);
    f
}

/// n diamonds chained structurally: entry branches into diamond 0; each
/// join branches into the next diamond; the last join returns its phi.
/// Arms always compute from the ENTRY param (v0 scope rules forbid using
/// the previous join's phi in arms without a phi — booked as a v0 shape
/// limitation, not hidden).
pub fn diamonds(n: u64) -> Fabric {
    let mut f = Fabric::empty();
    let e = f.add_region("entry");
    let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
    let zero = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(0) }));
    let mut prev_join: Option<RegionId> = None;
    for i in 0..n {
        let t = f.add_region(format!("t{}", i));
        let el = f.add_region(format!("e{}", i));
        let j = f.add_region(format!("j{}", i));
        let split = prev_join.unwrap_or(e);
        let cmp = f.add_cell(split, Cell::new(split, CellKind::Cmp { op: CmpOp::Lt }));
        {
            let c = f.cell_mut(cmp).unwrap();
            c.operands = vec![p, zero];
        }
        let mut br = Cell::new(split, CellKind::Branch { then_r: t, else_r: el });
        br.operands = vec![cmp];
        f.add_cell(split, br);
        let one = f.add_cell(t, Cell::new(t, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let mut a1 = Cell::new(t, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a1.operands = vec![p, one];
        let a1 = f.add_cell(t, a1);
        f.add_cell(t, Cell::new(t, CellKind::Jump { target: j }));
        let two = f.add_cell(el, Cell::new(el, CellKind::Const { ty: Type::I32, val: ConstVal::I32(2) }));
        let mut a2 = Cell::new(el, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a2.operands = vec![p, two];
        let a2 = f.add_cell(el, a2);
        f.add_cell(el, Cell::new(el, CellKind::Jump { target: j }));
        let mut phi = Cell::new(j, CellKind::Phi { joins: vec![t, el] });
        phi.operands = vec![a1, a2];
        let phi_id = f.add_cell(j, phi);
        if i + 1 == n {
            let mut r = Cell::new(j, CellKind::Ret);
            r.operands = vec![phi_id];
            f.add_cell(j, r);
        }
        prev_join = Some(j);
    }
    f
}

/// Layered dense DAG of adds over two params (wide provenance, no folding).
pub fn dense_dag(n: u64, seed: u64) -> Fabric {
    let mut rng = crate::fuzz::Rng::new(seed);
    let mut f = Fabric::empty();
    let e = f.add_region("entry");
    let p1 = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
    let p2 = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
    let mut layer: Vec<CellId> = vec![p1, p2];
    let per_layer = (n / 8).max(2) as u64;
    let layers = (n / per_layer.max(1)).max(1);
    for _ in 0..layers {
        let mut next = vec![];
        for _ in 0..per_layer {
            let a = layer[rng.below(layer.len() as u64) as usize];
            let b = layer[rng.below(layer.len() as u64) as usize];
            let mut c = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
            c.operands = vec![a, b];
            next.push(f.add_cell(e, c));
        }
        layer = next;
    }
    let mut r = Cell::new(e, CellKind::Ret);
    r.operands = vec![*layer.last().expect("nonempty")];
    f.add_cell(e, r);
    f
}

/// Fully-foldable chain: consts all the way (pipeline history stress).
pub fn foldchain(n: u64) -> Fabric {
    let mut f = Fabric::empty();
    let e = f.add_region("entry");
    let mut prev = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
    for _ in 0..n {
        let one = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let mut a = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a.operands = vec![prev, one];
        prev = f.add_cell(e, a);
    }
    let mut r = Cell::new(e, CellKind::Ret);
    r.operands = vec![prev];
    f.add_cell(e, r);
    f
}

// ---------- plain SSA text baseline ----------

/// Minimal LLVM-.ll-flavored text for the same fabric. Same information,
/// fewer reserved words: bare block labels, short opcodes, typed phi.
pub fn baseline_ssa(f: &Fabric) -> String {
    let mut out = String::new();
    for (i, region) in f.regions.iter().enumerate() {
        let _ = i;
        out.push_str(&format!("{}:\n", region.name));
        for &id in &region.cells {
            let c = match f.cell(id) {
                Some(c) => c,
                None => continue,
            };
            let o = |k: usize| -> String {
                c.operands.get(k).map(|x| format!("%{}", x.0)).unwrap_or_default()
            };
            let line = match &c.kind {
                CellKind::Param { ty } => format!("{} = param {}", id.0, ty.name()),
                CellKind::Const { ty, val } => format!("{} = const {} {}", id.0, ty.name(), val.render()),
                CellKind::Arith { op, ty } => {
                    let opc = match op {
                        ArithOp::Add => "add",
                        ArithOp::Sub => "sub",
                        ArithOp::Mul => "mul",
                        ArithOp::Div => "sdiv",
                    };
                    format!("{} = {} {} {}, {}", id.0, opc, ty.name(), o(0), o(1))
                }
                CellKind::Cmp { op } => {
                    let opc = match op {
                        CmpOp::Eq => "icmp eq",
                        CmpOp::Ne => "icmp ne",
                        CmpOp::Lt => "icmp slt",
                        CmpOp::Le => "icmp sle",
                        CmpOp::Gt => "icmp sgt",
                        CmpOp::Ge => "icmp sge",
                    };
                    format!("{} = {} {}, {}", id.0, opc, o(0), o(1))
                }
                CellKind::Branch { then_r, else_r } => {
                    format!("br {}, label %{}, label %{}", o(0), f.region_name(*then_r), f.region_name(*else_r))
                }
                CellKind::Jump { target } => format!("br label %{}", f.region_name(*target)),
                CellKind::Phi { joins } => {
                    let ty = f.ty_of(c.operands[0]).map(|t| t.name().to_string()).unwrap_or_else(|| "?".into());
                    let parts: Vec<String> = joins
                        .iter()
                        .zip(c.operands.iter())
                        .map(|(r, v)| format!("[ %{}, %{} ]", v.0, f.region_name(*r)))
                        .collect();
                    format!("{} = phi {} {}", id.0, ty, parts.join(", "))
                }
                CellKind::Ret => {
                    if c.operands.is_empty() {
                        "ret void".into()
                    } else {
                        format!("ret {}", o(0))
                    }
                }
            };
            out.push_str("  ");
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

// ---------- measurement ----------

pub fn median_ns(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    let mid = samples.len() / 2;
    samples[mid]
}

pub struct Row {
    pub shape: String,
    pub cells: usize,
    pub fabric_bytes: usize,
    pub baseline_bytes: usize,
    pub print_ns: u128,
    pub baseline_print_ns: u128,
    pub parse_ns: u128,
    pub verify_ns: u128,
}

pub fn measure(shape: &str, f: &Fabric, reps: usize) -> Row {
    let mut p = vec![];
    let mut bp = vec![];
    let mut pa = vec![];
    let mut ve = vec![];
    let mut text = String::new();
    for i in 0..reps {
        let t = Instant::now();
        text = crate::text::print(f);
        p.push(t.elapsed().as_nanos());
        let t = Instant::now();
        let _ = baseline_ssa(f);
        bp.push(t.elapsed().as_nanos());
        let t = Instant::now();
        let _ = crate::text::parse(&text);
        pa.push(t.elapsed().as_nanos());
        let t = Instant::now();
        let _ = crate::verify::verify(f);
        ve.push(t.elapsed().as_nanos());
        let _ = i;
    }
    Row {
        shape: shape.to_string(),
        cells: f.cells().count(),
        fabric_bytes: text.len(),
        baseline_bytes: baseline_ssa(f).len(),
        print_ns: median_ns(&mut p),
        baseline_print_ns: median_ns(&mut bp),
        parse_ns: median_ns(&mut pa),
        verify_ns: median_ns(&mut ve),
    }
}

/// History overhead after the 4-pass pipeline (bytes history vs final).
pub fn history_overhead(f: &Fabric) -> Result<(usize, usize, usize), String> {
    let (final_f, history, _) = crate::pipeline::run(f)?;
    let hb = history.bytes(&final_f);
    let fb = crate::text::print(&final_f).len();
    let ob = crate::text::print(f).len();
    Ok((ob, fb, hb))
}

pub fn bench() -> String {
    let mut out = String::new();
    out.push_str("shape                cells   fabric-B  base-B  ratio  print-us  baseprint-us  parse-us  verify-us\n");
    let mut shapes: Vec<(String, Fabric)> = vec![];
    for n in [50u64, 200, 800] {
        shapes.push((format!("chain-{}", n), chain(n)));
    }
    for n in [10u64, 40, 160] {
        let f = diamonds(n);
        shapes.push((format!("diamonds-{}", n), f));
    }
    for n in [50u64, 200, 800] {
        shapes.push((format!("dag-{}", n), dense_dag(n, 42)));
    }
    for (name, f) in &shapes {
        let r = measure(name, f, 21);
        out.push_str(&format!(
            "{:<20} {:>5} {:>9} {:>7} {:>6.2} {:>8.1} {:>12.1} {:>9.1} {:>9.1}\n",
            r.shape,
            r.cells,
            r.fabric_bytes,
            r.baseline_bytes,
            r.fabric_bytes as f64 / r.baseline_bytes as f64,
            r.print_ns as f64 / 1000.0,
            r.baseline_print_ns as f64 / 1000.0,
            r.parse_ns as f64 / 1000.0,
            r.verify_ns as f64 / 1000.0,
        ));
    }
    out.push_str("\nhistory overhead (4-pass pipeline on foldchain-N):\n");
    out.push_str("shape          orig-B  final-B  history-B  hist/final\n");
    for n in [20u64, 100, 400] {
        let f = foldchain(n);
        if let Ok((ob, fb, hb)) = history_overhead(&f) {
            out.push_str(&format!(
                "foldchain-{:<6} {:>7} {:>8} {:>10} {:>11.1}\n",
                n,
                ob,
                fb,
                hb,
                hb as f64 / fb as f64
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::verify;

    #[test]
    fn shapes_verify() {
        for (name, f) in [
            ("chain", chain(30)),
            ("dag", dense_dag(40, 7)),
            ("foldchain", foldchain(30)),
        ] {
            assert!(verify(&f).is_ok(), "{} must verify", name);
        }
        let d = diamonds(5);
        assert!(verify(&d).is_ok(), "diamonds must verify");
    }

    #[test]
    fn baseline_is_smaller_but_same_order() {
        // The honest expectation: our format carries more explicit
        // vocabulary; the baseline must not be wildly smaller.
        let f = chain(100);
        let ours = crate::text::print(&f).len();
        let base = baseline_ssa(&f).len();
        assert!(base < ours, "baseline should be smaller ({} vs {})", base, ours);
        assert!((ours as f64) / (base as f64) < 2.5, "fabric text should be within 2.5x of baseline");
    }

    #[test]
    fn foldchain_pipeline_collapses() {
        // the point of foldchain: heavy history, tiny final
        let f = foldchain(50);
        let (ob, fb, hb) = history_overhead(&f).unwrap();
        assert!(fb < ob, "final fabric must be smaller than original");
        assert!(hb > fb, "history must outgrow the final fabric");
    }
}

//! R1 lane 2 — generator SHAPE AUDIT (docs/phase/SHAPE-AUDIT.md).
//!
//! Runs the SAME corpus as the published claims (`fuzz` defaults: 10,000
//! fabrics, seed base 0xFAB1C) and measures the SHAPE of what the
//! generator actually emits: phi count, ctrl-edge count, call depth,
//! region count, type coverage, cyclic-region frequency — plus the
//! biases that bound every "10,000/10,000, 0 failures" claim.
//!
//! Zero-dependency, deterministic, read-only over the generator.

use llvm_fabric::cell::{ArithOp, Cell, CellKind, CmpOp};
use llvm_fabric::ctrl;
use llvm_fabric::fabric::Fabric;
use llvm_fabric::fuzz::{gen_fabric, Rng};
use llvm_fabric::id::{CellId, RegionId};
use llvm_fabric::ty::{ConstVal, Type};
use llvm_fabric::verify::verify;
use std::collections::{BTreeMap, BTreeSet};

fn kind_name(c: &Cell) -> &'static str {
    match &c.kind {
        CellKind::Param { .. } => "param",
        CellKind::Const { .. } => "const",
        CellKind::Arith { .. } => "arith",
        CellKind::Cmp { .. } => "cmp",
        CellKind::Phi { .. } => "phi",
        CellKind::Branch { .. } => "branch",
        CellKind::Jump { .. } => "jump",
        CellKind::Ret => "ret",
        CellKind::Call { .. } => "call",
    }
}

fn ty_name(t: Option<Type>) -> &'static str {
    match t {
        Some(Type::I1) => "i1",
        Some(Type::I32) => "i32",
        Some(Type::I64) => "i64",
        Some(Type::F64) => "f64",
        None => "-",
    }
}

fn reaches(f: &Fabric, from: RegionId, to: RegionId) -> bool {
    // forward BFS over ctrl edges (path length >= 1)
    let mut seen = BTreeSet::new();
    let mut work = vec![from];
    while let Some(r) = work.pop() {
        for &s in f.successors(r) {
            if s == to {
                return true;
            }
            if seen.insert(s) {
                work.push(s);
            }
        }
    }
    false
}

fn reachable_from_entry(f: &Fabric) -> BTreeSet<RegionId> {
    let mut seen: BTreeSet<RegionId> = [RegionId(0)].into_iter().collect();
    let mut work = vec![RegionId(0)];
    while let Some(r) = work.pop() {
        for &s in f.successors(r) {
            if seen.insert(s) {
                work.push(s);
            }
        }
    }
    seen
}

#[derive(Default)]
struct Audit {
    fabrics: u64,
    gen_verify_fail: u64,
    cells_total: u64,
    cells_min: u64,
    cells_max: u64,
    value_cells: u64,
    dead_value_cells: u64,
    // per-fabric buckets
    cells_hist: BTreeMap<String, u64>,
    regions_hist: BTreeMap<u64, u64>,
    ctrl_edges_hist: BTreeMap<u64, u64>,
    phis_hist: BTreeMap<u64, u64>,
    // kinds
    kind_count: BTreeMap<&'static str, u64>,
    // ctrl graph
    ctrl_edges_total: u64,
    unique_edges_total: u64,
    self_loops: u64,
    fabrics_self_loop: u64,
    back_edges: u64, // edge u->v where v reaches u (closes a cycle)
    fabrics_cyclic: u64,
    cyclic_regions: u64,
    fabrics_cycle_from_entry: u64,
    unreachable_regions: u64,
    fabrics_with_unreachable: u64,
    // phis
    phis: u64,
    phi_ty: BTreeMap<&'static str, u64>,
    phi_join_size: BTreeMap<usize, u64>,
    phi_at_head: u64,
    phi_not_at_head: u64,
    phi_in_entry: u64,
    phi_same_region_operand: u64,
    phi_partial_coverage: u64, // joins != preds of the region
    phi_consumers: BTreeMap<&'static str, u64>,
    phi_unconsumed: u64,
    phis_per_region_max: u64,
    // calls
    call_cells: u64,
    call_depth_max: u64,
    // types
    value_by_ty: BTreeMap<&'static str, u64>,
    wires_by_ty: BTreeMap<&'static str, u64>,
    fabrics_with_ty: BTreeMap<&'static str, u64>,
    // consts
    const_by_ty: BTreeMap<&'static str, u64>,
    i32_min: i32,
    i32_max: i32,
    i32_neg: u64,
    i64_min: i64,
    i64_max: i64,
    i64_neg: u64,
    f64_min: f64,
    f64_max: f64,
    f64_non_eighth: u64,
    i1_true: u64,
    i1_false: u64,
    distinct_consts: BTreeSet<String>,
    // ops
    arith_op: BTreeMap<&'static str, u64>,
    arith_dup_operands: u64,
    arith_div_const_zero_rhs: u64,
    cmp_op: BTreeMap<&'static str, u64>,
    cmp_operand_ty: BTreeMap<&'static str, u64>,
    // branch / ret
    branch_cond_kind: BTreeMap<&'static str, u64>,
    branch_eq_targets: u64,
    ret_with_value: u64,
    ret_void: u64,
    ret_value_from_phi: u64,
    // params
    params: u64,
    param_ty: BTreeMap<&'static str, u64>,
}

impl Audit {
    fn new() -> Audit {
        Audit {
            cells_min: u64::MAX,
            i32_min: i32::MAX,
            i32_max: i32::MIN,
            i64_min: i64::MAX,
            i64_max: i64::MIN,
            f64_min: f64::INFINITY,
            f64_max: f64::NEG_INFINITY,
            ..Default::default()
        }
    }

    fn bump(m: &mut BTreeMap<&'static str, u64>, k: &'static str) {
        *m.entry(k).or_insert(0) += 1;
    }

    fn fabric(&mut self, f: &Fabric) {
        self.fabrics += 1;
        let ids: Vec<CellId> = f.cells().collect();
        let n_cells = ids.len() as u64;
        self.cells_total += n_cells;
        self.cells_min = self.cells_min.min(n_cells);
        self.cells_max = self.cells_max.max(n_cells);
        let bucket = match n_cells {
            0..=10 => "  1-10",
            11..=20 => " 11-20",
            21..=30 => " 21-30",
            31..=40 => " 31-40",
            _ => "   >40",
        };
        *self.cells_hist.entry(bucket.to_string()).or_insert(0) += 1;

        // ---- kinds, users, deadness, types ----
        let mut users: BTreeMap<CellId, u64> = BTreeMap::new();
        let mut phis_in_region: BTreeMap<RegionId, u64> = BTreeMap::new();
        let mut ty_present: BTreeSet<&'static str> = BTreeSet::new();
        for &id in &ids {
            let c = f.cell(id).unwrap();
            Self::bump(&mut self.kind_count, kind_name(c));
            for &op in &c.operands {
                *users.entry(op).or_insert(0) += 1;
                Self::bump(&mut self.wires_by_ty, ty_name(f.ty_of(op)));
            }
            if c.produces_value() {
                let t = f.ty_of(id);
                Self::bump(&mut self.value_by_ty, ty_name(t));
                if let Some(t) = t {
                    ty_present.insert(ty_name(Some(t)));
                }
                if let CellKind::Call { .. } = &c.kind {
                    self.call_cells += 1;
                }
            }
            if let CellKind::Phi { .. } = &c.kind {
                *phis_in_region.entry(c.region).or_insert(0) += 1;
            }
        }
        for t in ty_present {
            *self.fabrics_with_ty.entry(t).or_insert(0) += 1;
        }
        self.phis_per_region_max = self.phis_per_region_max.max(phis_in_region.values().copied().max().unwrap_or(0));

        // dead-on-arrival: value cell with zero users
        let mut n_value = 0u64;
        let mut n_dead = 0u64;
        for &id in &ids {
            let c = f.cell(id).unwrap();
            if c.produces_value() {
                n_value += 1;
                if users.get(&id).copied().unwrap_or(0) == 0 {
                    n_dead += 1;
                    if matches!(c.kind, CellKind::Phi { .. }) {
                        self.phi_unconsumed += 1;
                    }
                }
            }
        }
        self.value_cells += n_value;
        self.dead_value_cells += n_dead;

        // ---- ctrl graph ----
        let n_regions = f.regions.len() as u64;
        *self.regions_hist.entry(n_regions).or_insert(0) += 1;
        let mut edges: Vec<(RegionId, RegionId)> = vec![]; // multiplicity kept
        for (ri, _) in f.regions.iter().enumerate() {
            let r = RegionId(ri as u32);
            if let Some(region) = f.region(r) {
                if let Some(&last) = region.cells.last() {
                    if let Some(c) = f.cell(last) {
                        match &c.kind {
                            CellKind::Branch { then_r, else_r } => {
                                if then_r == else_r {
                                    self.branch_eq_targets += 1;
                                }
                                edges.push((r, *then_r));
                                edges.push((r, *else_r));
                            }
                            CellKind::Jump { target } => edges.push((r, *target)),
                            _ => {}
                        }
                    }
                }
            }
        }
        self.ctrl_edges_total += edges.len() as u64;
        *self.ctrl_edges_hist.entry(edges.len() as u64).or_insert(0) += 1;
        let unique: BTreeSet<(RegionId, RegionId)> = edges.iter().copied().collect();
        self.unique_edges_total += unique.len() as u64;
        let mut has_self = false;
        for &(u, v) in &edges {
            if u == v {
                self.self_loops += 1;
                has_self = true;
            }
            if reaches(f, v, u) {
                self.back_edges += 1;
            }
        }
        if has_self {
            self.fabrics_self_loop += 1;
        }
        // cycles: r on a cycle iff backward closure from r contains r
        let reach = reachable_from_entry(f);
        let mut cyclic = false;
        let mut cyclic_from_entry = false;
        for (ri, _) in f.regions.iter().enumerate() {
            let r = RegionId(ri as u32);
            let closure = ctrl::controlling_regions(f, r);
            if closure.contains(&r) {
                cyclic = true;
                self.cyclic_regions += 1;
                if reach.contains(&r) {
                    cyclic_from_entry = true;
                }
            }
            if !reach.contains(&r) {
                self.unreachable_regions += 1;
            }
        }
        if cyclic {
            self.fabrics_cyclic += 1;
        }
        if cyclic_from_entry {
            self.fabrics_cycle_from_entry += 1;
        }
        if f.regions.iter().enumerate().any(|(ri, _)| !reach.contains(&RegionId(ri as u32))) {
            self.fabrics_with_unreachable += 1;
        }

        // ---- phis ----
        let mut n_phis = 0u64;
        for &id in &ids {
            let c = f.cell(id).unwrap();
            if let CellKind::Phi { joins } = &c.kind {
                n_phis += 1;
                *self.phi_join_size.entry(joins.len()).or_insert(0) += 1;
                Self::bump(&mut self.phi_ty, ty_name(f.ty_of(id)));
                let idx = f.index_in_region(id).unwrap_or(0);
                if idx == 0 {
                    self.phi_at_head += 1;
                } else {
                    self.phi_not_at_head += 1;
                }
                if c.region == RegionId(0) {
                    self.phi_in_entry += 1;
                }
                for (i, &op) in c.operands.iter().enumerate() {
                    if let Some(src) = f.cell(op) {
                        if src.region == c.region {
                            self.phi_same_region_operand += 1;
                        }
                        let _ = i;
                    }
                }
                let preds = f.predecessors(c.region);
                let mut covered: BTreeSet<RegionId> = joins.iter().copied().collect();
                for &p in preds {
                    covered.remove(&p);
                }
                if !covered.is_empty() || joins.len() != preds.len() {
                    self.phi_partial_coverage += 1;
                }
            }
        }
        self.phis += n_phis;
        *self.phis_hist.entry(n_phis.min(4)).or_insert(0) += 1;
        // phi consumers + ret/branch sources
        for &id in &ids {
            let c = f.cell(id).unwrap();
            for &op in &c.operands {
                if let Some(src) = f.cell(op) {
                    if matches!(src.kind, CellKind::Phi { .. }) {
                        Self::bump(&mut self.phi_consumers, kind_name(c));
                    }
                }
            }
            match &c.kind {
                CellKind::Arith { op, .. } => {
                    Self::bump(&mut self.arith_op, op.name());
                    if c.operands.len() == 2 && c.operands[0] == c.operands[1] {
                        self.arith_dup_operands += 1;
                    }
                    if *op == ArithOp::Div {
                        if let Some(rhs) = f.cell(c.operands[1]) {
                            if let CellKind::Const { val, .. } = &rhs.kind {
                                let zero = match val {
                                    ConstVal::I1(b) => !*b,
                                    ConstVal::I32(v) => *v == 0,
                                    ConstVal::I64(v) => *v == 0,
                                    ConstVal::F64(v) => *v == 0.0,
                                };
                                if zero {
                                    self.arith_div_const_zero_rhs += 1;
                                }
                            }
                        }
                    }
                }
                CellKind::Cmp { op } => {
                    Self::bump(&mut self.cmp_op, op.name());
                    Self::bump(&mut self.cmp_operand_ty, ty_name(f.ty_of(c.operands[0])));
                }
                CellKind::Branch { then_r, else_r } => {
                    let _ = (then_r, else_r);
                    Self::bump(&mut self.branch_cond_kind, kind_name(f.cell(c.operands[0]).unwrap()));
                }
                CellKind::Ret => {
                    if c.operands.is_empty() {
                        self.ret_void += 1;
                    } else {
                        self.ret_with_value += 1;
                        if matches!(f.cell(c.operands[0]).map(|x| &x.kind), Some(CellKind::Phi { .. })) {
                            self.ret_value_from_phi += 1;
                        }
                    }
                }
                CellKind::Param { ty } => {
                    self.params += 1;
                    Self::bump(&mut self.param_ty, ty_name(Some(*ty)));
                }
                CellKind::Const { ty, val } => {
                    Self::bump(&mut self.const_by_ty, ty_name(Some(*ty)));
                    self.distinct_consts.insert(format!("{:?}", val));
                    match val {
                        ConstVal::I1(b) => {
                            if *b {
                                self.i1_true += 1;
                            } else {
                                self.i1_false += 1;
                            }
                        }
                        ConstVal::I32(v) => {
                            self.i32_min = self.i32_min.min(*v);
                            self.i32_max = self.i32_max.max(*v);
                            if *v < 0 {
                                self.i32_neg += 1;
                            }
                        }
                        ConstVal::I64(v) => {
                            self.i64_min = self.i64_min.min(*v);
                            self.i64_max = self.i64_max.max(*v);
                            if *v < 0 {
                                self.i64_neg += 1;
                            }
                        }
                        ConstVal::F64(v) => {
                            self.f64_min = self.f64_min.min(*v);
                            self.f64_max = self.f64_max.max(*v);
                            if (v * 8.0).fract() != 0.0 {
                                self.f64_non_eighth += 1;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn parse_u64(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()
    } else {
        t.parse().ok()
    }
}

fn main() {
    let mut iters: u64 = 10_000;
    let mut seed0: u64 = 0xFAB1C;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--iters" => {
                i += 1;
                iters = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(10_000);
            }
            "--seed" => {
                i += 1;
                let s = args.get(i).cloned().unwrap_or_default();
                seed0 = parse_u64(&s).unwrap_or_else(|| panic!("--seed needs a decimal or 0x-hex number, got {:?}", s));
            }
            other => panic!("unknown arg {}", other),
        }
        i += 1;
    }

    let mut a = Audit::new();
    for i in 0..iters {
        let seed = seed0.wrapping_add(i).max(1);
        let f = gen_fabric(&mut Rng::new(seed));
        if verify(&f).is_err() {
            a.gen_verify_fail += 1;
            continue;
        }
        a.fabric(&f);
    }

    println!("# shape-audit raw dump (iters={}, seed0={:#x})", iters, seed0);
    println!("fabrics verified:            {}  (gen_verify_fail: {})", a.fabrics, a.gen_verify_fail);
    println!("cells total:                 {}  min {}  max {}  mean {:.2}", a.cells_total, a.cells_min, a.cells_max, a.cells_total as f64 / a.fabrics as f64);
    println!("value cells:                 {}  dead-on-arrival {} ({:.1}% of value cells)", a.value_cells, a.dead_value_cells, 100.0 * a.dead_value_cells as f64 / a.value_cells as f64);
    println!();
    println!("## cells-per-fabric histogram");
    for (k, v) in &a.cells_hist {
        println!("  {:>6}: {} ({:.2}%)", k, v, 100.0 * *v as f64 / a.fabrics as f64);
    }
    println!("## regions-per-fabric histogram");
    for (k, v) in &a.regions_hist {
        println!("  {} regions: {} ({:.2}%)", k, v, 100.0 * *v as f64 / a.fabrics as f64);
    }
    println!("## ctrl-edges-per-fabric histogram (br=2, jmp=1, ret=0)");
    for (k, v) in &a.ctrl_edges_hist {
        println!("  {} edges: {} ({:.2}%)", k, v, 100.0 * *v as f64 / a.fabrics as f64);
    }
    println!("## phis-per-fabric histogram (4 = 4+, cap)");
    for (k, v) in &a.phis_hist {
        println!("  {} phis: {} ({:.2}%)", k, v, 100.0 * *v as f64 / a.fabrics as f64);
    }
    println!();
    println!("## cell-kind census (all fabrics)");
    for (k, v) in &a.kind_count {
        println!("  {:>8}: {} ({:.3}/fabric)", k, v, *v as f64 / a.fabrics as f64);
    }
    println!();
    println!("## ctrl graph");
    println!("  ctrl edges (with multiplicity): {}", a.ctrl_edges_total);
    println!("  unique region->region edges:    {}", a.unique_edges_total);
    println!("  branch with equal targets:      {}", a.branch_eq_targets);
    println!("  self-loops (jmp r->r):          {}  fabrics with one: {}", a.self_loops, a.fabrics_self_loop);
    println!("  back/cycle edges (u->v, v=>u):  {}", a.back_edges);
    println!("  FABRICS WITH >=1 CYCLE:         {} ({:.2}%)", a.fabrics_cyclic, 100.0 * a.fabrics_cyclic as f64 / a.fabrics as f64);
    println!("  regions on a cycle:             {} ({:.3}/fabric)", a.cyclic_regions, a.cyclic_regions as f64 / a.fabrics as f64);
    println!("  cycles reachable from entry:    {} fabrics ({:.2}%)", a.fabrics_cycle_from_entry, 100.0 * a.fabrics_cycle_from_entry as f64 / a.fabrics as f64);
    println!("  unreachable regions:            {} total; fabrics with >=1: {} ({:.2}%)", a.unreachable_regions, a.fabrics_with_unreachable, 100.0 * a.fabrics_with_unreachable as f64 / a.fabrics as f64);
    println!();
    println!("## phis");
    println!("  total phis:                {} ({:.3}/fabric)", a.phis, a.phis as f64 / a.fabrics as f64);
    println!("  phis per region (max seen): {}", a.phis_per_region_max);
    for (k, v) in &a.phi_ty {
        println!("  phi type {}: {}", k, v);
    }
    for (k, v) in &a.phi_join_size {
        println!("  join size {}: {}", k, v);
    }
    println!("  at spine head:             {} / not at head: {}", a.phi_at_head, a.phi_not_at_head);
    println!("  in entry region:           {}", a.phi_in_entry);
    println!("  same-region operands:      {}", a.phi_same_region_operand);
    println!("  partial pred coverage:     {}", a.phi_partial_coverage);
    println!("  unconsumed phis:           {}", a.phi_unconsumed);
    println!("  phi consumers by kind:");
    for (k, v) in &a.phi_consumers {
        println!("    {:>8}: {}", k, v);
    }
    println!();
    println!("## calls");
    println!("  call cells: {}   max call depth: {}", a.call_cells, a.call_depth_max);
    println!();
    println!("## type coverage");
    println!("  value cells by type:");
    for (k, v) in &a.value_by_ty {
        println!("    {}: {}", k, v);
    }
    println!("  use-wires by source type:");
    for (k, v) in &a.wires_by_ty {
        println!("    {}: {}", k, v);
    }
    println!("  fabrics containing at least one value of type:");
    for (k, v) in &a.fabrics_with_ty {
        println!("    {}: {} ({:.2}%)", k, v, 100.0 * *v as f64 / a.fabrics as f64);
    }
    println!("  consts by type: {:?}", a.const_by_ty);
    println!("  distinct const values: {}", a.distinct_consts.len());
    println!("  i32 consts: min {} max {} negatives {}", a.i32_min, a.i32_max, a.i32_neg);
    println!("  i64 consts: min {} max {} negatives {}", a.i64_min, a.i64_max, a.i64_neg);
    println!("  f64 consts: min {} max {} non-multiples-of-0.125: {}", a.f64_min, a.f64_max, a.f64_non_eighth);
    println!("  i1 consts: true {} false {}", a.i1_true, a.i1_false);
    println!();
    println!("## ops");
    println!("  arith ops: {:?}", a.arith_op);
    println!("  arith with duplicate operands (a==b): {}", a.arith_dup_operands);
    println!("  arith.div with const-zero RHS:         {}", a.arith_div_const_zero_rhs);
    println!("  cmp ops:  {:?}", a.cmp_op);
    println!("  cmp operand types: {:?}", a.cmp_operand_ty);
    println!("  branch cond source kinds: {:?}", a.branch_cond_kind);
    println!("  ret: with value {} / void {} / value-is-phi {}", a.ret_with_value, a.ret_void, a.ret_value_from_phi);
    println!("  params: {} types {:?}", a.params, a.param_ty);
    let _ = CmpOp::Eq;
}

// ===========================================================================
// Tests — validate the MEASURER against independent ground truth (R1 lane 4).
//
// Three layers:
//   1. corpus_totals_*: the full 10k published corpus reproduces the
//      EXPERIMENTS §9.2 numbers from scratch (phis 15,333 / cells 255,446
//      @0xFAB1C; 255,198 @0xD3CA5) — not parsed from the dump, regenerated.
//   2. histogram_invariants_*: hand-built corpus — every histogram sums to
//      the fabric count, every census sums to the cells total.
//   3. detector_*: synthetic fixtures where the cannot-emit detectors fire
//      (or stay silent) by construction.
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn audit_of(fabrics: &[Fabric]) -> Audit {
        let mut a = Audit::new();
        for f in fabrics {
            a.fabric(f);
        }
        a
    }

    fn run_corpus(iters: u64, seed0: u64) -> Audit {
        let mut a = Audit::new();
        for i in 0..iters {
            let seed = seed0.wrapping_add(i).max(1);
            let f = gen_fabric(&mut Rng::new(seed));
            if verify(&f).is_err() {
                a.gen_verify_fail += 1;
                continue;
            }
            a.fabric(&f);
        }
        a
    }

    // ---- hand-built fabric helpers (deliberately NOT via gen_fabric) ----

    fn simple_fabric(n_consts: usize) -> Fabric {
        // entry: n_consts consts + ret(void)
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        for i in 0..n_consts {
            f.add_cell(
                r0,
                Cell::new(r0, CellKind::Const { ty: Type::I32, val: ConstVal::I32(i as i32) }),
            );
        }
        f.add_cell(r0, Cell::new(r0, CellKind::Ret));
        f
    }

    fn wire(f: &mut Fabric, user: CellId, slot: usize, from: CellId) {
        let ops = &mut f.cell_mut(user).unwrap().operands;
        while ops.len() <= slot {
            ops.push(CellId(u32::MAX));
        }
        ops[slot] = from;
    }

    /// entry: c0=const0, phi@head, ret(phi). One region, one phi.
    fn phi_at_head_fabric() -> Fabric {
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        let phi = f.add_cell(r0, Cell::new(r0, CellKind::Phi { joins: vec![r0] }));
        let c0 = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        wire(&mut f, phi, 0, c0);
        let ret = f.add_cell(r0, Cell::new(r0, CellKind::Ret));
        wire(&mut f, ret, 0, phi);
        f
    }

    /// entry: const, phi (NOT at head), ret(phi) — phi at index 1.
    fn phi_not_at_head_fabric() -> Fabric {
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        let c0 = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let filler = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }));
        let phi = f.add_cell(r0, Cell::new(r0, CellKind::Phi { joins: vec![r0] }));
        wire(&mut f, phi, 0, c0);
        let ret = f.add_cell(r0, Cell::new(r0, CellKind::Ret));
        wire(&mut f, ret, 0, phi);
        let _ = filler;
        f
    }

    /// Two regions; branch with EQUAL targets (then == else).
    fn branch_eq_targets_fabric() -> Fabric {
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        let r1 = f.add_region("join");
        let c = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }));
        let br = f.add_cell(r0, Cell::new(r0, CellKind::Branch { then_r: r1, else_r: r1 }));
        wire(&mut f, br, 0, c);
        f.add_cell(r1, Cell::new(r1, CellKind::Ret));
        f
    }

    /// r0: ret. r1: ret. r1 unreachable from entry.
    fn unreachable_region_fabric() -> Fabric {
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        let r1 = f.add_region("island");
        f.add_cell(r0, Cell::new(r0, CellKind::Ret));
        f.add_cell(r1, Cell::new(r1, CellKind::Ret));
        f
    }

    /// r0: ret; r1: jump back to r1 — self-loop on an unreachable region.
    fn self_loop_fabric() -> Fabric {
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        let r1 = f.add_region("loop");
        f.add_cell(r0, Cell::new(r0, CellKind::Ret));
        f.add_cell(r1, Cell::new(r1, CellKind::Jump { target: r1 }));
        f
    }

    /// r0: c0=1, c1=0, div = c0/c1 (const-zero RHS), ret(div).
    fn div_const_zero_fabric() -> Fabric {
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        let c0 = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let cz = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I32, val: ConstVal::I32(0) }));
        let div = f.add_cell(r0, Cell::new(r0, CellKind::Arith { op: ArithOp::Div, ty: Type::I32 }));
        f.cell_mut(div).unwrap().operands = vec![c0, cz];
        let ret = f.add_cell(r0, Cell::new(r0, CellKind::Ret));
        wire(&mut f, ret, 0, div);
        f
    }

    /// r0: c0=1, dup = c0+c0 (duplicate operands), ret(dup).
    fn dup_operand_fabric() -> Fabric {
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        let c0 = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let add = f.add_cell(r0, Cell::new(r0, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 }));
        f.cell_mut(add).unwrap().operands = vec![c0, c0];
        let ret = f.add_cell(r0, Cell::new(r0, CellKind::Ret));
        wire(&mut f, ret, 0, add);
        f
    }

    /// Back edge: r0 branches to r1, r1 jumps back to r0 — r1->r0 is a
    /// back edge (r0 reaches r1), both regions cyclic.
    fn two_region_cycle_fabric() -> Fabric {
        let mut f = Fabric::empty();
        let r0 = f.add_region("a");
        let r1 = f.add_region("b");
        let c = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }));
        let br = f.add_cell(r0, Cell::new(r0, CellKind::Branch { then_r: r1, else_r: r1 }));
        wire(&mut f, br, 0, c);
        f.add_cell(r1, Cell::new(r1, CellKind::Jump { target: r0 }));
        f
    }

    /// Two value cells, one consumed (ret), one dead. Also an unconsumed phi.
    fn dead_cells_fabric() -> Fabric {
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        let c0 = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I32, val: ConstVal::I32(7) }));
        let dead = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I64, val: ConstVal::I64(9) }));
        let phi = f.add_cell(r0, Cell::new(r0, CellKind::Phi { joins: vec![r0] }));
        wire(&mut f, phi, 0, c0);
        let ret = f.add_cell(r0, Cell::new(r0, CellKind::Ret));
        wire(&mut f, ret, 0, c0);
        let _ = dead;
        f
    }

    fn const_f64(v: f64) -> Fabric {
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::F64, val: ConstVal::F64(v) }));
        f.add_cell(r0, Cell::new(r0, CellKind::Ret));
        f
    }

    /// A fabric with no terminator at all — ctrl-edge count 0, no crash.
    fn no_terminator_fabric() -> Fabric {
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I1, val: ConstVal::I1(false) }));
        f
    }

    // =====================================================================
    // Layer 1 — corpus totals vs EXPERIMENTS §9.2 (independent ground truth)
    // =====================================================================

    #[test]
    fn corpus_totals_match_experiments_s92_fab1c() {
        let a = run_corpus(10_000, 0xFAB1C);
        assert_eq!(a.fabrics, 10_000, "every generated fabric must verify");
        assert_eq!(a.gen_verify_fail, 0);
        assert_eq!(a.cells_total, 255_446, "cells total must match EXPERIMENTS §9.2");
        assert_eq!(a.phis, 15_333, "phi total must match EXPERIMENTS §9.2");
    }

    #[test]
    fn corpus_totals_match_experiments_s92_d3ca5() {
        let a = run_corpus(10_000, 0xD3CA5);
        assert_eq!(a.fabrics, 10_000);
        assert_eq!(a.cells_total, 255_198, "stability-run cells total must match §9.2");
    }

    #[test]
    fn corpus_determinism_replays_identically() {
        let a1 = run_corpus(500, 0xFAB1C);
        let a2 = run_corpus(500, 0xFAB1C);
        assert_eq!(a1.cells_total, a2.cells_total);
        assert_eq!(a1.phis, a2.phis);
        assert_eq!(a1.ctrl_edges_total, a2.ctrl_edges_total);
        assert_eq!(a1.fabrics_cyclic, a2.fabrics_cyclic);
    }

    // =====================================================================
    // Layer 2 — histogram / census invariants on a hand-built corpus
    // =====================================================================

    fn hand_corpus() -> Vec<Fabric> {
        vec![
            simple_fabric(2),
            simple_fabric(5),
            simple_fabric(12), // 11-20 bucket
            simple_fabric(44), // >40 bucket
            phi_at_head_fabric(),
            branch_eq_targets_fabric(),
            two_region_cycle_fabric(),
            self_loop_fabric(),
            no_terminator_fabric(),
        ]
    }

    #[test]
    fn histograms_sum_to_fabric_count() {
        let a = audit_of(&hand_corpus());
        let n = a.fabrics;
        assert_eq!(n, 9);
        for (name, sum) in [
            ("cells_hist", a.cells_hist.values().sum::<u64>()),
            ("regions_hist", a.regions_hist.values().sum::<u64>()),
            ("ctrl_edges_hist", a.ctrl_edges_hist.values().sum::<u64>()),
            ("phis_hist", a.phis_hist.values().sum::<u64>()),
        ] {
            assert_eq!(sum, n, "{} must account for every fabric exactly once", name);
        }
    }

    #[test]
    fn kind_census_sums_to_cells_total() {
        let a = audit_of(&hand_corpus());
        assert_eq!(
            a.kind_count.values().sum::<u64>(),
            a.cells_total,
            "cell-kind census must account for every cell"
        );
        assert_eq!(a.cells_min, 1); // no_terminator fabric: 1 cell
        assert!(a.cells_max >= 13); // simple_fabric(12) + ret
    }

    #[test]
    fn ctrl_edge_multiplicity_counted() {
        // branch = 2 edges (even when targets are equal), jump = 1, ret = 0
        let a = audit_of(&[branch_eq_targets_fabric(), two_region_cycle_fabric()]);
        // branch_eq: br(then=else) = 2 edges; cycle: br = 2 + jump = 1 => 3; total 5
        assert_eq!(a.ctrl_edges_total, 5);
    }

    // =====================================================================
    // Layer 3 — cannot-emit detectors on synthetic fixtures
    // =====================================================================

    #[test]
    fn detector_self_loop() {
        let a = audit_of(&[self_loop_fabric()]);
        assert_eq!(a.self_loops, 1);
        assert_eq!(a.fabrics_self_loop, 1);
        assert_eq!(a.fabrics_cyclic, 1);
        assert_eq!(a.cyclic_regions, 1);
    }

    #[test]
    fn detector_back_edge_two_region_cycle() {
        let a = audit_of(&[two_region_cycle_fabric()]);
        // Definition: edge u->v is back iff v reaches u. In a 2-region
        // cycle each region reaches the other, so ALL 3 edges qualify:
        // 2x (r0->r1: r1 jumps to r0) + 1x (r1->r0: r0 branches to r1).
        assert_eq!(a.back_edges, 3);
        assert_eq!(a.fabrics_cyclic, 1);
        assert_eq!(a.cyclic_regions, 2, "both regions lie on the cycle");
        assert_eq!(a.fabrics_cycle_from_entry, 1, "cycle starts at entry");
        assert_eq!(a.self_loops, 0);
    }

    #[test]
    fn detector_unreachable_region() {
        let a = audit_of(&[unreachable_region_fabric()]);
        assert_eq!(a.unreachable_regions, 1);
        assert_eq!(a.fabrics_with_unreachable, 1);
        assert_eq!(a.fabrics_cycle_from_entry, 0);
    }

    #[test]
    fn detector_branch_equal_targets() {
        let a = audit_of(&[branch_eq_targets_fabric()]);
        assert_eq!(a.branch_eq_targets, 1);
        // equal targets still count as 2 ctrl edges (multiplicity)
        assert_eq!(a.ctrl_edges_total, 2);
        assert_eq!(a.unique_edges_total, 1);
    }

    #[test]
    fn detector_div_const_zero_rhs() {
        let a = audit_of(&[div_const_zero_fabric()]);
        assert_eq!(a.arith_div_const_zero_rhs, 1);
        let clean = audit_of(&[dup_operand_fabric()]);
        assert_eq!(clean.arith_div_const_zero_rhs, 0);
    }

    #[test]
    fn detector_duplicate_arith_operands() {
        let a = audit_of(&[dup_operand_fabric()]);
        assert_eq!(a.arith_dup_operands, 1);
    }

    #[test]
    fn detector_phi_head_position() {
        let a = audit_of(&[phi_at_head_fabric()]);
        assert_eq!(a.phi_at_head, 1);
        assert_eq!(a.phi_not_at_head, 0);
        let b = audit_of(&[phi_not_at_head_fabric()]);
        assert_eq!(b.phi_at_head, 0);
        assert_eq!(b.phi_not_at_head, 1);
    }

    #[test]
    fn detector_phi_in_entry_region() {
        let a = audit_of(&[phi_at_head_fabric()]);
        assert_eq!(a.phi_in_entry, 1);
        assert_eq!(a.phis, 1);
        assert_eq!(a.phi_join_size.get(&1), Some(&1));
    }

    #[test]
    fn detector_dead_and_unconsumed() {
        let a = audit_of(&[dead_cells_fabric()]);
        // consts c0 (consumed by phi + ret), dead i64, unconsumed phi
        assert_eq!(a.value_cells, 3);
        assert_eq!(a.dead_value_cells, 2); // dead i64 const + unconsumed phi
        assert_eq!(a.phi_unconsumed, 1);
    }

    #[test]
    fn detector_f64_eighth_multiples() {
        let a = audit_of(&[const_f64(0.125)]);
        assert_eq!(a.f64_non_eighth, 0, "0.125 is a multiple of 1/8");
        let b = audit_of(&[const_f64(0.3)]);
        assert_eq!(b.f64_non_eighth, 1, "0.3 is not a multiple of 1/8");
    }

    #[test]
    fn detector_ret_void_vs_value() {
        let a = audit_of(&[simple_fabric(2)]);
        assert_eq!(a.ret_void, 1);
        assert_eq!(a.ret_with_value, 0);
        let b = audit_of(&[div_const_zero_fabric()]);
        assert_eq!(b.ret_with_value, 1);
        assert_eq!(b.ret_void, 0);
        assert_eq!(b.ret_value_from_phi, 0);
        // ret(phi) counts as value-from-phi
        let c = audit_of(&[phi_at_head_fabric()]);
        assert_eq!(c.ret_value_from_phi, 1);
    }

    #[test]
    fn detector_acyclic_fabric_silent() {
        // All-negative control: simple fabric must fire NOTHING.
        let a = audit_of(&[simple_fabric(3), branch_eq_targets_fabric()]);
        // remove the branch-eq fabric's contribution by auditing it alone:
        let b = audit_of(&[branch_eq_targets_fabric()]);
        let a_minus_b_self = a.self_loops - b.self_loops;
        assert_eq!(a_minus_b_self, 0);
        let clean = audit_of(&[simple_fabric(3)]);
        assert_eq!(clean.self_loops, 0);
        assert_eq!(clean.back_edges, 0);
        assert_eq!(clean.fabrics_cyclic, 0);
        assert_eq!(clean.unreachable_regions, 0);
        assert_eq!(clean.phi_partial_coverage, 0);
        assert_eq!(clean.branch_eq_targets, 0);
        assert_eq!(clean.arith_dup_operands, 0);
        assert_eq!(clean.arith_div_const_zero_rhs, 0);
    }

    #[test]
    fn detector_phi_partial_coverage() {
        // phi in r1 whose joins do not match r1's predecessors:
        // r0 branches to r1 (pred = [r0] twice -> predecessors dedup?),
        // phi joins [r1] (wrong region) => mismatch => detector fires.
        let mut f = Fabric::empty();
        let r0 = f.add_region("entry");
        let r1 = f.add_region("join");
        let c = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }));
        let br = f.add_cell(r0, Cell::new(r0, CellKind::Branch { then_r: r1, else_r: r1 }));
        wire(&mut f, br, 0, c);
        let phi = f.add_cell(r1, Cell::new(r1, CellKind::Phi { joins: vec![r0] }));
        wire(&mut f, phi, 0, c);
        let ret = f.add_cell(r1, Cell::new(r1, CellKind::Ret));
        wire(&mut f, ret, 0, phi);
        let a = audit_of(&[f]);
        // joins == [r0], preds of r1 = {r0} => sizes match, set covered:
        // should NOT fire. Now build the firing variant:
        let mut g = Fabric::empty();
        let r0 = g.add_region("entry");
        let r1 = g.add_region("join");
        let c = g.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }));
        let br = g.add_cell(r0, Cell::new(r0, CellKind::Branch { then_r: r1, else_r: r1 }));
        wire(&mut g, br, 0, c);
        let phi = g.add_cell(r1, Cell::new(r1, CellKind::Phi { joins: vec![r1] }));
        wire(&mut g, phi, 0, c);
        let ret = g.add_cell(r1, Cell::new(r1, CellKind::Ret));
        wire(&mut g, ret, 0, phi);
        let b = audit_of(&[g]);
        assert_eq!(a.phi_partial_coverage, 0, "exact pred coverage must not fire");
        assert_eq!(b.phi_partial_coverage, 1, "join region not a predecessor must fire");
    }
}

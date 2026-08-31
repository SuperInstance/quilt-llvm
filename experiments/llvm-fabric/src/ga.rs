//! GA-CORPUS spike — breed corpus fabrics toward SHAPE-AUDIT's
//! cannot-emit list (docs/phase/SHAPE-AUDIT.md, worktree
//! quilt-llvm-wt-shape; reproduced conclusions quoted below).
//!
//! Engine structure is ported from SuperInstance/mud-arena
//! `src/evolve.py` (575-line GA): random/tournament selection, elite
//! carry, crossover breeding + per-gene mutation, replace-worst,
//! per-generation fitness history. The representation here is a fabric
//! (`Fabric`), not a rule list, so crossover is region-grafting and
//! mutation is a menu of constructive IR edits aimed at specific
//! C-items.
//!
//! Fitness = number of cannot-emit constructs (C1..C11, §1 of the
//! audit) a fabric *legitimately* exercises: verify() must pass, else
//! fitness is 0. A fabric that verifies green while containing a
//! head-phi or a phi→arith wire is exactly what the corpus generator
//! cannot produce — breeding those is the point.
//!
//! Provably unreachable (see tests at the bottom + the doc):
//! - C5 partial phis: V16 requires every predecessor to carry a join.
//! - C7 params outside entry: V12 rejects them outright.
//! - C8 nested regions: the IR has no parent field; nothing to emit.

use crate::cell::{ArithOp, Cell, CellKind};
use crate::fabric::Fabric;
use crate::fuzz::Rng;
use crate::id::{CellId, RegionId};
use crate::ty::{ConstVal, Type};
use crate::verify::verify;
use std::collections::BTreeMap;

pub const C_ITEMS: [&str; 11] = [
    "C1", "C2", "C3", "C4", "C5", "C6", "C7", "C8", "C9", "C10", "C11",
];

/// C-item coverage of one fabric. `n_items` is the fitness-relevant
/// count (only reachable items are ever non-zero).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Coverage {
    pub items: [bool; 11],
    /// C6 depth: wires from a phi into arith/cmp/branch (the audit's
    /// headline blind spot) — used as a tiebreaker so the GA keeps
    /// deepening, not just touching.
    pub phi_consumers: usize,
    pub calls: usize,
    pub boundary_consts: usize,
}

impl Coverage {
    pub fn n_items(&self) -> usize {
        self.items.iter().filter(|b| **b).count()
    }
}

fn ctrl_edges(f: &Fabric) -> usize {
    let mut n = 0;
    for (i, _) in f.regions.iter().enumerate() {
        if let Some(&last) = f.regions[i].cells.last() {
            if let Some(c) = f.cell(last) {
                match &c.kind {
                    CellKind::Branch { .. } => n += 2,
                    CellKind::Jump { .. } => n += 1,
                    _ => {}
                }
            }
        }
    }
    n
}

/// Boundary-valued or negative-domain constant (audit C9: i64 confined
/// to [0, 1e6), i32 to [-500,500), f64 to multiples of 0.125 in
/// [-12.5,49), no NaN/Inf in the corpus).
fn is_boundary(ty: Type, val: &ConstVal) -> bool {
    match val {
        ConstVal::I1(_) => false,
        ConstVal::I32(v) => *v < -500 || *v >= 500,
        ConstVal::I64(v) => *v < 0 || *v >= 1_000_000,
        ConstVal::F64(v) => {
            v.is_infinite() || *v < -12.5 || *v >= 49.0 || (v * 8.0).fract() != 0.0
        }
    }
    .then_some(())
    .is_some()
    && {
        // belt and braces: variant must agree with declared type anyway
        matches!(
            (ty, val),
            (Type::I1, ConstVal::I1(_))
                | (Type::I32, ConstVal::I32(_))
                | (Type::I64, ConstVal::I64(_))
                | (Type::F64, ConstVal::F64(_))
        )
    }
}

/// Measure which C-items a fabric exercises. Called only on fabrics
/// that already pass verify for fitness, but written to be safe on any
/// fabric (missing cells short-circuit to false).
pub fn coverage(f: &Fabric) -> Coverage {
    let mut cov = Coverage::default();
    let mut phis_per_region: BTreeMap<u32, usize> = BTreeMap::new();

    for id in f.cells() {
        let c = match f.cell(id) {
            Some(c) => c,
            None => continue,
        };
        match &c.kind {
            CellKind::Call { .. } => {
                cov.items[0] = true; // C1: call cell (fabric-level; program-level call graph is out of fitness scope)
                cov.calls += 1;
            }
            CellKind::Phi { .. } => {
                if f.ty_of(id) != Some(Type::I32) {
                    cov.items[1] = true; // C2: non-i32 phi
                }
                *phis_per_region.entry(c.region.0).or_insert(0) += 1;
                // C4: phi at spine head (first cell of its region)
                if f.region(c.region).and_then(|r| r.cells.first()) == Some(&id) {
                    cov.items[3] = true;
                }
                // C6: phi feeding arith/cmp/branch computation
                for (user, _) in f.uses_of(id) {
                    if let Some(u) = f.cell(*user) {
                        if matches!(
                            u.kind,
                            CellKind::Arith { .. } | CellKind::Cmp { .. } | CellKind::Branch { .. }
                        ) {
                            cov.items[5] = true;
                            cov.phi_consumers += 1;
                        }
                    }
                }
            }
            CellKind::Const { ty, val } => {
                if is_boundary(*ty, val) {
                    cov.items[8] = true; // C9
                    cov.boundary_consts += 1;
                }
            }
            _ => {}
        }
    }
    // C3: >1 phi in a region
    if phis_per_region.values().any(|n| *n > 1) {
        cov.items[2] = true;
    }
    // C10: size caps exceeded
    let n_cells = f.cells().count();
    if f.regions.len() > 6 || n_cells > 63 || ctrl_edges(f) > 12 {
        cov.items[9] = true;
    }
    // C11: an arith whose slot-0 operand is NOT the most recent
    // same-typed value defined before it in its region (the corpus's
    // chain-shaped operand choice, made optional).
    'outer: for id in f.cells() {
        let c = match f.cell(id) {
            Some(c) => c,
            None => continue,
        };
        if !matches!(c.kind, CellKind::Arith { .. }) {
            continue;
        }
        let op0 = match c.operands.first() {
            Some(&o) if f.cell(o).is_some() => o,
            _ => continue,
        };
        let def = match f.cell(op0) {
            Some(d) => d,
            None => continue,
        };
        if def.region != c.region {
            // operand from entry: latest-position comparison only makes
            // sense same-region; cross-region entry uses are already
            // outside the corpus's chain shape — count them.
            cov.items[10] = true;
            break 'outer;
        }
        let cells_here = match f.region(c.region) {
            Some(r) => &r.cells,
            None => continue,
        };
        let op_pos = cells_here.iter().position(|&x| x == op0);
        let my_pos = cells_here.iter().position(|&x| x == id);
        let (op_pos, my_pos) = match (op_pos, my_pos) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };
        for &other in &cells_here[op_pos + 1..my_pos] {
            if let Some(oc) = f.cell(other) {
                if oc.produces_value() && f.ty_of(other) == f.ty_of(op0) {
                    cov.items[10] = true; // a later-defined same-typed value was skipped
                    break 'outer;
                }
            }
        }
    }
    // C5/C7/C8 have no detector arms: a fabric containing any of them
    // cannot pass verify (C5: V16, C7: V12) or cannot exist (C8: no
    // nesting in the IR). Coverage of them is structurally false.
    cov
}

/// Fitness: 0 if verify fails; else 10 per C-item + capped depth
/// bonuses (tiebreakers that keep the GA deepening, never dominating
/// an extra item).
pub fn fitness(f: &Fabric) -> f64 {
    if verify(f).is_err() {
        return 0.0;
    }
    let cov = coverage(f);
    10.0 * cov.n_items() as f64
        + (cov.phi_consumers.min(10) + cov.calls.min(5) + cov.boundary_consts.min(5)) as f64 * 0.5
}

// ----------------------------------------------------------------------
// Mutation operators — constructive IR edits, each aimed at one or
// more C-items. All take a verified-ish fabric and return a child that
// MAY be broken; selection (fitness=0 on verify failure) culls.
// ----------------------------------------------------------------------

fn insert_before_term(f: &mut Fabric, r: RegionId, cell: Cell) -> CellId {
    let len = f.region(r).map(|x| x.cells.len()).unwrap_or(1);
    f.insert_cell(r, len.saturating_sub(1), cell)
}

fn rand_type(rng: &mut Rng, allow_i1: bool) -> Type {
    match (rng.below(4), allow_i1) {
        (0, _) => Type::I32,
        (1, _) => Type::I64,
        (2, _) => Type::F64,
        (_, true) => Type::I1,
        (_, false) => Type::I64,
    }
}

fn const_of(ty: Type, rng: &mut Rng) -> ConstVal {
    match ty {
        Type::I1 => ConstVal::I1(rng.below(2) == 0),
        Type::I32 => ConstVal::I32(rng.below(100) as i32),
        Type::I64 => ConstVal::I64(rng.below(100) as i64),
        Type::F64 => ConstVal::F64(rng.below(50) as f64 / 8.0),
    }
}

/// Values defined before `pos` in region `r`, or anywhere in entry,
/// of type `ty` (V12-legal operand pool for a non-phi user at `pos`).
fn visible_values(f: &Fabric, r: RegionId, pos: usize, ty: Type) -> Vec<CellId> {
    let mut out = vec![];
    let entry = RegionId(0);
    let scan = |region: RegionId, upto: Option<usize>, out: &mut Vec<CellId>| {
        if let Some(reg) = f.region(region) {
            for (i, &id) in reg.cells.iter().enumerate() {
                if let Some(u) = upto {
                    if i >= u {
                        break;
                    }
                }
                if let Some(c) = f.cell(id) {
                    if c.produces_value() && f.ty_of(id) == Some(ty) {
                        out.push(id);
                    }
                }
            }
        }
    };
    if r == entry {
        scan(entry, Some(pos), &mut out);
    } else {
        scan(r, Some(pos), &mut out);
        scan(entry, None, &mut out);
    }
    out
}

/// M1 — add a phi to a region with predecessors (targets C2, C3, C4).
/// `ty` is chosen by the caller bias: half non-i32. Placed at spine
/// head half the time (the spec's own placement, audit C4), else just
/// before the terminator. Joins = exactly the region's predecessors
/// (V06/V14/V16-legal by construction).
pub fn mut_add_phi(f: &Fabric, rng: &mut Rng) -> Fabric {
    let mut g = f.clone();
    let candidates: Vec<RegionId> = (0..g.regions.len() as u32)
        .map(RegionId)
        .filter(|&r| !g.predecessors(r).is_empty())
        .collect();
    let r = match rng.pick(&candidates) {
        Some(&r) => r,
        None => return g,
    };
    let ty = if rng.chance(50) { rand_type(rng, false) } else { Type::I32 };
    let preds: Vec<RegionId> = g.predecessors(r).to_vec();
    let mut joins = vec![];
    let mut ops = vec![];
    for p in preds {
        // operand must be defined in p (or entry) — V07. For a
        // self-loop pred (p == r) a pre-existing value could sit after
        // the phi and close an operand cycle (V17); mint a fresh const
        // instead, which is always safe.
        if p == r {
            let len = g.region(p).map(|x| x.cells.len()).unwrap_or(1);
            let id = g.insert_cell(p, len - 1, Cell::new(p, CellKind::Const { ty, val: const_of(ty, rng) }));
            joins.push(p);
            ops.push(id);
            continue;
        }
        let pool: Vec<CellId> = g
            .region(p)
            .map(|reg| {
                reg.cells
                    .iter()
                    .copied()
                    .filter(|&id| {
                        g.cell(id).map(|c| c.produces_value()).unwrap_or(false)
                            && g.ty_of(id) == Some(ty)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let (j, o) = if pool.is_empty() {
            let id = insert_before_term(&mut g, p, Cell::new(p, CellKind::Const { ty, val: const_of(ty, rng) }));
            (p, id)
        } else {
            (p, *pool.last().expect("nonempty"))
        };
        joins.push(j);
        ops.push(o);
    }
    let mut phi = Cell::new(r, CellKind::Phi { joins });
    phi.operands = ops;
    let at_head = rng.chance(50);
    let _ = g.insert_cell(r, if at_head { 0 } else { usize::MAX }, phi);
    g
}

/// M2 — make an existing phi feed computation (targets C6): insert an
/// arith consuming the phi before the terminator, or (i1 phis) point
/// the terminator branch's condition at it.
pub fn mut_consume_phi(f: &Fabric, rng: &mut Rng) -> Fabric {
    let mut g = f.clone();
    let phis: Vec<CellId> = g
        .cells()
        .filter(|&id| matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Phi { .. })))
        .collect();
    let phi = match rng.pick(&phis) {
        Some(&p) => p,
        None => return g,
    };
    let r = g.cell(phi).expect("present").region;
    let ty = match g.ty_of(phi) {
        Some(t) => t,
        None => return g,
    };
    // variant: i1 phi → existing branch cond in the same region
    if ty == Type::I1 && rng.chance(40) {
        if let Some(&last) = g.region(r).and_then(|x| x.cells.last()) {
            if let Some(c) = g.cell_mut(last) {
                if matches!(c.kind, CellKind::Branch { .. }) {
                    c.operands = vec![phi];
                    return g;
                }
            }
        }
    }
    let term_pos = g.region(r).map(|x| x.cells.len()).unwrap_or(1) - 1;
    // mint a companion const right before the terminator, then an
    // arith consuming phi + const (both before the terminator)
    let c2 = g.insert_cell(
        r,
        term_pos,
        Cell::new(r, CellKind::Const { ty, val: const_of(ty, rng) }),
    );
    let op = match rng.below(4) {
        0 => ArithOp::Add,
        1 => ArithOp::Sub,
        2 => ArithOp::Mul,
        _ => ArithOp::Div,
    };
    let mut a = Cell::new(r, CellKind::Arith { op, ty });
    a.operands = vec![phi, c2];
    g.insert_cell(r, term_pos + 1, a);
    g
}

/// M3 — add a call cell + a consumer (targets C1).
pub fn mut_add_call(f: &Fabric, rng: &mut Rng) -> Fabric {
    let mut g = f.clone();
    if g.regions.is_empty() {
        return g;
    }
    let r = RegionId(rng.below(g.regions.len() as u64) as u32);
    let ret_ty = rand_type(rng, true);
    let nargs = rng.below(3) as usize;
    let mut call = Cell::new(r, CellKind::Call { name: format!("ga{}", rng.below(8)), ret_ty });
    // args: consts minted inline (V18-safe: consts are value cells)
    for _ in 0..nargs {
        let at = g.region(r).map(|x| x.cells.len()).unwrap_or(1) - 1;
        let ty = rand_type(rng, true);
        g.insert_cell(r, at, Cell::new(r, CellKind::Const { ty, val: const_of(ty, rng) }));
    }
    let last: Vec<CellId> = g
        .region(r)
        .map(|x| x.cells[..x.cells.len() - 1].to_vec())
        .unwrap_or_default();
    let call = {
        // re-grab positions after minting
        let at = g.region(r).map(|x| x.cells.len()).unwrap_or(1) - 1;
        for (i, id) in last.iter().enumerate().rev().take(nargs) {
            let _ = i;
            call.operands.push(*id);
        }
        call.operands.truncate(nargs);
        g.insert_cell(r, at, call)
    };
    // consumer, so the call feeds computation too
    let c2 = g.insert_cell(
        r,
        g.region(r).map(|x| x.cells.len()).unwrap_or(1) - 1,
        Cell::new(r, CellKind::Const { ty: ret_ty, val: const_of(ret_ty, rng) }),
    );
    let mut a = Cell::new(r, CellKind::Arith { op: ArithOp::Mul, ty: ret_ty });
    a.operands = vec![call, c2];
    insert_before_term(&mut g, r, a);
    g
}

/// M4 — add a boundary constant and feed it into an arith (targets
/// C9: the corner the corpus generator's numeric domain cannot reach).
pub fn mut_boundary_const(f: &Fabric, rng: &mut Rng) -> Fabric {
    let mut g = f.clone();
    if g.regions.is_empty() {
        return g;
    }
    let r = RegionId(rng.below(g.regions.len() as u64) as u32);
    let pick = rng.below(10);
    let (ty, val) = match pick {
        0 => (Type::I32, ConstVal::I32(i32::MIN)),
        1 => (Type::I32, ConstVal::I32(i32::MAX)),
        2 => (Type::I64, ConstVal::I64(i64::MIN)),
        3 => (Type::I64, ConstVal::I64(i64::MAX)),
        4 => (Type::I64, ConstVal::I64(-1)),
        5 => (Type::F64, ConstVal::F64(f64::INFINITY)),
        6 => (Type::F64, ConstVal::F64(f64::NEG_INFINITY)),
        7 => (Type::F64, ConstVal::F64(1e308)),
        8 => (Type::F64, ConstVal::F64(-273.0)), // out-of-domain eighth
        _ => (Type::F64, ConstVal::F64(0.1)),    // non-eighth
    };
    let v1 = insert_before_term(&mut g, r, Cell::new(r, CellKind::Const { ty, val }));
    let v2 = insert_before_term(&mut g, r, Cell::new(r, CellKind::Const { ty, val: const_of(ty, rng) }));
    let mut a = Cell::new(r, CellKind::Arith { op: ArithOp::Add, ty });
    a.operands = vec![v1, v2];
    insert_before_term(&mut g, r, a);
    g
}

/// M5 — grow past the corpus size caps (targets C10).
pub fn mut_grow(f: &Fabric, rng: &mut Rng) -> Fabric {
    let mut g = f.clone();
    if g.regions.is_empty() {
        return g;
    }
    match rng.below(3) {
        0 => {
            // new region jumping back somewhere (adds region + edge)
            let target = RegionId(rng.below(g.regions.len() as u64) as u32);
            let r = g.add_region(format!("ga{}", g.regions.len()));
            let v = g.add_cell(r, Cell::new(r, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
            g.add_cell(r, Cell::new(r, CellKind::Jump { target }));
            let _ = v;
        }
        1 => {
            // append a small arith chain before some region's terminator
            let r = RegionId(rng.below(g.regions.len() as u64) as u32);
            let term_pos = g.region(r).map(|x| x.cells.len()).unwrap_or(1) - 1;
            let mut prev = g.insert_cell(
                r,
                term_pos,
                Cell::new(r, CellKind::Const { ty: Type::I32, val: ConstVal::I32(3) }),
            );
            let n = 3 + rng.below(8) as usize;
            for _ in 0..n {
                let c = g.insert_cell(
                    r,
                    g.region(r).map(|x| x.cells.len()).unwrap_or(1) - 1,
                    Cell::new(r, CellKind::Const { ty: Type::I32, val: ConstVal::I32(2) }),
                );
                let mut a = Cell::new(r, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
                a.operands = vec![prev, c];
                prev = g.insert_cell(
                    r,
                    g.region(r).map(|x| x.cells.len()).unwrap_or(1) - 1,
                    a,
                );
            }
        }
        _ => {
            // jmp terminator → branch on a fresh const (adds an edge)
            let r = RegionId(rng.below(g.regions.len() as u64) as u32);
            let last = match g.region(r).and_then(|x| x.cells.last().copied()) {
                Some(l) => l,
                None => return g,
            };
            let target = match g.cell(last).map(|c| &c.kind) {
                Some(CellKind::Jump { target }) => *target,
                _ => return g,
            };
            let cond = insert_before_term(
                &mut g,
                r,
                Cell::new(r, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }),
            );
            if g.region_mut(r).is_some() {
                // replace the jump with a branch (same target twice is legal)
                g.slab[last.0 as usize] = None;
                if let Some(reg) = g.region_mut(r) {
                    reg.cells.pop();
                }
                let mut br = Cell::new(r, CellKind::Branch { then_r: target, else_r: target });
                br.operands = vec![cond];
                g.add_cell(r, br);
            }
        }
    }
    g
}

/// M6 — retarget an arith's slot-0 operand to a visible value that is
/// NOT the most recent one (targets C11: chain-shaped operand choice).
pub fn mut_operand_shuffle(f: &Fabric, rng: &mut Rng) -> Fabric {
    let mut g = f.clone();
    let ariths: Vec<CellId> = g
        .cells()
        .filter(|&id| matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Arith { .. })))
        .collect();
    let a = match rng.pick(&ariths) {
        Some(&a) => a,
        None => return g,
    };
    let ty = match g.cell(a).and_then(|c| match &c.kind {
        CellKind::Arith { ty, .. } => Some(*ty),
        _ => None,
    }) {
        Some(t) => t,
        None => return g,
    };
    let (r, pos) = match g.cell(a).map(|c| c.region).and_then(|r| {
        g.index_in_region(a).map(|p| (r, p))
    }) {
        Some(x) => x,
        None => return g,
    };
    let pool = visible_values(&g, r, pos, ty);
    // prefer a non-latest pick when one exists
    if pool.len() >= 2 {
        let i = rng.below((pool.len() - 1) as u64) as usize; // excludes the last
        if let Some(c) = g.cell_mut(a) {
            if let Some(slot0) = c.operands.first_mut() {
                *slot0 = pool[i];
            }
        }
    }
    g
}

/// Apply one mutation chosen from the menu (weighted: phi-adding and
/// phi-consuming lead — C4+C6 are the audit's sharpest blind spots).
pub fn mutate_breed(f: &Fabric, rng: &mut Rng) -> Fabric {
    match rng.below(100) {
        0..=24 => mut_add_phi(f, rng),
        25..=49 => mut_consume_phi(f, rng),
        50..=64 => mut_add_call(f, rng),
        65..=79 => mut_boundary_const(f, rng),
        80..=89 => mut_grow(f, rng),
        _ => mut_operand_shuffle(f, rng),
    }
}

// ----------------------------------------------------------------------
// Crossover — region grafting (the fabric-IR analogue of evolve.py's
// single-point crossover). Graft a contiguous run of B's non-entry
// regions onto a clone of A: operand ids remapped inside the graft,
// entry values substituted with same-typed child-entry values (or
// fresh consts), phis with joins outside the graft dropped, terminator
// targets outside the graft clamped to the child's entry. Broken
// children simply score 0 and die — selection is the repair pass.
// ----------------------------------------------------------------------

pub fn crossover(a: &Fabric, b: &Fabric, rng: &mut Rng) -> Fabric {
    let mut child = a.clone();
    let n = b.regions.len();
    if n < 2 {
        return child;
    }
    let start = 1 + rng.below((n - 1) as u64) as usize;
    let count = 1 + rng.below((n - start) as u64) as usize;
    let graft: Vec<usize> = (start..(start + count).min(n)).collect();

    // map b-region-idx -> child RegionId
    let mut region_map: BTreeMap<u32, RegionId> = BTreeMap::new();
    for &gi in &graft {
        let rid = child.add_region(b.regions[gi].name.clone());
        region_map.insert(gi as u32, rid);
    }

    // decide which phis survive: all joins must map inside the graft
    let phi_kept = |joins: &[RegionId]| -> bool {
        joins.iter().all(|j| region_map.contains_key(&j.0))
    };

    // value cells in b's entry, by type, for operand substitution
    // substitute: b-entry value -> child-entry value (first match) or a
    // fresh const in the child's entry (before its terminator)
    fn substitute(child: &mut Fabric, b: &Fabric, ty: Type, rng: &mut Rng) -> CellId {
        let pool: Vec<CellId> = b
            .regions
            .first()
            .map(|r| {
                r.cells
                    .iter()
                    .copied()
                    .filter(|&id| {
                        b.cell(id).map(|c| c.produces_value()).unwrap_or(false)
                            && b.ty_of(id) == Some(ty)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !pool.is_empty() {
            if let Some(reg) = child.region(RegionId(0)) {
                for &id in &reg.cells {
                    if let Some(c) = child.cell(id) {
                        if c.produces_value() && child.ty_of(id) == Some(ty) {
                            return id;
                        }
                    }
                }
            }
        }
        insert_before_term(
            child,
            RegionId(0),
            Cell::new(RegionId(0), CellKind::Const { ty, val: const_of(ty, rng) }),
        )
    }

    for &gi in &graft {
        let child_r = *region_map.get(&(gi as u32)).expect("mapped above");
        let mut id_map: BTreeMap<u32, CellId> = BTreeMap::new();
        // pass 1: record which cells will exist (drop rejected phis)
        let mut kept_cells: Vec<CellId> = vec![];
        for &bid in &b.regions[gi].cells {
            match b.cell(bid).map(|c| &c.kind) {
                Some(CellKind::Phi { joins }) if !phi_kept(joins) => {}
                _ => kept_cells.push(bid),
            }
        }
        // pass 2: place cells in order
        let mut pending: Vec<(CellId, Cell)> = vec![];
        for &bid in &kept_cells {
            let bc = match b.cell(bid) {
                Some(c) => c.clone(),
                None => continue,
            };
            let kind = match &bc.kind {
                CellKind::Param { ty } => CellKind::Const { ty: *ty, val: const_of(*ty, rng) },
                CellKind::Phi { joins } => {
                    CellKind::Phi { joins: joins.clone() }
                }
                k => k.clone(),
            };
            let mut cell = Cell::new(child_r, kind);
            cell.operands = bc.operands.clone();
            pending.push((bid, cell));
        }
        // resolve operands in two sweeps: local graft refs first (they
        // may be forward), then non-local
        for (bid, cell) in pending.iter_mut() {
            let bc = match b.cell(*bid) {
                Some(c) => c.clone(),
                None => continue,
            };
            let mut resolved = vec![];
            for &op in &bc.operands {
                if let Some(local) = id_map.get(&op.0) {
                    resolved.push(*local);
                } else if b.cell(op).is_some() {
                    // comes from outside the graft: b-entry value (or
                    // some other region) — substitute by type
                    let ty = match b.ty_of(op) {
                        Some(t) => t,
                        None => Type::I32,
                    };
                    resolved.push(substitute(&mut child, b, ty, rng));
                } else {
                    resolved.push(op);
                }
            }
            cell.operands = resolved;
        }
        // place: phis first (spine head — audit C4 placement), then the
        // rest in b order, terminator last
        let mut body: Vec<(CellId, Cell)> = pending;
        body.sort_by_key(|(_, c)| !matches!(c.kind, CellKind::Phi { .. }));
        for (bid, mut cell) in body {
            let is_term = cell.is_terminator();
            if is_term {
                // remap terminator targets into child space
                match &mut cell.kind {
                    CellKind::Jump { target } => {
                        *target = region_map.get(&target.0).copied().unwrap_or(RegionId(0));
                    }
                    CellKind::Branch { then_r, else_r } => {
                        *then_r = region_map.get(&then_r.0).copied().unwrap_or(RegionId(0));
                        *else_r = region_map.get(&else_r.0).copied().unwrap_or(RegionId(0));
                    }
                    _ => {}
                }
            }
            let placed = child.add_cell(child_r, cell);
            id_map.insert(bid.0, placed);
        }
        // fix phi joins to child region ids
        for (_bid, placed) in id_map.iter() {
            if let Some(c) = child.cell_mut(*placed) {
                if let CellKind::Phi { joins } = &mut c.kind {
                    for j in joins.iter_mut() {
                        *j = region_map.get(&j.0).copied().unwrap_or(*j);
                    }
                }
            }
        }
    }
    child
}

// ----------------------------------------------------------------------
// The engine (structure mirrors mud-arena evolve.py: initialize,
// evaluate, tournament-select, breed+mutate, replace-worst, history)
// ----------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct GaConfig {
    pub population: usize,
    pub generations: usize,
    pub elite: usize,
    pub tournament: usize,
    pub seed: u64,
}

impl Default for GaConfig {
    fn default() -> Self {
        GaConfig { population: 200, generations: 50, elite: 20, tournament: 5, seed: 0x6A1C0 }
    }
}

/// Per-generation statistics, the measured exit of the spike.
#[derive(Clone, Debug, Default)]
pub struct GenStats {
    pub gen: usize,
    pub avg_fitness: f64,
    pub best_fitness: f64,
    pub verify_pass: usize, // of population
    /// per C-item: how many fabrics in the population exercise it
    pub item_counts: [usize; 11],
}

#[derive(Clone, Debug)]
pub struct RunReport {
    pub gens: Vec<GenStats>,
    /// generation (0-based) in which each item was FIRST covered by
    /// any verifying fabric; usize::MAX = never
    pub first_covered: [usize; 11],
    /// max population count per item over the whole run
    pub max_item_counts: [usize; 11],
    /// coverage of the best fabric at end of run
    pub best_coverage: Coverage,
    pub best_fitness: f64,
    pub total_evals: usize,
}

pub fn run(cfg: &GaConfig) -> RunReport {
    let mut rng = Rng::new(cfg.seed);
    // initialize: seed population from the corpus generator itself —
    // the GA starts exactly where the audit's blind spot begins
    let mut population: Vec<Fabric> = (0..cfg.population)
        .map(|i| crate::fuzz::gen_fabric(&mut Rng::new(cfg.seed.wrapping_add((i as u64).max(1)).max(1))))
        .collect();
    let mut gens = vec![];
    let mut first_covered = [usize::MAX; 11];
    let mut max_item_counts = [0usize; 11];
    let mut total_evals = 0usize;
    let mut best_fabric = population[0].clone();
    let mut best_fitness = -1.0;

    for gen in 0..cfg.generations {
        // evaluate
        let fits: Vec<f64> = population.iter().map(|f| fitness(f)).collect();
        total_evals += population.len();
        let mut stats = GenStats {
            gen,
            avg_fitness: fits.iter().sum::<f64>() / population.len().max(1) as f64,
            best_fitness: fits.iter().cloned().fold(f64::MIN, f64::max),
            ..Default::default()
        };
        for (f, fit) in population.iter().zip(fits.iter()) {
            if *fit > 0.0 {
                stats.verify_pass += 1;
            }
            if *fit > best_fitness {
                best_fitness = *fit;
                best_fabric = f.clone();
            }
        }
        for f in &population {
            if verify(f).is_ok() {
                let cov = coverage(f);
                for i in 0..11 {
                    if cov.items[i] {
                        stats.item_counts[i] += 1;
                        if first_covered[i] == usize::MAX {
                            first_covered[i] = gen;
                        }
                    }
                }
            }
        }
        for i in 0..11 {
            max_item_counts[i] = max_item_counts[i].max(stats.item_counts[i]);
        }
        gens.push(stats);

        // select (tournament) + replace worst with bred children
        let mut next: Vec<Fabric> = vec![];
        // elites: top N by fitness, carried unchanged
        let mut order: Vec<usize> = (0..population.len()).collect();
        order.sort_by(|&x, &y| fits[y].partial_cmp(&fits[x]).unwrap_or(std::cmp::Ordering::Equal));
        for &i in order.iter().take(cfg.elite) {
            next.push(population[i].clone());
        }
        while next.len() < cfg.population {
            let tourney = (0..cfg.tournament)
                .map(|_| rng.below(population.len() as u64) as usize)
                .collect::<Vec<_>>();
            let pa = *tourney.iter().max_by_key(|&&i| fits[i].to_bits()).expect("nonempty");
            let pb = *tourney.iter().max_by_key(|&&i| fits[i].to_bits()).expect("nonempty");
            let child = if rng.chance(60) && pa != pb {
                crossover(&population[pa], &population[pb], &mut rng)
            } else {
                population[pa].clone()
            };
            let n_mut = 1 + rng.below(3);
            let mut child = child;
            for _ in 0..n_mut {
                child = mutate_breed(&child, &mut rng);
            }
            next.push(child);
        }
        population = next;
    }

    let best_coverage = {
        let fits: Vec<f64> = population.iter().map(fitness).collect();
        let bi = fits
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);
        let _ = best_fabric;
        coverage(&population[bi])
    };

    RunReport { gens, first_covered, max_item_counts, best_coverage, best_fitness, total_evals }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diamond(entry_val: i32) -> Fabric {
        let text = format!(
            "fabric v0\n\
             region entry\n\
               %0 = const i1 true\n\
               %1 = br %0, t, el\n\
             region t\n\
               %2 = const i32 {v}\n\
               %3 = jump j\n\
             region el\n\
               %4 = const i32 2\n\
               %5 = jump j\n\
             region j\n\
               %6 = phi [t: %2] [el: %4]\n\
               %7 = ret %6\n",
            v = entry_val
        );
        crate::text::parse(&text).expect("diamond parses")
    }

    #[test]
    fn detectors_fire_on_hand_built_shapes() {
        // baseline: audit-shaped diamond — phi present but only feeding ret
        let f = diamond(1);
        assert!(verify(&f).is_ok());
        let cov = coverage(&f);
        assert!(!cov.items[0], "no calls");
        assert!(!cov.items[1], "i32 phi");
        assert!(cov.items[3], "phi IS at spine head here — verify accepts it (audit C4: the rule is enforced nowhere); the corpus never emits it only because its generator always adds body consts first");
        assert!(!cov.items[5], "phi feeds ret only");

        // head-phi + phi→arith: the audit's C4+C6 pair, hand-built
        let text = "fabric v0\n\
                    region entry\n\
                      %0 = const i1 true\n\
                      %1 = br %0, t, el\n\
                    region t\n\
                      %2 = const i64 5i64\n\
                      %3 = jump j\n\
                    region el\n\
                      %4 = const i64 7i64\n\
                      %5 = jump j\n\
                    region j\n\
                      %6 = phi [t: %2] [el: %4]\n\
                      %7 = const i64 1i64\n\
                      %8 = arith.add i64 %6, %7\n\
                      %9 = ret %8\n";
        let f = crate::text::parse(text).expect("head-phi diamond parses");
        assert!(verify(&f).is_ok(), "head-phi + phi→arith must VERIFY green");
        let cov = coverage(&f);
        assert!(cov.items[1], "C2: i64 phi");
        assert!(cov.items[3], "C4: phi at spine head");
        assert!(cov.items[5], "C6: phi feeds arith");
    }

    #[test]
    fn boundary_detector_and_unreachable_laws() {
        let mut f = diamond(1);
        // C9: negative i32 const is boundary-domain
        let pos = f.regions.last().unwrap().cells.len() - 1;
        let reg = crate::id::RegionId(3);
        f.insert_cell(
            reg,
            pos,
            Cell::new(reg, CellKind::Const { ty: Type::I32, val: ConstVal::I32(i32::MIN) }),
        );
        assert!(verify(&f).is_ok());
        assert!(coverage(&f).items[8], "C9: i32::MIN is boundary-domain");

        // C7: param outside entry → V12 rejects (unreachable by law)
        let mut g = diamond(1);
        let b = g.add_region("x");
        g.add_cell(b, Cell::new(b, CellKind::Param { ty: Type::I32 }));
        g.add_cell(b, Cell::new(b, CellKind::Ret));
        assert_eq!(verify(&g).expect_err("param outside entry must fail").code, "V12");

        // C5: partial phi → V16 rejects (unreachable by law)
        let text = "fabric v0\n\
                    region entry\n\
                      %0 = const i1 true\n\
                      %1 = br %0, t, el\n\
                    region t\n\
                      %2 = const i32 1\n\
                      %3 = jump j\n\
                    region el\n\
                      %4 = jump j\n\
                    region j\n\
                      %5 = phi [t: %2]\n\
                      %6 = ret %5\n";
        let h = crate::text::parse(text).expect("partial-phi diamond parses");
        assert_eq!(verify(&h).expect_err("partial phi must fail").code, "V16");
    }

    #[test]
    fn ga_run_reaches_reachable_items() {
        // small-budget run (debug-mode test): every reachable C-item
        // must be covered by some verifying fabric before the budget
        // runs out. This is the spike's positive claim, at test scale.
        let cfg = GaConfig {
            population: 60,
            generations: 30,
            elite: 8,
            tournament: 4,
            seed: 0x6A1C0,
        };
        let rep = run(&cfg);
        let reachable = [0usize, 1, 2, 3, 5, 8, 9, 10]; // C1,C2,C3,C4,C6,C9,C10,C11
        for i in reachable {
            assert_ne!(
                rep.first_covered[i],
                usize::MAX,
                "GA never covered {} in {} gens",
                C_ITEMS[i],
                cfg.generations
            );
        }
        // unreachable items must never fire — ever, in any run
        for i in [4usize, 6, 7] {
            assert_eq!(rep.first_covered[i], usize::MAX, "{} is unreachable", C_ITEMS[i]);
            assert_eq!(rep.max_item_counts[i], 0);
        }
    }

    #[test]
    fn fitness_zero_when_broken() {
        let mut f = diamond(1);
        f.slab[0] = None; // punch a hole
        assert_eq!(fitness(&f), 0.0);
        assert!(fitness(&diamond(1)) >= 0.0);
    }
}

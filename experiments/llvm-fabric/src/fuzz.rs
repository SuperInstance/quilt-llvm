//! Deterministic fuzzing for the fabric: seeded xorshift PRNG (no deps),
//! structured generator for valid fabrics, mutations for invalid ones.
//!
//! The invariant under test: verify() either accepts or rejects with a
//! precise reason — NEVER panics. Valid fabrics must also round-trip
//! through the textual format bit-for-bit (at the string level).

use crate::cell::{ArithOp, Cell, CellKind, CmpOp};
use crate::fabric::Fabric;
use crate::id::{CellId, RegionId};
use crate::ty::{ConstVal, Type};
use crate::verify::verify;
use std::collections::BTreeMap;

/// xorshift64* — deterministic, fast, good enough for a spike fuzzer.
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }

    pub fn pick<'a, T>(&mut self, xs: &'a [T]) -> Option<&'a T> {
        if xs.is_empty() {
            None
        } else {
            Some(&xs[self.below(xs.len() as u64) as usize])
        }
    }

    pub fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

fn rand_const(rng: &mut Rng) -> (Type, ConstVal) {
    match rng.below(4) {
        0 => (Type::I1, ConstVal::I1(rng.below(2) == 0)),
        1 => (Type::I32, ConstVal::I32(rng.below(1000) as i32 - 500)),
        2 => (Type::I64, ConstVal::I64(rng.below(1_000_000) as i64)),
        _ => (Type::F64, ConstVal::F64(rng.below(200) as f64 / 8.0 - 12.5)),
    }
}

/// Generate a structured fabric that is valid by construction.
/// Any generator bug surfaces as a corpus_run error naming the seed.
pub fn gen_fabric(rng: &mut Rng) -> Fabric {
    let mut f = Fabric::empty();
    let n_regions = 1 + rng.below(6) as usize; // 1..=6
    let region_ids: Vec<RegionId> =
        (0..n_regions).map(|i| f.add_region(format!("r{}", i))).collect();
    let entry = region_ids[0];

    // 1. Region graph: choose terminators for every region up front.
    //    Bodies are built after, phis last (they need preds + values).
    enum Term {
        Br(RegionId, RegionId),
        Jmp(RegionId),
        Ret(),
    }
    let mut terms: Vec<Term> = vec![];
    for (i, &r) in region_ids.iter().enumerate() {
        let _ = r;
        let term = if n_regions == 1 || (i > 0 && rng.chance(20)) {
            Term::Ret()
        } else if rng.chance(45) {
            let mut a = region_ids[rng.below(n_regions as u64) as usize];
            let mut b = region_ids[rng.below(n_regions as u64) as usize];
            // prefer distinct targets for interesting CFGs
            if a == b && n_regions > 1 {
                b = region_ids[(rng.below(n_regions as u64 - 1) + 1) as usize];
            }
            if rng.chance(50) {
                std::mem::swap(&mut a, &mut b);
            }
            Term::Br(a, b)
        } else {
            Term::Jmp(region_ids[rng.below(n_regions as u64) as usize])
        };
        terms.push(term);
    }

    // 2. Bodies: consts first (every region gets at least one), then
    //    arith/cmp using visible values. Entry also gets params.
    // value cells by region, by type — filled as we go.

    // Visible-value bookkeeping for scope-correct generation.
    #[derive(Default)]
    struct Seen {
        by_region: Vec<Vec<CellId>>, // value cells per region (index = region idx)
    }
    impl Seen {
        fn visible(&self, f: &Fabric, region: RegionId, entry: RegionId, ty: Type) -> Vec<CellId> {
            let mut out = vec![];
            for (ri, ids) in self.by_region.iter().enumerate() {
                let rid = RegionId(ri as u32);
                if rid == region || rid == entry {
                    for &id in ids {
                        if f.ty_of(id) == Some(ty) {
                            out.push(id);
                        }
                    }
                }
            }
            out
        }
        fn record(&mut self, f: &Fabric, id: CellId) {
            let c = match f.cell(id) {
                Some(c) if c.produces_value() => c,
                _ => return,
            };
            while self.by_region.len() <= c.region.0 as usize {
                self.by_region.push(vec![]);
            }
            self.by_region[c.region.0 as usize].push(id);
        }
    }
    let mut seen = Seen::default();

    // params in entry
    let n_params = rng.below(4);
    for _ in 0..n_params {
        let (ty, _) = rand_const(rng); // just to vary the type
        let ty = if rng.chance(30) { ty } else { Type::I32 };
        let id = f.add_cell(entry, Cell::new(entry, CellKind::Param { ty }));
        seen.record(&f, id);
    }

    // consts + arith bodies per region (before terminators, so terminators
    // stay last: we build bodies for ALL regions first, then terminators).
    for (ri, &r) in region_ids.iter().enumerate() {
        let n_const = 1 + rng.below(3);
        for _ in 0..n_const {
            let (ty, val) = rand_const(rng);
            let id = f.add_cell(r, Cell::new(r, CellKind::Const { ty, val }));
            seen.record(&f, id);
        }
        let n_ops = rng.below(6);
        for _ in 0..n_ops {
            let ty = match rng.below(4) {
                0 => Type::I32,
                1 => Type::I64,
                2 => Type::F64,
                _ => Type::I1,
            };
            let visible = seen.visible(&f, r, entry, ty);
            if rng.chance(35) {
                // cmp: operands of one type, result i1
                let vis_t = if visible.len() >= 2 {
                    ty
                } else {
                    Type::I32
                };
                let pool = seen.visible(&f, r, entry, vis_t);
                if pool.len() < 2 {
                    let id = f.add_cell(
                        r,
                        Cell::new(r, CellKind::Const { ty: vis_t, val: ConstVal::I32(7) }),
                    );
                    seen.record(&f, id);
                }
                let pool = seen.visible(&f, r, entry, vis_t);
                if pool.len() >= 2 {
                    let a = pool[pool.len() - 1];
                    let b = pool[pool.len() - 2];
                    let op = match rng.below(6) {
                        0 => CmpOp::Eq,
                        1 => CmpOp::Ne,
                        2 => CmpOp::Lt,
                        3 => CmpOp::Le,
                        4 => CmpOp::Gt,
                        _ => CmpOp::Ge,
                    };
                    let mut c = Cell::new(r, CellKind::Cmp { op });
                    c.operands = vec![a, b];
                    let id = f.add_cell(r, c);
                    seen.record(&f, id);
                }
                continue;
            }
            // arith: need two visible values of ty; mint consts if short
            while seen.visible(&f, r, entry, ty).len() < 2 {
                let (_, _) = rand_const(rng);
                let val = match ty {
                    Type::I1 => ConstVal::I1(rng.below(2) == 0),
                    Type::I32 => ConstVal::I32(rng.below(100) as i32),
                    Type::I64 => ConstVal::I64(rng.below(100) as i64),
                    Type::F64 => ConstVal::F64(rng.below(50) as f64),
                };
                let id = f.add_cell(r, Cell::new(r, CellKind::Const { ty, val }));
                seen.record(&f, id);
            }
            let pool = seen.visible(&f, r, entry, ty);
            let a = pool[pool.len() - 1];
            let b = if pool.len() >= 2 && rng.chance(70) {
                pool[pool.len() - 2]
            } else {
                pool[pool.len() - 1] // self-reference a,a is legal arith
            };
            let op = match rng.below(4) {
                0 => ArithOp::Add,
                1 => ArithOp::Sub,
                2 => ArithOp::Mul,
                _ => ArithOp::Div,
            };
            let mut c = Cell::new(r, CellKind::Arith { op, ty });
            c.operands = vec![a, b];
            let id = f.add_cell(r, c);
            seen.record(&f, id);
        }
        let _ = ri;
    }

    // 3. Phis: insert before the (not yet added) terminator position,
    //    i.e. at end of the body built so far. Joins = actual preds.
    for &r in &region_ids {
        let preds = f.predecessors(r);
        if preds.is_empty() || !rng.chance(60) {
            continue;
        }
        // pick a type for the phi: any type with a visible value in >=1 pred
        let mut joins = vec![];
        let mut ops = vec![];
        let ty = Type::I32; // keep it simple: phis are i32 in the generator
        for p in &preds {
            let pool = seen.visible(&f, *p, entry, ty);
            if pool.is_empty() {
                let id = f.add_cell(
                    *p,
                    Cell::new(*p, CellKind::Const { ty, val: ConstVal::I32(rng.below(9) as i32) }),
                );
                seen.record(&f, id);
                joins.push(*p);
                ops.push(id);
            } else {
                joins.push(*p);
                ops.push(pool[pool.len() - 1]);
            }
        }
        let mut c = Cell::new(r, CellKind::Phi { joins });
        c.operands = ops;
        let id = f.add_cell(r, c);
        seen.record(&f, id);
    }

    // 4. Terminators last. Branch conditions: an i1 value visible in the
    //    region (mint a const if none).
    for (i, &r) in region_ids.iter().enumerate() {
        match &terms[i] {
            Term::Ret() => {
                let pool: Vec<CellId> = {
                    let region_cells = f.region(r).map(|x| x.cells.clone()).unwrap_or_default();
                    region_cells
                        .into_iter()
                        .filter(|&id| f.ty_of(id).is_some())
                        .collect()
                };
                let mut c = Cell::new(r, CellKind::Ret);
                if let Some(&v) = pool.last() {
                    if rng.chance(70) {
                        c.operands = vec![v];
                    }
                }
                f.add_cell(r, c);
            }
            Term::Jmp(t) => {
                f.add_cell(r, Cell::new(r, CellKind::Jump { target: *t }));
            }
            Term::Br(a, b) => {
                let pool = seen.visible(&f, r, entry, Type::I1);
                let cond = if let Some(&c0) = pool.last() {
                    c0
                } else {
                    let id = f.add_cell(
                        r,
                        Cell::new(r, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }),
                    );
                    seen.record(&f, id);
                    id
                };
                let mut c = Cell::new(r, CellKind::Branch { then_r: *a, else_r: *b });
                c.operands = vec![cond];
                f.add_cell(r, c);
            }
        }
    }

    f
}

/// One corruption mutation. May (rarely) leave the fabric still valid —
/// the corpus honestly counts those as valid.
pub fn mutate(f: &Fabric, rng: &mut Rng) -> Fabric {
    let mut g = f.clone();
    let present: Vec<CellId> = g.cells().collect();
    if present.is_empty() || g.regions.is_empty() {
        return g;
    }
    let victim = *rng.pick(&present).expect("nonempty");
    match rng.below(9) {
        0 => {
            // operand points out of bounds
            let bad = CellId(g.slab.len() as u32 + 7);
            if let Some(c) = g.cell_mut(victim) {
                if let Some(slot) = c.operands.first_mut() {
                    *slot = bad;
                }
            }
        }
        1 => {
            // retarget an operand to a random present cell (may break scope/type)
            let other = *rng.pick(&present).expect("nonempty");
            if let Some(c) = g.cell_mut(victim) {
                if !c.operands.is_empty() {
                    let slot = rng.below(c.operands.len() as u64) as usize;
                    c.operands[slot] = other;
                }
            }
        }
        2 => {
            // punch a hole in the slab
            g.slab[victim.0 as usize] = None;
        }
        3 => {
            // swap a const's declared type
            if let Some(c) = g.cell_mut(victim) {
                if let CellKind::Const { ty, .. } = &mut c.kind {
                    *ty = match *ty {
                        Type::I1 => Type::I32,
                        Type::I32 => Type::I64,
                        Type::I64 => Type::F64,
                        Type::F64 => Type::I1,
                    };
                }
            }
        }
        4 => {
            // drop a region's terminator
            let r = RegionId(rng.below(g.regions.len() as u64) as u32);
            let last_is_term = g
                .region(r)
                .and_then(|region| region.cells.last().copied())
                .and_then(|last| g.cell(last))
                .map(|c| c.is_terminator())
                .unwrap_or(false);
            if last_is_term {
                if let Some(region) = g.region_mut(r) {
                    region.cells.pop();
                }
            }
        }
        5 => {
            // duplicate a terminator mid-region
            let terms: Vec<CellId> = present
                .iter()
                .copied()
                .filter(|&id| g.cell(id).map(|c| c.is_terminator()).unwrap_or(false))
                .collect();
            if let Some(&t) = rng.pick(&terms) {
                if let Some(tc) = g.cell(t).cloned() {
                    let region = tc.region;
                    let len = g.region(region).map(|r| r.cells.len()).unwrap_or(1);
                    g.insert_cell(region, len.saturating_sub(1), tc);
                }
            }
        }
        6 => {
            // retype an arith
            if let Some(c) = g.cell_mut(victim) {
                if let CellKind::Arith { ty, .. } = &mut c.kind {
                    *ty = match *ty {
                        Type::I1 => Type::I32,
                        Type::I32 => Type::I64,
                        Type::I64 => Type::F64,
                        Type::F64 => Type::I1,
                    };
                }
            }
        }
        7 => {
            // point a phi join at a random region
            let n = g.regions.len() as u64 + 3;
            if let Some(c) = g.cell_mut(victim) {
                if let CellKind::Phi { joins } = &mut c.kind {
                    if let Some(j) = joins.first_mut() {
                        *j = RegionId(rng.below(n) as u32);
                    }
                }
            }
        }
        _ => {
            // make a branch condition i32
            let pool: Vec<CellId> = present
                .iter()
                .copied()
                .filter(|&id| g.ty_of(id) == Some(Type::I32))
                .collect();
            let v = rng.pick(&pool).copied();
            if let Some(c) = g.cell_mut(victim) {
                if matches!(&c.kind, CellKind::Branch { .. }) {
                    if let Some(v) = v {
                        c.operands = vec![v];
                    }
                }
            }
        }
    }
    g
}

#[derive(Debug, Default)]
pub struct CorpusStats {
    pub iters: u64,
    pub valid: u64,
    pub cells_walked: u64, // provenance walks performed (one per cell)
    pub mutated: u64,
    pub mutated_still_valid: u64,
    pub rejected: BTreeMap<String, u64>,
    pub roundtrip_fail: u64,
    pub prov_fail: u64,
    pub replay_fail: u64,
    pub panics: u64, // can only be observed as a test/bin crash; kept for the report
}

impl CorpusStats {
    pub fn rejected_total(&self) -> u64 {
        self.rejected.values().sum()
    }
}

/// Run the corpus. Any invariant failure returns Err naming the seed.
pub fn corpus_run(iters: u64, seed0: u64) -> Result<CorpusStats, String> {
    let mut st = CorpusStats { iters, ..Default::default() };
    for i in 0..iters {
        let seed = seed0.wrapping_add(i).max(1);
        let mut rng = Rng::new(seed);
        let f = gen_fabric(&mut rng);
        // generator contract: built fabrics must verify
        if let Err(e) = verify(&f) {
            return Err(format!("seed {}: generator produced invalid fabric: {}", seed, e));
        }
        st.valid += 1;
        st.cells_walked += f.cells().count() as u64;

        // valid fabrics must round-trip through text
        let once = crate::text::print(&f);
        let twice = match crate::text::parse(&once) {
            Ok(f2) => crate::text::print(&f2),
            Err(e) => return Err(format!("seed {}: valid fabric failed to reparse: {}", seed, e)),
        };
        if once != twice {
            st.roundtrip_fail += 1;
            return Err(format!("seed {}: print/parse/print not stable", seed));
        }

        // provenance walk from EVERY cell must terminate at roots
        for id in f.cells() {
            if let Err(e) = crate::prov::check_prov(&f, id) {
                st.prov_fail += 1;
                return Err(format!("seed {}: provenance of {} failed: {}", seed, id, e));
            }
        }

        // pipeline + replay: history must reproduce every intermediate
        // fabric bit-identically (structural + canonical text)
        match crate::pipeline::run(&f) {
            Ok((final_f, history, stages)) => {
                match crate::replay::replay(&f, &history) {
                    Ok((replayed, final_r)) => {
                        if replayed.len() != stages.len()
                            || stages.iter().zip(replayed.iter()).any(|(a, b)| a != b)
                            || final_f != final_r
                        {
                            st.replay_fail += 1;
                            return Err(format!("seed {}: replay diverged from pipeline", seed));
                        }
                    }
                    Err(e) => {
                        st.replay_fail += 1;
                        return Err(format!("seed {}: replay failed: {}", seed, e));
                    }
                }
                // conservation across the whole pipeline
                if let Err(e) = crate::conserve::check_pipeline(&f, &final_f, &history) {
                    return Err(format!("seed {}: pipeline conservation: {}", seed, e));
                }
            }
            Err(e) => return Err(format!("seed {}: pipeline failed on valid fabric: {}", seed, e)),
        }

        // mutations: verify must reject or accept WITH a reason-free panic never occurring
        if rng.chance(45) {
            let n = 1 + rng.below(3);
            let mut g = f.clone();
            for _ in 0..n {
                g = mutate(&g, &mut rng);
            }
            st.mutated += 1;
            match verify(&g) {
                Ok(()) => st.mutated_still_valid += 1,
                Err(e) => {
                    if e.detail.is_empty() {
                        return Err(format!("seed {}: rejection without detail", seed));
                    }
                    *st.rejected.entry(e.code.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    Ok(st)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_400_iters_no_panic_all_invariants() {
        let st = corpus_run(400, 0xFAB1C).expect("corpus invariants");
        assert_eq!(st.valid, 400);
        assert_eq!(st.roundtrip_fail, 0);
        assert_eq!(st.prov_fail, 0);
        assert_eq!(st.replay_fail, 0);
        assert!(st.mutated > 0, "mutations must actually happen");
        assert!(
            st.mutated_still_valid > 0,
            "some mutations should leave valid fabrics (honesty check), got {}",
            st.mutated_still_valid
        );
        assert!(st.rejected_total() > 0, "mutations must produce rejections");
    }

    #[test]
    fn generator_is_deterministic() {
        let a = gen_fabric(&mut Rng::new(12345));
        let b = gen_fabric(&mut Rng::new(12345));
        assert_eq!(a, b, "same seed must produce the identical fabric");
    }
}

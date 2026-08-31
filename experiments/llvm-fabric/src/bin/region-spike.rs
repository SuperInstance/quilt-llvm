//! REGION-SPIKE measurement harness (docs/phase/REGION-SPIKE.md).
//!
//! Breeds the GA corpus, then measures, over ≥100 real bred fabrics:
//!   * material: const-conditioned branches, unreachable regions,
//!     inline-eligible callees;
//!   * pass A/B/C end-to-end: verify-green rate, semantics preserved
//!     per the property oracle (interp), replay bit-identity;
//!   * raw op throughput: region_add / join_phi / drop_edge /
//!     region_remove / region_graft, ops/sec;
//!   * phi-join maintenance success rate on every branch arm.
//!
//! Deterministic: fixed seeds. Release mode for the timing numbers.

use llvm_fabric::cell::{Cell, CellKind};
use llvm_fabric::diff::History;
use llvm_fabric::fabric::Fabric;
use llvm_fabric::ga::{self, GaConfig};
use llvm_fabric::id::{CellId, RegionId};
use llvm_fabric::region::{
    cfg_graft_inline, const_branch_fold, constify, drop_edge, interp, join_phi,
    reachable_regions, region_add, region_dce, region_graft, region_remove,
};
use llvm_fabric::semmut::eval_dataflow;
use llvm_fabric::ty::{ConstVal, Type};
use llvm_fabric::verify::verify;
use std::collections::BTreeMap;
use std::time::Instant;

fn has_const_branch(f: &Fabric) -> usize {
    // count branches whose cond is a dataflow const AND whose arms differ
    let mut n = 0;
    for (i, _) in f.regions.iter().enumerate() {
        if let Some(&t) = f.regions[i].cells.last() {
            if let Some(c) = f.cell(t) {
                if let CellKind::Branch { then_r, else_r } = &c.kind {
                    if then_r != else_r {
                        if let Some(&cond) = c.operands.first() {
                            if matches!(
                                eval_dataflow(f, cond, 0),
                                Some(ConstVal::I1(_))
                            ) {
                                n += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    n
}

fn unreachable_count(f: &Fabric) -> usize {
    let live = reachable_regions(f);
    (0..f.regions.len() as u32).filter(|i| !live.contains(i)).count()
}

/// Is this bred fabric usable as an INLINE CALLEE: verify green, entry
/// has no predecessors, every ret single-value of one common type?
fn inline_eligible_callee(f: &Fabric) -> Option<Type> {
    if verify(f).is_err() {
        return None;
    }
    let entry = RegionId(0);
    if !f.predecessors(entry).is_empty() {
        return None;
    }
    let mut ty: Option<Type> = None;
    for id in f.cells() {
        if let Some(c) = f.cell(id) {
            if let CellKind::Ret = &c.kind {
                let t = c.operands.first().and_then(|&o| f.ty_of(o))?;
                match ty {
                    None => ty = Some(t),
                    Some(t0) if t0 == t => {}
                    _ => return None,
                }
            }
        }
    }
    ty
}

/// A minimal synthetic main calling `callee` with const args of the
/// callee's param types and consuming the result in a same-region add
/// (the shape cfg_graft_inline guards require).
fn synth_main(callee: &Fabric, name: &str, ret_ty: Type, seed_val: i32) -> Fabric {
    let mut f = Fabric::empty();
    let e = f.add_region("entry");
    let mut args = vec![];
    let mut v = seed_val;
    let entry = RegionId(0);
    let params: Vec<Type> = callee
        .region(entry)
        .map(|r| r.cells.clone())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| match callee.cell(id).map(|c| &c.kind) {
            Some(CellKind::Param { ty }) => Some(*ty),
            _ => None,
        })
        .collect();
    for ty in params {
        let val = match ty {
            Type::I1 => ConstVal::I1(v % 2 == 0),
            Type::I32 => ConstVal::I32(v),
            Type::I64 => ConstVal::I64(v as i64),
            Type::F64 => ConstVal::F64(v as f64 / 4.0),
        };
        let a = f.add_cell(e, Cell::new(e, CellKind::Const { ty, val }));
        args.push(a);
        v = v.wrapping_mul(3).wrapping_add(7);
    }
    let mut call = Cell::new(e, CellKind::Call { name: name.into(), ret_ty });
    call.operands = args;
    let call = f.add_cell(e, call);
    let c2 = f.add_cell(
        e,
        Cell::new(
            e,
            CellKind::Const {
                ty: ret_ty,
                val: match ret_ty {
                    Type::I1 => ConstVal::I1(true),
                    Type::I32 => ConstVal::I32(5),
                    Type::I64 => ConstVal::I64(5),
                    Type::F64 => ConstVal::F64(5.0),
                },
            },
        ),
    );
    let mut add = Cell::new(e, CellKind::Arith { op: llvm_fabric::cell::ArithOp::Add, ty: ret_ty });
    add.operands = vec![call, c2];
    let add = f.add_cell(e, add);
    let mut ret = Cell::new(e, CellKind::Ret);
    ret.operands = vec![add];
    f.add_cell(e, ret);
    f
}

#[derive(Default, Debug)]
struct PassCounts {
    attempted: usize,
    verify_green: usize,
    verify_red: usize,
    interp_preserved: usize,
    interp_changed: usize, // would be a bug — reported loudly
    unjudgeable: usize,
    replay_identical: usize,
    replay_diverged: usize,
    sites: usize,
}

fn main() {
    // ---------------- breed the corpus ----------------
    let t0 = Instant::now();
    let cfg = GaConfig { population: 200, generations: 50, elite: 20, tournament: 5, seed: 0x6A1C0 };
    let (rep, pop) = ga::run_keep(&cfg);
    let bred: Vec<Fabric> = pop.into_iter().filter(|f| verify(f).is_ok()).collect();
    println!("breeding: {} gens x {} pop in {:?} (best fitness {})", cfg.generations, cfg.population, t0.elapsed(), rep.best_fitness);
    println!("verify-green bred fabrics: {} ({}% of population)", bred.len(), bred.len() * 100 / cfg.population);
    assert!(bred.len() >= 100, "the spike needs >=100 bred fabrics, got {}", bred.len());

    // ---------------- material ----------------
    let mut const_br_fabs = 0usize;
    let mut const_br_sites = 0usize;
    let mut unreach_fabs = 0usize;
    let mut unreach_regions = 0usize;
    let mut eligible_callees = 0usize;
    for f in &bred {
        let n = has_const_branch(f);
        if n > 0 {
            const_br_fabs += 1;
            const_br_sites += n;
        }
        let u = unreachable_count(f);
        if u > 0 {
            unreach_fabs += 1;
            unreach_regions += u;
        }
        if inline_eligible_callee(f).is_some() {
            eligible_callees += 1;
        }
    }
    println!(
        "\nMATERIAL over {} bred fabrics:",
        bred.len()
    );
    println!("  const-conditioned branches: {} sites in {} fabrics (avg {:.2}/fab)", const_br_sites, const_br_fabs, const_br_sites as f64 / bred.len() as f64);
    println!("  unreachable regions: {} regions in {} fabrics ({:.1}% of fabrics)", unreach_regions, unreach_fabs, unreach_fabs as f64 * 100.0 / bred.len() as f64);
    println!("  inline-eligible callees (verify+acyclic entry+uniform rets): {}", eligible_callees);

    // ---------------- Pass A: const-branch fold ----------------
    let mut a = PassCounts::default();
    let mut a_twin_decidable = 0usize;
    let mut a_twin_preserved = 0usize;
    let mut a_twin_changed = 0usize;
    let t = Instant::now();
    let mut ops_a = 0usize;
    for f in &bred {
        a.attempted += 1;
        a.sites += has_const_branch(f);
        // property oracle on the param-constified twin (decidable runs)
        let twin = constify(f);
        let twin_before = interp(&twin, &BTreeMap::new(), 100_000);
        match const_branch_fold(f) {
            Ok((g, rec, st)) => {
                a.verify_green += 1;
                ops_a += st.folded.max(1);
                // twin through the same pass for the oracle leg
                if let Ok((tg, _trec, _tst)) = const_branch_fold(&twin) {
                    let ta = interp(&tg, &BTreeMap::new(), 100_000);
                    match (twin_before, ta) {
                        (Some(p), Some(q)) => {
                            a_twin_decidable += 1;
                            if p == q { a_twin_preserved += 1; } else { a_twin_changed += 1; }
                        }
                        _ => {}
                    }
                }
                let (x, y) = (interp(f, &BTreeMap::new(), 100_000), interp(&g, &BTreeMap::new(), 100_000));
                match (x, y) {
                    (Some(p), Some(q)) => {
                        if p == q {
                            a.interp_preserved += 1;
                        } else {
                            a.interp_changed += 1;
                        }
                    }
                    (None, None) => a.unjudgeable += 1,
                    _ => a.interp_changed += 1, // decidability changed: loudly a bug
                }
                // replay bit-identity
                let mut h = History::new();
                h.push(rec);
                match llvm_fabric::replay::replay(f, &h) {
                    Ok((_, final_r)) if final_r == g => a.replay_identical += 1,
                    Ok(_) => a.replay_diverged += 1,
                    Err(_) => a.replay_diverged += 1,
                }
            }
            Err(_) => a.verify_red += 1,
        }
    }
    let dt_a = t.elapsed();
    println!(
        "\nPASS A const-branch fold: attempted {} (sites {}), verify-green {}, red {}; interp preserved {} / changed {} / unjudgeable {}; replay identical {} / diverged {}",
        a.attempted, a.sites, a.verify_green, a.verify_red, a.interp_preserved, a.interp_changed, a.unjudgeable, a.replay_identical, a.replay_diverged
    );
    println!(
        "  oracle on param-constified twins: decidable {} / preserved {} / CHANGED {}",
        a_twin_decidable, a_twin_preserved, a_twin_changed
    );
    println!("  ops/sec (pass invocations): {:.0}; edits/sec (fold ops): {:.0}", a.attempted as f64 / dt_a.as_secs_f64(), ops_a as f64 / dt_a.as_secs_f64());

    // ---------------- Pass B: region-DCE ----------------
    let mut b = PassCounts::default();
    let mut b_twin_decidable = 0usize;
    let mut b_twin_preserved = 0usize;
    let mut b_twin_changed = 0usize;
    let t = Instant::now();
    let mut ops_b = 0usize;
    for f in &bred {
        let u = unreachable_count(f);
        if u == 0 {
            continue;
        }
        b.attempted += 1;
        b.sites += u;
        let twin = constify(f);
        let twin_before = interp(&twin, &BTreeMap::new(), 100_000);
        if let Ok((tg, _trec, _tst)) = region_dce(&twin) {
            let ta = interp(&tg, &BTreeMap::new(), 100_000);
            match (twin_before, ta) {
                (Some(p), Some(q)) => {
                    b_twin_decidable += 1;
                    if p == q { b_twin_preserved += 1; } else { b_twin_changed += 1; }
                }
                _ => {}
            }
        }
        match region_dce(f) {
            Ok((g, _rec, st)) => {
                b.verify_green += 1;
                ops_b += st.regions_removed.max(1);
                let (x, y) = (interp(f, &BTreeMap::new(), 100_000), interp(&g, &BTreeMap::new(), 100_000));
                match (x, y) {
                    (Some(p), Some(q)) if p == q => b.interp_preserved += 1,
                    (None, None) => b.unjudgeable += 1,
                    _ => b.interp_changed += 1,
                }
                // replay: RemoveCell edits exist, but region compaction
                // is inexpressible — replay must diverge (the finding)
                let mut h = History::new();
                h.push(_rec);
                match llvm_fabric::replay::replay(f, &h) {
                    Ok((_, final_r)) if final_r == g => b.replay_identical += 1,
                    Ok(_) => b.replay_diverged += 1,
                    Err(_) => b.replay_diverged += 1,
                }
            }
            Err(_) => b.verify_red += 1,
        }
    }
    let dt_b = t.elapsed();
    println!(
        "\nPASS B region-dce: attempted {} (dead regions {}), verify-green {}, red {}; interp preserved {} / changed {} / unjudgeable {}; replay identical {} / diverged {}",
        b.attempted, b.sites, b.verify_green, b.verify_red, b.interp_preserved, b.interp_changed, b.unjudgeable, b.replay_identical, b.replay_diverged
    );
    println!(
        "  oracle on param-constified twins: decidable {} / preserved {} / CHANGED {}",
        b_twin_decidable, b_twin_preserved, b_twin_changed
    );
    println!("  ops/sec (pass invocations): {:.0}; region removals/sec: {:.0}", b.attempted as f64 / dt_b.as_secs_f64(), ops_b as f64 / dt_b.as_secs_f64());

    // ---------------- Pass C: CFG-graft inline ----------------
    // Callee pool: the GA SEED corpus (ga.rs seeds gen-0 from
    // fuzz::gen_fabric — the audit's C1 world) plus bred survivors
    // that stay eligible. GA mutation pressure (mut_grow back-edges)
    // destroys callee eligibility; measured and stated.
    let mut callee_pool: Vec<(bool, Fabric)> = vec![];
    for i in 0..200u64 {
        let f = llvm_fabric::fuzz::gen_fabric(&mut llvm_fabric::fuzz::Rng::new(cfg.seed.wrapping_add(i).max(1)));
        if verify(&f).is_err() {
            continue;
        }
        if inline_eligible_callee(&f).is_some() {
            callee_pool.push((false, f)); // seed-corpus callee
        }
    }
    for f in &bred {
        if inline_eligible_callee(f).is_some() {
            callee_pool.push((true, f.clone())); // bred callee
        }
    }
    let n_bred_callees = callee_pool.iter().filter(|(b, _)| *b).count();
    let mut c = PassCounts::default();
    let mut c_twin_decidable = 0usize;
    let mut c_twin_preserved = 0usize;
    let mut c_twin_changed = 0usize;
    let t = Instant::now();
    let mut programs = 0usize;
    for (i, (is_bred, f)) in callee_pool.iter().enumerate() {
        let ret_ty = match inline_eligible_callee(f) {
            Some(t) => t,
            None => continue,
        };
        // oracle leg: constified callee twin (decidable conds)
        let twin_callee = constify(f);
        let twin_main = synth_main(&twin_callee, "bred_callee", ret_ty, (i as i32 % 97) + 3);
        let main = synth_main(f, "bred_callee", ret_ty, (i as i32 % 97) + 3);
        if verify(&main).is_err() {
            continue;
        }
        programs += 1;
        let mut funcs = BTreeMap::new();
        funcs.insert("bred_callee".to_string(), f.clone());
        let before = interp(&main, &funcs, 100_000);
        let mut tfuncs = BTreeMap::new();
        tfuncs.insert("bred_callee".to_string(), twin_callee.clone());
        let twin_before = interp(&twin_main, &tfuncs, 100_000);
        c.attempted += 1;
        match cfg_graft_inline(&main, &funcs) {
            Ok((g, rec, st)) => {
                c.verify_green += 1;
                c.sites += st.inlined;
                let after = interp(&g, &BTreeMap::new(), 100_000);
                match (before, after) {
                    (Some(p), Some(q)) if p == q => c.interp_preserved += 1,
                    (None, None) => c.unjudgeable += 1,
                    _ => c.interp_changed += 1,
                }
                // twin leg: inline the twin program too
                if let Ok((tg, _trec, _tst)) = cfg_graft_inline(&twin_main, &tfuncs) {
                    let ta = interp(&tg, &BTreeMap::new(), 100_000);
                    match (twin_before, ta) {
                        (Some(p), Some(q)) => {
                            c_twin_decidable += 1;
                            if p == q { c_twin_preserved += 1; } else { c_twin_changed += 1; }
                        }
                        _ => {}
                    }
                }
                let mut h = History::new();
                h.push(rec);
                match llvm_fabric::replay::replay(&main, &h) {
                    Ok((_, final_r)) if final_r == g => c.replay_identical += 1,
                    Ok(_) => c.replay_diverged += 1,
                    Err(_) => c.replay_diverged += 1,
                }
            }
            Err(_) => c.verify_red += 1,
        }
        if programs >= 120 {
            break;
        }
    }
    let dt_c = t.elapsed();
    println!(
        "\nPASS C cfg-graft inline: callee pool {} ({} bred / {} seed-corpus); programs {} attempted {}, verify-green {}, red {}; interp preserved {} / changed {} / unjudgeable {}; replay identical {} / diverged {}",
        callee_pool.len(), n_bred_callees, callee_pool.len() - n_bred_callees, programs, c.attempted, c.verify_green, c.verify_red, c.interp_preserved, c.interp_changed, c.unjudgeable, c.replay_identical, c.replay_diverged
    );
    println!(
        "  oracle on constified-callee twins: decidable {} / preserved {} / CHANGED {}",
        c_twin_decidable, c_twin_preserved, c_twin_changed
    );
    println!("  ops/sec (pass invocations): {:.0}", c.attempted as f64 / dt_c.as_secs_f64());

    // ---------------- GA SEED CORPUS leg (ga.rs gen-0 fabrics) -------
    // the literal "GA seed corpus": the seed population ga.rs breeds
    // from (fuzz::gen_fabric, the same seeding run_keep uses)
    let seed_corpus: Vec<Fabric> = (0..200u64)
        .map(|i| llvm_fabric::fuzz::gen_fabric(&mut llvm_fabric::fuzz::Rng::new(cfg.seed.wrapping_add(i).max(1))))
        .filter(|f| verify(f).is_ok())
        .collect();
    let mut s_const_sites = 0usize;
    let mut s_const_fabs = 0usize;
    let mut s_unreach_fabs = 0usize;
    let mut s_fold_green = 0usize;
    let mut s_fold_red = 0usize;
    let mut s_fold_replay = 0usize;
    let mut s_dce_green = 0usize;
    let mut s_dce_red = 0usize;
    let mut s_interp_pairs = 0usize;
    let mut s_interp_preserved = 0usize;
    for f in &seed_corpus {
        let n = has_const_branch(f);
        if n > 0 {
            s_const_fabs += 1;
            s_const_sites += n;
            match const_branch_fold(f) {
                Ok((g, rec, _st)) => {
                    s_fold_green += 1;
                    let (x, y) = (interp(f, &BTreeMap::new(), 100_000), interp(&g, &BTreeMap::new(), 100_000));
                    if x.is_some() && y.is_some() {
                        s_interp_pairs += 1;
                        if x == y {
                            s_interp_preserved += 1;
                        }
                    }
                    let mut h = History::new();
                    h.push(rec);
                    if let Ok((_, fr)) = llvm_fabric::replay::replay(f, &h) {
                        if fr == g {
                            s_fold_replay += 1;
                        }
                    }
                }
                Err(_) => s_fold_red += 1,
            }
        }
        if unreachable_count(f) > 0 {
            s_unreach_fabs += 1;
            match region_dce(f) {
                Ok((g, _rec, _st)) => {
                    s_dce_green += 1;
                    let (x, y) = (interp(f, &BTreeMap::new(), 100_000), interp(&g, &BTreeMap::new(), 100_000));
                    if x.is_some() && y.is_some() {
                        s_interp_pairs += 1;
                        if x == y {
                            s_interp_preserved += 1;
                        }
                    }
                }
                Err(_) => s_dce_red += 1,
            }
        }
    }
    println!(
        "\nGA SEED CORPUS leg ({} verify-green seed fabrics): const-cond sites {} in {} fabrics -> fold green {} / red {} (replay identical {}); unreachable in {} fabrics -> dce green {} / red {}; interp preserved {}/{} decidable",
        seed_corpus.len(), s_const_sites, s_const_fabs, s_fold_green, s_fold_red, s_fold_replay, s_unreach_fabs, s_dce_green, s_dce_red, s_interp_preserved, s_interp_pairs
    );
    // seed-corpus drop_edge: the apples-to-apples leg against semmut's
    // join-drop-with-edge number (123/250, measured over gen_fabric)
    {
        let mut st = 0usize;
        let mut legal = 0usize;
        let mut refused_struct = 0usize;
        let mut refused_single = 0usize;
        let mut red = 0usize;
        for f in &seed_corpus {
            for (i, _) in f.regions.iter().enumerate() {
                let src = RegionId(i as u32);
                let term = match f.regions[i].cells.last() { Some(&t) => t, None => continue };
                let (then_r, else_r) = match f.cell(term).map(|c| c.kind.clone()) {
                    Some(CellKind::Branch { then_r, else_r }) => (then_r, else_r),
                    _ => continue,
                };
                if then_r == else_r { continue; }
                for arm in [then_r, else_r] {
                    st += 1;
                    match drop_edge(f, src, arm) {
                        Ok(_) => legal += 1,
                        Err(e) if e.contains("no edge to drop") || e.contains("does not target") => refused_struct += 1,
                        Err(e) if e.contains("cannot legally replace") => refused_single += 1,
                        Err(_) => red += 1,
                    }
                }
            }
        }
        println!(
            "  seed-corpus drop_edge: arms {} -> verify-legal {} ({:.1}%); single-join-refused {}; structural {}; red {}  [semmut join-drop-with-edge on the same generator distribution: 123/250 = 49.2%]",
            st, legal, legal as f64 * 100.0 / st as f64, refused_single, refused_struct, red
        );
    }

    // ---------------- phi-join maintenance: every branch arm ----------------
    // (the semmut 123/250 comparison — now with the zero-join strategy)
    let mut arms_total = 0usize;
    let mut arms_legal = 0usize;
    let mut collapses = 0usize;
    let mut materialized = 0usize;
    let mut single_join_refused = 0usize;
    let mut refused = 0usize;
    let mut red = 0usize;
    let mut fail_reasons: BTreeMap<String, usize> = BTreeMap::new();
    let t = Instant::now();
    let mut interp_checked = 0usize;
    let mut interp_preserved = 0usize;
    for f in &bred {
        for (i, _) in f.regions.iter().enumerate() {
            let src = RegionId(i as u32);
            let term = match f.regions[i].cells.last() {
                Some(&t) => t,
                None => continue,
            };
            let (then_r, else_r) = match f.cell(term).map(|c| c.kind.clone()) {
                Some(CellKind::Branch { then_r, else_r }) => (then_r, else_r),
                _ => continue,
            };
            if then_r == else_r {
                continue;
            }
            for arm in [then_r, else_r] {
                arms_total += 1;
                match drop_edge(f, src, arm) {
                    Ok((g, rec)) => {
                        arms_legal += 1;
                        collapses += rec.notes.iter().filter(|n| n.contains("collapsed to")).count();
                        materialized += rec.notes.iter().filter(|n| n.contains("materialized as const")).count();
                        // semantics: dropping a CONST-SELECTED dead arm preserves
                        let (x, y) = (interp(f, &BTreeMap::new(), 100_000), interp(&g, &BTreeMap::new(), 100_000));
                        if x.is_some() || y.is_some() {
                            interp_checked += 1;
                            if x == y {
                                interp_preserved += 1;
                            }
                        }
                    }
                    Err(e) => {
                        if e.contains("targets") || e.contains("no edge to drop") {
                            refused += 1;
                        } else if e.contains("cannot legally replace it") {
                            single_join_refused += 1;
                        } else {
                            red += 1;
                            let key = e.split(':').next().unwrap_or("?").to_string();
                            *fail_reasons.entry(key).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
    }
    let dt_phi = t.elapsed();
    println!(
        "\nPHI-JOIN MAINTENANCE (drop_edge on every branch arm of every bred fabric):"
    );
    println!(
        "  arms {} -> verify-legal {} ({:.1}%); collapses {}, materialized {}, single-join-refused {}, structural-refused {}, verify-red {}",
        arms_total, arms_legal, arms_legal as f64 * 100.0 / arms_total as f64, collapses, materialized, single_join_refused, refused, red
    );
    if !fail_reasons.is_empty() {
        println!("  verify-red reasons: {:?}", fail_reasons);
    }
    println!(
        "  semantics spot-check on decidable arms: {}/{} preserved",
        interp_preserved, interp_checked
    );
    println!("  drop_edge ops/sec: {:.0}", arms_total as f64 / dt_phi.as_secs_f64());

    // ---------------- raw op throughput ----------------
    let n = bred.len();
    // region_add
    let t = Instant::now();
    for f in &bred {
        let _ = region_add(f, "x");
    }
    println!("\nRAW OPS over {} bred fabrics:", n);
    println!("  region_add:  {:.0} ops/sec", n as f64 / t.elapsed().as_secs_f64());

    // join_phi: two measurable sites — the guard path (entry join on
    // a random phi: V06 refuses unless entry is a pred) and the LEGAL
    // path (fresh region + jump into a phi region, then join — the
    // maintenance inverse drop_edge undoes).
    let mut jp_guard = 0usize;
    let t = Instant::now();
    for f in &bred {
        for id in f.cells() {
            if matches!(f.cell(id).map(|c| &c.kind), Some(CellKind::Phi { .. })) {
                let _ = join_phi(f, id, RegionId(0), CellId(0));
                jp_guard += 1;
                break;
            }
        }
    }
    let guard_rate = jp_guard as f64 / t.elapsed().as_secs_f64();
    let mut jp_ok = 0usize;
    let t = Instant::now();
    for f in &bred {
        // find a phi; add a fresh region jumping into its region; join
        let phi = match f.cells().find(|&id| matches!(f.cell(id).map(|c| &c.kind), Some(CellKind::Phi { .. }))) {
            Some(p) => p,
            None => continue,
        };
        let target = f.cell(phi).unwrap().region;
        let (mut g, new_r, _r) = region_add(f, "jp_bench");
        let v = g.add_cell(new_r, Cell::new(new_r, CellKind::Const { ty: Type::I32, val: ConstVal::I32(3) }));
        g.add_cell(new_r, Cell::new(new_r, CellKind::Jump { target }));
        let _ = v;
        // value must be defined in the join region (new_r) — v is; the
        // phi must accept i32... only when types align does it succeed
        if g.cell(phi).is_some() {
            if g.ty_of(phi) == Some(Type::I32) {
                if join_phi(&g, phi, new_r, v).is_ok() {
                    jp_ok += 1;
                }
            }
        }
    }
    println!(
        "  join_phi:    guard-path {:.0} ops/sec; legal-path {:.0} ops/sec ({} legal joins)",
        guard_rate,
        (jp_ok.max(1)) as f64 / t.elapsed().as_secs_f64(),
        jp_ok
    );

    // region_remove: only on fabrics with unreachable regions (legal
    // sites after dce's join strip is skipped — measure the RAW op's
    // refusal behavior too by trying live regions)
    let mut rr_ok = 0usize;
    let mut rr_refused = 0usize;
    let t = Instant::now();
    for f in &bred {
        // try to remove the LAST region (often unreachable / referenced)
        if f.regions.len() < 2 {
            continue;
        }
        match region_remove(f, RegionId(f.regions.len() as u32 - 1)) {
            Ok(_) => rr_ok += 1,
            Err(_) => rr_refused += 1,
        }
    }
    println!(
        "  region_remove: {:.0} ops/sec (applied {} / refused {})",
        (rr_ok + rr_refused) as f64 / t.elapsed().as_secs_f64(),
        rr_ok,
        rr_refused
    );

    // region_graft: graft the first non-entry region of fabric j into i
    let mut grafts = 0usize;
    let t = Instant::now();
    for (i, f) in bred.iter().enumerate() {
        let donor = &bred[(i + 1) % bred.len()];
        if donor.regions.len() < 2 {
            continue;
        }
        let dr = RegionId(1);
        // map: every donor operand of that region -> a const in f's
        // entry is too naive; graft with an empty map and count
        // successful closed-region grafts only
        let map = BTreeMap::new();
        let rmap: BTreeMap<u32, RegionId> =
            (0..donor.regions.len() as u32).map(|r| (r, RegionId(0))).collect();
        if region_graft(f, donor, dr, &map, &rmap, "g").is_ok() {
            grafts += 1;
        }
    }
    println!(
        "  region_graft: {:.0} ops/sec (closed-region grafts {}; unclosed operands refuse loudly)",
        grafts as f64 / t.elapsed().as_secs_f64(),
        grafts
    );

    // ---------------- demo fabrics for the doc ----------------
    println!("\nDEMO FABRICS (first per pass, for the doc):");
    for f in &bred {
        if has_const_branch(f) > 0 {
            if let Ok((g, _rec, st)) = const_branch_fold(f) {
                if verify(&g).is_ok() && st.folded >= 1 {
                    println!("--- fold demo ({} sites folded) ---", st.folded);
                    println!("{}", llvm_fabric::text::print(f));
                    break;
                }
            }
        }
    }
    for f in &bred {
        if unreachable_count(f) > 0 {
            if let Ok((g, _rec, st)) = region_dce(f) {
                if verify(&g).is_ok() {
                    println!("--- dce demo ({} regions removed) ---", st.regions_removed);
                    println!("{}", llvm_fabric::text::print(f));
                    break;
                }
            }
        }
    }
    // prefer a BRED callee, fall back to a seed-corpus one (the GA's
    // own gen-0 population — the audit's C1 world); 0 bred callees
    // exist this run (measured above)
    for (i, (is_bred, f)) in callee_pool.iter().enumerate() {
        if f.regions.len() >= 2 {
            if let Some(ret_ty) = inline_eligible_callee(f) {
                let main = synth_main(f, "bred_callee", ret_ty, (i as i32 % 97) + 3);
                let mut funcs = BTreeMap::new();
                funcs.insert("bred_callee".to_string(), f.clone());
                if let Ok((g, _rec, st)) = cfg_graft_inline(&main, &funcs) {
                    if verify(&g).is_ok() && st.inlined == 1 {
                        println!("--- inline demo ({} multi-region callee, {} regions) ---", if *is_bred { "GA-BRED" } else { "GA-seed-corpus" }, f.regions.len());
                        println!("MAIN:\n{}", llvm_fabric::text::print(&main));
                        println!("CALLEE ({}):\n{}", if *is_bred { "GA-bred" } else { "GA seed corpus (ga.rs gen-0)" }, llvm_fabric::text::print(f));
                        break;
                    }
                }
            }
        }
    }
    println!("\nDONE");
}

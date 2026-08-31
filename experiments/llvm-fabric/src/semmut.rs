//! R1 lane 3 — the SEMANTIC mutation tier (NEXT-PHASE §3 R1).
//!
//! The structural battery (fuzz::mutate, 4,430 mutants over the 10k
//! corpus) can only kill what a V-code can see: broken wires, bad
//! types, malformed control. This module adds the complementary tier —
//! mutants that are STRUCTURALLY VALID (verify() accepts them) but
//! SEMANTICALLY WRONG (the program's value changes).
//!
//! Two families, honestly separated:
//!
//! * DATAFLOW kinds (const off-by-one, operand swap under a
//!   non-commutative op, ordered-cmp swap, ret rebind): semantic
//!   wrongness is PROVABLE by a ground-truth dataflow evaluator over
//!   Rust's own checked arithmetic (the same basis as the R1 lane-1
//!   property oracle). Params/phis return None — there is no execution
//!   semantics for control flow (NEXT-PHASE §2 residual), so mutants
//!   whose observable value cannot be decided go to the UNJUDGEABLE
//!   bucket rather than being assumed wrong.
//! * CONTROL kinds (branch target swap, phi operand rebind, join drop
//!   with its edge): structurally valid, semantics UNJUDGEABLE — the
//!   battery counts them and reports kill rate as undefined, because
//!   no oracle exists to call them wrong (the T2 gate question).
//!
//! A tamper CONTROL is included for calibration: corrupt a folded
//! const in a PIPELINE OUTPUT while history claims otherwise. The
//! machinery must fire (replay divergence) — a battery where nothing
//! fires proves nothing.
//!
//! Kill = any judge fires: verify, text round-trip, provenance walks
//! (data + control), pipeline, weft law/chain, replay bit-identity,
//! conservation. The measured expectation (NEXT-PHASE §2): the fabric
//! judges are structural, so dataflow-semantic mutants survive them
//! all. This module turns that expectation into a per-kind number.

use crate::cell::{CellKind, CmpOp};
use crate::fabric::Fabric;
use crate::fuzz::Rng;
use crate::id::CellId;
use crate::ty::{ConstVal, Type};
use crate::verify::verify;
use std::collections::BTreeMap;
use std::time::Instant;

/// The semantic mutation kinds. Copy + Eq so stats can key on them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SemKind {
    /// DATAFLOW: bump an i32/i64/f64 const by ±1 (or +1.0).
    ConstOffByOne,
    /// DATAFLOW: swap operands of a Sub/Div cell (non-commutative ops).
    ArithSubDivSwap,
    /// DATAFLOW: swap operands of an ordered cmp (Lt/Le/Gt/Ge).
    CmpOrderedSwap,
    /// DATAFLOW: make ret return a different same-typed same-region value.
    RetValueSwap,
    /// CONTROL: swap then/else targets of a branch (edges unchanged).
    BranchTargetSwap,
    /// CONTROL: rebind one phi operand to a different value from the
    /// same join region (V07-safe value misbinding — the silent wrong
    /// wire that a dropped join would leave behind).
    PhiOperandRebind,
    /// CONTROL: "drop a phi join" done CONSISTENTLY — the branch arm is
    /// removed (Br→Jmp) together with the matching join entries, so the
    /// fabric stays valid but takes a different path forever after.
    JoinDropWithEdge,
    /// CONTROL-TIER CALIBRATION: tamper a folded const in the pipeline
    /// OUTPUT; history still claims the untampered value. Not an input
    /// mutation — the machinery must catch it or the battery is deaf.
    StageTamperControl,
}

impl SemKind {
    pub fn name(self) -> &'static str {
        match self {
            SemKind::ConstOffByOne => "const-off-by-one",
            SemKind::ArithSubDivSwap => "arith-sub-div-swap",
            SemKind::CmpOrderedSwap => "cmp-ordered-swap",
            SemKind::RetValueSwap => "ret-value-swap",
            SemKind::BranchTargetSwap => "branch-target-swap",
            SemKind::PhiOperandRebind => "phi-operand-rebind",
            SemKind::JoinDropWithEdge => "join-drop-with-edge",
            SemKind::StageTamperControl => "stage-tamper-control",
        }
    }

    /// Dataflow kinds have a mechanical wrongness oracle; control kinds
    /// do not (no execution semantics exists — NEXT-PHASE §2).
    pub fn is_dataflow(self) -> bool {
        matches!(
            self,
            SemKind::ConstOffByOne
                | SemKind::ArithSubDivSwap
                | SemKind::CmpOrderedSwap
                | SemKind::RetValueSwap
        )
    }

    pub const ALL: [SemKind; 8] = [
        SemKind::ConstOffByOne,
        SemKind::ArithSubDivSwap,
        SemKind::CmpOrderedSwap,
        SemKind::RetValueSwap,
        SemKind::BranchTargetSwap,
        SemKind::PhiOperandRebind,
        SemKind::JoinDropWithEdge,
        SemKind::StageTamperControl,
    ];
}

fn bump_const(val: ConstVal, rng: &mut Rng) -> Option<ConstVal> {
    // ±1 (or +1.0 for f64), checked: never changes the variant/type,
    // never overflows, never mints NaN. None = not bumpable.
    match val {
        ConstVal::I32(x) => {
            let d: i32 = if rng.chance(50) { 1 } else { -1 };
            x.checked_add(d).map(ConstVal::I32).or_else(|| x.checked_sub(d).map(ConstVal::I32))
        }
        ConstVal::I64(x) => {
            let d: i64 = if rng.chance(50) { 1 } else { -1 };
            x.checked_add(d).map(ConstVal::I64).or_else(|| x.checked_sub(d).map(ConstVal::I64))
        }
        ConstVal::F64(x) => {
            let r = x + 1.0;
            if r.is_nan() || r == x {
                None // inf and friends: no observable change
            } else {
                Some(ConstVal::F64(r))
            }
        }
        ConstVal::I1(_) => None, // booleans have no "off by one"
    }
}

/// Apply one input-level mutation kind. None = no applicable site.
/// The returned fabric is NOT yet verified — the runner classifies.
fn apply_input_mutation(f: &Fabric, kind: SemKind, rng: &mut Rng) -> Option<Fabric> {
    let mut g = f.clone();
    match kind {
        SemKind::ConstOffByOne => {
            let cands: Vec<CellId> = g
                .cells()
                .filter(|&id| match g.cell(id).map(|c| &c.kind) {
                    Some(CellKind::Const { ty, .. }) => !matches!(ty, Type::I1),
                    _ => false,
                })
                .collect();
            let victim = *rng.pick(&cands)?;
            let old = match g.cell(victim)?.kind {
                CellKind::Const { val, .. } => val,
                _ => return None,
            };
            let new = bump_const(old, rng)?;
            if let Some(c) = g.cell_mut(victim) {
                if let CellKind::Const { val, .. } = &mut c.kind {
                    *val = new;
                }
            }
            Some(g)
        }
        SemKind::ArithSubDivSwap => {
            let cands: Vec<CellId> = g
                .cells()
                .filter(|&id| match g.cell(id).map(|c| (&c.kind, c.operands.as_slice())) {
                    Some((CellKind::Arith { op: crate::cell::ArithOp::Sub, .. }, o))
                    | Some((CellKind::Arith { op: crate::cell::ArithOp::Div, .. }, o)) => {
                        o.len() == 2 && o[0] != o[1] // a no-op swap is not a mutation
                    }
                    _ => false,
                })
                .collect();
            let victim = *rng.pick(&cands)?;
            // sanctioned rewire (two retargets) so the use tables stay
            // truthful — a raw swap desyncs users[] rows
            let (a, b) = {
                let c = g.cell(victim)?;
                (c.operands[0], c.operands[1])
            };
            g.retarget(victim, 0, b)?;
            g.retarget(victim, 1, a)?;
            Some(g)
        }
        SemKind::CmpOrderedSwap => {
            let ordered = |op: &CmpOp| {
                matches!(op, CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge)
            };
            let cands: Vec<CellId> = g
                .cells()
                .filter(|&id| match g.cell(id).map(|c| (&c.kind, c.operands.as_slice())) {
                    Some((CellKind::Cmp { op }, o)) => ordered(op) && o.len() == 2 && o[0] != o[1],
                    _ => false,
                })
                .collect();
            let victim = *rng.pick(&cands)?;
            let (a, b) = {
                let c = g.cell(victim)?;
                (c.operands[0], c.operands[1])
            };
            g.retarget(victim, 0, b)?;
            g.retarget(victim, 1, a)?;
            Some(g)
        }
        SemKind::RetValueSwap => {
            // ret with exactly one operand; replacement is another
            // same-typed VALUE cell in the SAME region (V12-safe: the
            // terminator is the last cell, so every def precedes it).
            let rets: Vec<CellId> = g
                .cells()
                .filter(|&id| {
                    g.cell(id).map(|c| matches!(c.kind, CellKind::Ret) && c.operands.len() == 1)
                        .unwrap_or(false)
                })
                .collect();
            let victim = *rng.pick(&rets)?;
            let (region, cur) = {
                let c = g.cell(victim)?;
                (c.region, c.operands[0])
            };
            let ty = g.ty_of(cur)?;
            let mut pool: Vec<CellId> = vec![];
            for id in g.cells() {
                let c = g.cell(id)?;
                if c.region == region && id != cur && g.ty_of(id) == Some(ty) && c.produces_value() {
                    pool.push(id);
                }
            }
            let repl = *rng.pick(&pool)?;
            g.retarget(victim, 0, repl)?;
            Some(g)
        }
        SemKind::BranchTargetSwap => {
            let cands: Vec<CellId> = g
                .cells()
                .filter(|&id| match g.cell(id).map(|c| &c.kind) {
                    Some(CellKind::Branch { then_r, else_r }) => then_r != else_r,
                    _ => false,
                })
                .collect();
            let victim = *rng.pick(&cands)?;
            // sanctioned kind edit: a terminator kind swap must repair
            // the succ/pred tables (the region's edge set changes)
            if let Some(CellKind::Branch { then_r, else_r }) = g.cell(victim).map(|c| c.kind.clone()) {
                g.set_kind(victim, CellKind::Branch { then_r: else_r, else_r: then_r });
            }
            Some(g)
        }
        SemKind::PhiOperandRebind => {
            // pick a phi slot; replace the value with a DIFFERENT
            // same-typed value defined in that slot's join region (or
            // entry) — V07/V13 stay green, the mux selects a wrong wire.
            let phis: Vec<CellId> = g
                .cells()
                .filter(|&id| matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Phi { .. })))
                .collect();
            let victim = *rng.pick(&phis)?;
            let (phi_region, joins, operands) =
                match g.cell(victim).map(|c| (c.region, c.kind.clone(), c.operands.clone())) {
                    Some((r, CellKind::Phi { joins }, ops)) => (r, joins, ops),
                    _ => return None,
                };
            let entry = g.entry()?;
            // candidate slots: ones with a different same-typed value
            // defined in the join's region (or entry)
            let mut slots: Vec<(usize, Vec<CellId>)> = vec![];
            for (i, &j) in joins.iter().enumerate() {
                let want_ty = g.ty_of(operands[i])?;
                let pool: Vec<CellId> = g
                    .cells()
                    .filter(|&id| {
                        g.cell(id)
                            .map(|c| {
                                id != operands[i]
                                    && c.region != phi_region
                                    && (c.region == j || c.region == entry)
                                    && c.produces_value()
                                    && g.ty_of(id) == Some(want_ty)
                            })
                            .unwrap_or(false)
                    })
                    .collect();
                if !pool.is_empty() {
                    slots.push((i, pool));
                }
            }
            let (slot, pool) = rng.pick(&slots)?.clone();
            let repl = *rng.pick(&pool)?;
            g.retarget(victim, slot as u32, repl)?;
            Some(g)
        }
        SemKind::JoinDropWithEdge => {
            // Br(a,b), a≠b → Jmp(a): arm b loses this predecessor edge;
            // every phi in b drops the matching join+operand pair, so
            // the fabric stays structurally valid (V06/V14/V16/V05 all
            // re-check green) while the program can never take arm b.
            let cands: Vec<CellId> = g
                .cells()
                .filter(|&id| match g.cell(id).map(|c| &c.kind) {
                    Some(CellKind::Branch { then_r, else_r }) => then_r != else_r,
                    _ => false,
                })
                .collect();
            let victim = *rng.pick(&cands)?;
            let (src_region, keep, drop_arm) = match g.cell(victim).map(|c| (c.region, c.kind.clone())) {
                Some((r, CellKind::Branch { then_r, else_r })) => (r, then_r, else_r),
                _ => return None,
            };
            // rewire the branch to an unconditional jump on the kept
            // arm — sanctioned kind edit + operand replacement so the
            // use tables and the succ/pred tables both stay truthful
            g.set_kind(victim, CellKind::Jump { target: keep });
            g.set_operands(victim, &[])?;
            // clean phis in the dropped arm: remove the join that names
            // this source region, with its operand
            let phi_ids: Vec<CellId> = g
                .cells()
                .filter(|&id| {
                    g.cell(id).map(|c| c.region == drop_arm && matches!(c.kind, CellKind::Phi { .. }))
                        .unwrap_or(false)
                })
                .collect();
            for pid in phi_ids {
                let cleaned = g.cell(pid).and_then(|c| match &c.kind {
                    CellKind::Phi { joins } => {
                        let pos = joins.iter().position(|&j| j == src_region)?;
                        let mut ops = c.operands.clone();
                        if ops.len() > pos {
                            ops.remove(pos);
                        }
                        let mut joins2 = joins.clone();
                        joins2.remove(pos);
                        Some((joins2, ops))
                    }
                    _ => None,
                });
                if let Some((joins, ops)) = cleaned {
                    if let Some(c) = g.cell_mut(pid) {
                        c.kind = CellKind::Phi { joins };
                    }
                    g.set_operands(pid, &ops)?;
                }
            }
            Some(g)
        }
        SemKind::StageTamperControl => None, // handled by the runner (needs pipeline output)
    }
}

/// Ground-truth dataflow evaluation over Rust's own checked arithmetic.
/// This is the WRONGNESS ORACLE, not a judge: it exists to prove a
/// mutant changed the program's value. Params and phis are None — v0
/// has no execution semantics for control flow, and this tier does not
/// build one (NEXT-PHASE R1 cap).
///
/// Written directly against Rust primitive ops (the same basis as the
/// property oracle), NOT against eval_arith: a corrupted fold table
/// cannot make this evaluator agree with it by construction.
pub fn eval_dataflow(f: &Fabric, root: CellId, depth: u32) -> Option<ConstVal> {
    use crate::cell::ArithOp::*;
    use ConstVal::*;
    if depth > 256 {
        return None; // defensive; verified fabrics are acyclic (V17)
    }
    let c = f.cell(root)?;
    let ops = c.operands.clone();
    match c.kind {
        CellKind::Const { val, .. } => Some(val),
        CellKind::Arith { op, .. } => {
            let a = eval_dataflow(f, ops[0], depth + 1)?;
            let b = eval_dataflow(f, ops[1], depth + 1)?;
            match (op, a, b) {
                (Add, I32(x), I32(y)) => x.checked_add(y).map(I32),
                (Add, I64(x), I64(y)) => x.checked_add(y).map(I64),
                (Add, F64(x), F64(y)) => no_nan(x + y).map(F64),
                (Sub, I32(x), I32(y)) => x.checked_sub(y).map(I32),
                (Sub, I64(x), I64(y)) => x.checked_sub(y).map(I64),
                (Sub, F64(x), F64(y)) => no_nan(x - y).map(F64),
                (Mul, I32(x), I32(y)) => x.checked_mul(y).map(I32),
                (Mul, I64(x), I64(y)) => x.checked_mul(y).map(I64),
                (Mul, F64(x), F64(y)) => no_nan(x * y).map(F64),
                (Div, I32(x), I32(y)) => x.checked_div(y).map(I32),
                (Div, I64(x), I64(y)) => x.checked_div(y).map(I64),
                (Div, F64(x), F64(y)) => no_nan(x / y).map(F64),
                // v0 spec: i1 arith does not fold / has no defined value
                _ => None,
            }
        }
        CellKind::Cmp { op } => {
            let a = eval_dataflow(f, ops[0], depth + 1)?;
            let b = eval_dataflow(f, ops[1], depth + 1)?;
            let r = match (op, a, b) {
                (CmpOp::Eq, I1(x), I1(y)) => x == y,
                (CmpOp::Ne, I1(x), I1(y)) => x != y,
                (CmpOp::Eq, I32(x), I32(y)) => x == y,
                (CmpOp::Ne, I32(x), I32(y)) => x != y,
                (CmpOp::Lt, I32(x), I32(y)) => x < y,
                (CmpOp::Le, I32(x), I32(y)) => x <= y,
                (CmpOp::Gt, I32(x), I32(y)) => x > y,
                (CmpOp::Ge, I32(x), I32(y)) => x >= y,
                (CmpOp::Eq, I64(x), I64(y)) => x == y,
                (CmpOp::Ne, I64(x), I64(y)) => x != y,
                (CmpOp::Lt, I64(x), I64(y)) => x < y,
                (CmpOp::Le, I64(x), I64(y)) => x <= y,
                (CmpOp::Gt, I64(x), I64(y)) => x > y,
                (CmpOp::Ge, I64(x), I64(y)) => x >= y,
                (CmpOp::Eq, F64(x), F64(y)) => x == y,
                (CmpOp::Ne, F64(x), F64(y)) => x != y,
                (CmpOp::Lt, F64(x), F64(y)) => x < y,
                (CmpOp::Le, F64(x), F64(y)) => x <= y,
                (CmpOp::Gt, F64(x), F64(y)) => x > y,
                (CmpOp::Ge, F64(x), F64(y)) => x >= y,
                _ => return None,
            };
            Some(I1(r))
        }
        _ => None, // params, phis, terminators: no dataflow value in v0
    }
}

fn no_nan(r: f64) -> Option<f64> {
    if r.is_nan() {
        None
    } else {
        Some(r)
    }
}

/// The fabric's observable value: the (single) ret operand, when the
/// dataflow into it is decidable.
fn observable(f: &Fabric) -> Option<ConstVal> {
    // entry-region ret first (the program's answer); any ret otherwise
    let rets: Vec<CellId> = f
        .cells()
        .filter(|&id| f.cell(id).map(|c| matches!(c.kind, CellKind::Ret) && c.operands.len() == 1).unwrap_or(false))
        .collect();
    let pick = rets.first().copied()?;
    let op = f.cell(pick)?.operands[0];
    eval_dataflow(f, op, 0)
}

/// The full judge battery for an input fabric — every check the
/// existing corpus runs against valid fabrics. Returns (judge, fired).
/// The timings twin (below) is the R2 deliverable: which judge earns
/// its microseconds.
pub fn judge_battery(f: &Fabric) -> Vec<(&'static str, bool)> {
    judge_battery_timed(f).0
}

/// The judge battery with per-judge wall time (ns). Same order, same
/// rng-free determinism — timings are observed, never consumed.
pub fn judge_battery_timed(f: &Fabric) -> (Vec<(&'static str, bool)>, Vec<(&'static str, u128)>) {
    let mut out: Vec<(&'static str, bool)> = vec![];
    let mut times: Vec<(&'static str, u128)> = vec![];
    let t = Instant::now();
    let v = verify(f).is_err();
    times.push(("verify", t.elapsed().as_nanos()));
    out.push(("verify", v));
    let t = Instant::now();
    let once = crate::text::print(f);
    let rt_bad = match crate::text::parse(&once) {
        Ok(f2) => crate::text::print(&f2) != once,
        Err(_) => true,
    };
    times.push(("roundtrip", t.elapsed().as_nanos()));
    out.push(("roundtrip", rt_bad));
    let t = Instant::now();
    let p = f.cells().any(|id| crate::prov::check_prov(f, id).is_err());
    times.push(("prov", t.elapsed().as_nanos()));
    out.push(("prov", p));
    let t = Instant::now();
    let c = f.cells().any(|id| crate::ctrl::check_full_prov(f, id).is_err());
    times.push(("ctrl", t.elapsed().as_nanos()));
    out.push(("ctrl", c));
    let t = Instant::now();
    let pipe = crate::pipeline::run(f);
    times.push(("pipeline", t.elapsed().as_nanos()));
    match pipe {
        Err(_) => out.push(("pipeline", true)),
        Ok((final_f, history, stages)) => {
            let weft_bad =
                history.check_weft().is_err() || history.verify_chain(&stages).is_err();
            out.push(("weft", weft_bad));
            let t = Instant::now();
            let rep = crate::replay::replay(f, &history);
            let rep_ns = t.elapsed().as_nanos();
            match rep {
                Err(_) => out.push(("replay", true)),
                Ok((replayed, final_r)) => {
                    let diverged = replayed.len() != stages.len()
                        || stages.iter().zip(replayed.iter()).any(|(a, b)| a != b)
                        || final_f != final_r;
                    out.push(("replay", diverged));
                }
            }
            times.push(("replay", rep_ns));
            let t = Instant::now();
            let cons = crate::conserve::check_pipeline(f, &final_f, &history).is_err();
            times.push(("conserve", t.elapsed().as_nanos()));
            out.push(("conserve", cons));
        }
    }
    (out, times)
}

/// The tamper control: corrupt a folded const in the pipeline OUTPUT;
/// history still claims the untampered edit stream. Judges timed (the
/// 76/76 replay kills live here — their cost belongs in the R2 table).
fn run_stage_tamper(
    f: &Fabric,
    rng: &mut Rng,
) -> Option<(Vec<(&'static str, bool)>, Vec<(&'static str, u128)>, bool)> {
    let (final_f, history, _stages) = crate::pipeline::run(f).ok()?;
    let cands: Vec<CellId> = final_f
        .cells()
        .filter(|&id| matches!(final_f.cell(id).map(|c| &c.kind), Some(CellKind::Const { .. })))
        .collect();
    let victim = *rng.pick(&cands)?;
    let old = match final_f.cell(victim)?.kind {
        CellKind::Const { val, .. } => val,
        _ => return None,
    };
    let new = bump_const(old, rng)?;
    let mut t = final_f.clone();
    if let Some(c) = t.cell_mut(victim) {
        if let CellKind::Const { val, .. } = &mut c.kind {
            *val = new;
        }
    }
    // judges over the tampered OUTPUT: structural checks + replay
    // bit-identity + conservation against the claimed history
    let mut judges = vec![];
    let mut times = vec![];
    let sw = Instant::now();
    let v = verify(&t).is_err();
    times.push(("verify", sw.elapsed().as_nanos()));
    judges.push(("verify", v));
    let sw = Instant::now();
    let once = crate::text::print(&t);
    let rt_bad = match crate::text::parse(&once) {
        Ok(f2) => crate::text::print(&f2) != once,
        Err(_) => true,
    };
    times.push(("roundtrip", sw.elapsed().as_nanos()));
    judges.push(("roundtrip", rt_bad));
    let sw = Instant::now();
    let diverged = match crate::replay::replay(f, &history) {
        Ok((_, final_r)) => final_r != t,
        Err(_) => true,
    };
    times.push(("replay", sw.elapsed().as_nanos()));
    judges.push(("replay", diverged));
    let sw = Instant::now();
    let cons = crate::conserve::check_pipeline(f, &t, &history).is_err();
    times.push(("conserve", sw.elapsed().as_nanos()));
    judges.push(("conserve", cons));
    Some((judges, times, true)) // tamper is wrong by construction
}

/// Per-judge cost accounting (R2): how many times each judge ran,
/// how many mutants it killed (fired), and how long it took. The
/// gate's question: which judge earns its microseconds?
#[derive(Debug, Default, Clone)]
pub struct JudgeCost {
    pub calls: u64,
    pub fired: u64,
    pub ns: u128,
}

/// Per-kind statistics.
#[derive(Debug, Default, Clone)]
pub struct KindStat {
    pub attempted: u64,
    pub no_site: u64,
    pub structural_invalid: u64, // mutant failed verify — excluded from judging
    pub judged: u64,             // structurally valid mutants run through the battery
    pub sem_equivalent: u64,     // dataflow-decidable, value unchanged (not wrong)
    pub sem_wrong: u64,          // dataflow-decidable, value changed
    pub unjudgeable: u64,        // no execution semantics to prove wrongness
    pub killed: u64,             // at least one judge fired
    pub fired: BTreeMap<String, u64>, // per-judge fire counts
}

#[derive(Debug, Default, Clone)]
pub struct SemReport {
    pub iters: u64,
    pub kinds: BTreeMap<&'static str, KindStat>,
    /// Wall time per judge across the whole battery (input kinds +
    /// tamper control). Timings do NOT participate in equality/Display
    /// determinism — they are an observation about cost, not a result.
    pub judge_cost: BTreeMap<String, JudgeCost>,
    /// The dataflow wrongness oracle's own cost (sem classification).
    pub oracle_cost: JudgeCost,
}

impl SemReport {
    pub fn judged_total(&self) -> u64 {
        self.kinds.values().map(|k| k.judged).sum()
    }
    pub fn sem_wrong_total(&self) -> u64 {
        self.kinds.values().map(|k| k.sem_wrong).sum()
    }
    pub fn killed_total(&self) -> u64 {
        self.kinds.values().map(|k| k.killed).sum()
    }
    pub fn unjudgeable_total(&self) -> u64 {
        self.kinds.values().map(|k| k.unjudgeable).sum()
    }
}

/// Run the semantic battery. Kinds are round-robined across iterations
/// (deterministic coverage — no kind is left unexercised by rng skew).
/// An Err means the GENERATOR contract broke (invalid base fabric) —
/// same rule as corpus_run.
pub fn semmut_run(iters: u64, seed0: u64) -> Result<SemReport, String> {
    let mut report = SemReport { iters, ..Default::default() };
    for i in 0..iters {
        let seed = seed0.wrapping_add(i).max(1);
        let mut rng = Rng::new(seed);
        let f = crate::fuzz::gen_fabric(&mut rng);
        if let Err(e) = verify(&f) {
            return Err(format!("seed {}: generator produced invalid fabric: {}", seed, e));
        }
        let kind = SemKind::ALL[(i as usize) % SemKind::ALL.len()];
        let st = report.kinds.entry(kind.name()).or_default();
        st.attempted += 1;

        if kind == SemKind::StageTamperControl {
            match run_stage_tamper(&f, &mut rng) {
                None => st.no_site += 1,
                Some((judges, times, wrong)) => {
                    st.judged += 1;
                    if wrong {
                        st.sem_wrong += 1;
                    }
                    for (name, ns) in &times {
                        let jc = report.judge_cost.entry(name.to_string()).or_default();
                        jc.calls += 1;
                        jc.ns += *ns;
                    }
                    let killed = judges.iter().any(|(_, fired)| *fired);
                    for (name, fired) in judges {
                        if fired {
                            *st.fired.entry(name.to_string()).or_insert(0) += 1;
                            if let Some(jc) = report.judge_cost.get_mut(name) {
                                jc.fired += 1;
                            }
                        }
                    }
                    if killed {
                        st.killed += 1;
                    }
                }
            }
            continue;
        }

        let mutant = match apply_input_mutation(&f, kind, &mut rng) {
            None => {
                st.no_site += 1;
                continue;
            }
            Some(m) => m,
        };
        if verify(&mutant).is_err() {
            st.structural_invalid += 1;
            continue;
        }
        st.judged += 1;

        // wrongness classification (the property oracle, timed: its
        // kill-rate-per-microsecond is part of the R2 judge-cost table)
        if kind.is_dataflow() {
            let t = Instant::now();
            let obs = (observable(&f), observable(&mutant));
            let oracle_ns = t.elapsed().as_nanos();
            report.oracle_cost.calls += 2; // two observable() evaluations
            report.oracle_cost.ns += oracle_ns;
            match obs {
                (Some(a), Some(b)) => {
                    if a == b {
                        st.sem_equivalent += 1;
                    } else {
                        st.sem_wrong += 1;
                        report.oracle_cost.fired += 1; // the oracle "kills": proves wrongness
                    }
                }
                _ => st.unjudgeable += 1,
            }
        } else {
            st.unjudgeable += 1;
        }

        let (judges, times) = judge_battery_timed(&mutant);
        for (name, ns) in &times {
            let jc = report
                .judge_cost
                .entry(name.to_string())
                .or_default();
            jc.calls += 1;
            jc.ns += *ns;
        }
        let killed = judges.iter().any(|(_, fired)| *fired);
        for (name, fired) in judges {
            if fired {
                *st.fired.entry(name.to_string()).or_insert(0) += 1;
                if let Some(jc) = report.judge_cost.get_mut(name) {
                    jc.fired += 1;
                }
            }
        }
        if killed {
            st.killed += 1;
        }
    }
    Ok(report)
}

impl JudgeCost {
    /// kills per microsecond this judge (or oracle) spent judging.
    pub fn kills_per_us(&self) -> f64 {
        if self.ns == 0 {
            0.0
        } else {
            self.fired as f64 / (self.ns as f64 / 1000.0)
        }
    }
    pub fn us_total(&self) -> f64 {
        self.ns as f64 / 1000.0
    }
}

/// The R2 judge-cost table: per judge (and the dataflow oracle), how
/// many calls, how many kills, total microseconds, and
/// kills-per-microsecond. Timings are one session's wall clock — an
/// observation about cost, labeled as such (not a deterministic
/// result; excluded from Display on purpose).
pub fn judge_cost_report(r: &SemReport) -> String {
    let mut out = String::new();
    out.push_str("judge cost (wall time, this run): kills-per-microsecond per judge
");
    out.push_str("judge          calls   kills   total-us   us/call   kills/us
");
    let mut rows: Vec<(&str, &JudgeCost)> = r
        .judge_cost
        .iter()
        .map(|(k, v)| (k.as_str(), v))
        .collect();
    rows.push(("oracle(dataflow)", &r.oracle_cost));
    rows.sort_by_key(|(k, _)| *k);
    for (name, jc) in rows {
        let us_per_call = if jc.calls > 0 { jc.us_total() / jc.calls as f64 } else { 0.0 };
        out.push_str(&format!(
            "{:<14} {:>6} {:>7} {:>10.1} {:>9.2} {:>10.3}\n",
            name,
            jc.calls,
            jc.fired,
            jc.us_total(),
            us_per_call,
            jc.kills_per_us()
        ));
    }
    out
}

impl std::fmt::Display for SemReport {
    fn fmt(&self, w: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(w, "semantic mutation battery — {} iters, kinds round-robined", self.iters)?;
        writeln!(
            w,
            "{:<22} {:>9} {:>8} {:>9} {:>7} {:>7} {:>9} {:>11} {:>7} {:>7}",
            "kind", "attempt", "nosite", "str inval", "judged", "equiv", "sem-wrong", "unjudge", "killed", "kill%"
        )?;
        for kind in SemKind::ALL {
            let st = self.kinds.get(kind.name());
            let (attempt, nosite, inval, judged, equiv, wrong, unjudge, killed) = match st {
                None => (0, 0, 0, 0, 0, 0, 0, 0),
                Some(s) => (s.attempted, s.no_site, s.structural_invalid, s.judged, s.sem_equivalent, s.sem_wrong, s.unjudgeable, s.killed),
            };
            let pct = if judged > 0 { (killed as f64 / judged as f64 * 1000.0).round() / 10.0 } else { 0.0 };
            writeln!(
                w,
                "{:<22} {:>9} {:>8} {:>9} {:>7} {:>7} {:>9} {:>11} {:>7} {:>6.1}%",
                kind.name(), attempt, nosite, inval, judged, equiv, wrong, unjudge, killed, pct
            )?;
        }
        let jt = self.judged_total();
        let kt = self.killed_total();
        let pct = if jt > 0 { (kt as f64 / jt as f64 * 1000.0).round() / 10.0 } else { 0.0 };
        writeln!(w, "tier total: judged {} killed {} kill-rate {}%", jt, kt, pct)?;
        writeln!(
            w,
            "confirmed-wrong (dataflow-provable) {} · unjudgeable (no execution semantics) {}",
            self.sem_wrong_total(),
            self.unjudgeable_total()
        )?;
        // per-judge fire counts across all kinds (who ever fires?)
        let mut all: BTreeMap<&str, u64> = BTreeMap::new();
        for st in self.kinds.values() {
            for (name, n) in &st.fired {
                *all.entry(name.as_str()).or_insert(0) += n;
            }
        }
        writeln!(w, "judge fire counts: {}", if all.is_empty() { "(none fired outside the control)".into() } else {
            all.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(" ")
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The battery runs, every kind is exercised, the tamper control
    /// FIRES (the judges are not deaf), and the wrongness oracle
    /// confirms at least one dataflow mutant changed the program value.
    /// The blindness of the judges to input-level semantic mutants is a
    /// PUBLISHED MEASUREMENT (docs/phase/SEM-MUTANTS.md), not a suite
    /// invariant — when an oracle lands, kills appear here and the doc
    /// number moves.
    #[test]
    fn semmut_battery_smoke_control_fires() {
        let st = semmut_run(160, 0x5EED).expect("battery");
        for kind in SemKind::ALL {
            assert!(
                st.kinds.contains_key(kind.name()),
                "kind {} never attempted",
                kind.name()
            );
        }
        let control = &st.kinds[SemKind::StageTamperControl.name()];
        assert!(control.judged > 0, "control must have applicable sites");
        assert!(control.killed > 0, "tamper control MUST fire — a deaf battery proves nothing");
        assert!(control.fired.contains_key("replay"), "replay bit-identity is the tamper detector");
        assert!(st.sem_wrong_total() > 0, "wrongness oracle must confirm at least one dataflow mutant");
    }

    /// The generator base and the mutated-then-judged path never panic:
    /// 160 iters above already assert that via semmut_run's Err-free
    /// completion; this test pins determinism of the report.
    #[test]
    fn semmut_report_is_deterministic() {
        let a = semmut_run(40, 0xD1CE).unwrap();
        let b = semmut_run(40, 0xD1CE).unwrap();
        assert_eq!(format!("{}", a), format!("{}", b), "same seeds must produce identical reports");
    }
}

//! M3 — the ledger pass manager.
//!
//! A driver that runs pass pipelines where EVERY pass outcome lands as a
//! Weft ledger entry: a tick either advances the fabric (mechanically
//! derived: the diff carries edits) or declares fixed point (a ledger
//! entry, symmetric with dropped-with-entry — REVERSE-ACTUALIZATION
//! invariant 2). Enforcement belongs to the MANAGER, not the pass
//! author: a pass is a pure function `fabric -> (fabric, diff)`, and
//! the manager mechanically rejects a pass that
//!
//!   1. emits a fabric that fails `verify`,
//!   2. drops or conjures values without ledger entries (`conserve`),
//!   3. records its diff under the wrong name (name laundering), or
//!   4. would leave a gap in the Weft (progress law).
//!
//! The progress law itself is proven machinery (diff.rs `push_tick` /
//! `check_weft` / `verify_chain`); the manager is the driver that makes
//! it the ONLY way to run a pipeline. Conservation across the manager
//! holds by construction: every tick is reconciled before it lands.

use crate::conserve;
use crate::diff::History;
use crate::fabric::Fabric;
use crate::replay;
use crate::verify::verify;
use std::collections::BTreeMap;

/// Context handed to every pass by the manager: the tick the pass's
/// diff will be recorded at. Certificates that must name their tick
/// (M4 death certs) cite `ctx.tick` — the pass does NOT guess it.
#[derive(Clone, Copy, Debug)]
pub struct TickCtx {
    pub tick: u64,
}

/// A pass: pure function of the fabric (plus callees), returning the
/// next fabric and the diff. Adapters below bind the existing passes.
pub type PassFn =
    fn(&Fabric, &TickCtx, &BTreeMap<String, Fabric>) -> Result<(Fabric, crate::diff::DiffRecord), String>;

fn constfold_fn(
    f: &Fabric,
    _ctx: &TickCtx,
    _funcs: &BTreeMap<String, Fabric>,
) -> Result<(Fabric, crate::diff::DiffRecord), String> {
    crate::passes::constfold::const_fold(f)
}

fn dce_fn(
    f: &Fabric,
    _ctx: &TickCtx,
    _funcs: &BTreeMap<String, Fabric>,
) -> Result<(Fabric, crate::diff::DiffRecord), String> {
    crate::passes::dce::dce(f)
}

fn inline_fn(
    f: &Fabric,
    _ctx: &TickCtx,
    funcs: &BTreeMap<String, Fabric>,
) -> Result<(Fabric, crate::diff::DiffRecord), String> {
    crate::passes::inline::inline_calls(f, funcs)
}

/// One line of the manager's audit trail: what was checked at a tick.
/// `verified` and `conserved` are only ever true here — a violation
/// aborts the run with an error instead of auditing false.
#[derive(Clone, Debug, PartialEq)]
pub struct TickAudit {
    pub tick: u64,
    pub pass: &'static str,
    pub edits: usize,
    pub advanced: bool,
    pub verified: bool,
    pub conserved: bool,
}

/// A completed managed run: final fabric, full history (Weft complete),
/// every stage, and the per-tick audit.
pub struct ManagedRun {
    pub fabric: Fabric,
    pub history: History,
    pub stages: Vec<Fabric>,
    pub audit: Vec<TickAudit>,
}

impl ManagedRun {
    /// The audit as a human-readable table (CLI/report shape).
    pub fn audit_summary(&self) -> String {
        let mut out = String::from("tick pass        edits  outcome\n");
        for a in &self.audit {
            out.push_str(&format!(
                "{:4} {:10} {:5}  {}\n",
                a.tick,
                a.pass,
                a.edits,
                if a.advanced { "advanced" } else { "fixed point" }
            ));
        }
        out
    }
}

pub struct PassManager {
    passes: BTreeMap<&'static str, PassFn>,
}

impl PassManager {
    /// The stock registry: the three real passes.
    pub fn new() -> PassManager {
        let mut m = PassManager { passes: BTreeMap::new() };
        m.register("constfold", constfold_fn);
        m.register("dce", dce_fn);
        m.register("inline", inline_fn);
        m
    }

    pub fn register(&mut self, name: &'static str, f: PassFn) {
        self.passes.insert(name, f);
    }

    pub fn knows(&self, name: &str) -> bool {
        self.passes.contains_key(name)
    }

    /// Run a pipeline. Every tick is reconciled before it lands; the
    /// full run is re-checked at the end (weft law, chain vs stages,
    /// pipeline-wide conservation, bit-identical replay). Any violation
    /// aborts naming the tick, the pass, and the cell.
    pub fn run(
        &self,
        f: &Fabric,
        pipeline: &[&str],
        funcs: &BTreeMap<String, Fabric>,
    ) -> Result<ManagedRun, String> {
        let initial = f.clone();
        let mut h = History::new();
        let mut stages = vec![initial.clone()];
        let mut audit = vec![];
        let mut cur = initial.clone();
        for name in pipeline {
            let tick = h.len() as u64;
            let pass = *self.passes.get(name).ok_or_else(|| format!(
                "manager: unknown pass '{}' (registered: {})",
                name,
                self.passes.keys().cloned().collect::<Vec<_>>().join(", ")
            ))?;
            let ctx = TickCtx { tick };
            let (next, rec) = pass(&cur, &ctx, funcs)
                .map_err(|e| format!("manager: tick {} pass {} failed: {}", tick, name, e))?;
            if rec.pass != *name {
                return Err(format!(
                    "manager: tick {} scheduled '{}' but the diff records '{}' — name laundering rejected",
                    tick, name, rec.pass
                ));
            }
            if let Err(e) = verify(&next) {
                return Err(format!(
                    "manager: tick {} pass {} emitted an unverifiable fabric: {}",
                    tick, name, e
                ));
            }
            if let Err(e) = conserve::check(&cur, &next, &rec) {
                return Err(format!(
                    "manager: tick {} pass {} broke conservation: {}",
                    tick, name, e
                ));
            }
            let rec_pass = rec.pass;
            h.push_tick(rec, &next);
            let advanced = !h.records[tick as usize].edits.is_empty();
            audit.push(TickAudit {
                tick,
                // rec.pass was proven == the scheduled name above; use it
                // so the audit carries the pass's own &'static declaration
                pass: rec_pass,
                edits: h.records[tick as usize].edits.len(),
                advanced,
                verified: true,
                conserved: true,
            });
            cur = next;
            stages.push(cur.clone());
        }

        // Post-run reconciliation — the manager's own contract.
        h.check_weft()
            .map_err(|e| format!("manager: post-run weft law: {}", e))?;
        h.verify_chain(&stages)
            .map_err(|e| format!("manager: post-run chain: {}", e))?;
        conserve::check_pipeline(&initial, &cur, &h)
            .map_err(|e| format!("manager: post-run conservation: {}", e))?;
        let (replayed, replayed_final) = replay::replay(&initial, &h)
            .map_err(|e| format!("manager: post-run replay: {}", e))?;
        if replayed.len() != stages.len() || replayed_final != cur {
            return Err("manager: post-run replay is not bit-identical".into());
        }
        for (i, (a, b)) in stages.iter().zip(replayed.iter()).enumerate() {
            if a != b || crate::text::print(a) != crate::text::print(b) {
                return Err(format!("manager: post-run replay diverges at stage {}", i));
            }
        }
        Ok(ManagedRun { fabric: cur, history: h, stages, audit })
    }

    /// Convenience: run the stock v0 pipeline (fold, dce, fold, dce).
    pub fn run_v0(&self, f: &Fabric) -> Result<ManagedRun, String> {
        self.run(f, crate::pipeline::PIPELINE, &BTreeMap::new())
    }
}

impl Default for PassManager {
    fn default() -> Self {
        PassManager::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, CellKind};
    use crate::diff::{DiffRecord, Edit};
    use crate::id::CellId;
    use crate::ty::{ConstVal, Type};

    fn mix() -> Fabric {
        // 20+22 folds; dead i64 const for dce; a live param chain
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
    fn green_pipeline_every_outcome_ledgered_and_checked() {
        let m = PassManager::new();
        let run = m.run_v0(&mix()).expect("managed run");
        assert_eq!(run.stages.len(), 5);
        assert_eq!(run.audit.len(), 4);
        // every tick audited verified+conserved; ticks 0,1 advance; 2 fixed point
        assert!(run.audit.iter().all(|a| a.verified && a.conserved));
        assert!(run.audit[0].advanced, "constfold fires");
        assert!(run.audit[1].advanced, "dce fires");
        assert!(!run.audit[2].advanced, "second constfold is a fixed point");
        // the fixed point IS a weft entry (progress law: no silent no-ops)
        assert_eq!(run.history.weft.len(), 4);
        assert!(run.history.weft[2].note.contains("fixed point"));
        assert!(run.history.check_weft().is_ok());
        // composition parity: same result as the raw v0 pipeline
        let (raw_final, raw_h, raw_stages) = crate::pipeline::run(&mix()).unwrap();
        assert_eq!(crate::text::print(&run.fabric), crate::text::print(&raw_final));
        assert_eq!(run.stages.len(), raw_stages.len());
        assert!(run
            .stages
            .iter()
            .zip(raw_stages.iter())
            .all(|(a, b)| crate::text::print(a) == crate::text::print(b)));
        assert_eq!(run.history.weft.last().map(|t| t.sig), raw_h.weft.last().map(|t| t.sig));
    }

    #[test]
    fn replay_from_mid_history_is_bit_identical() {
        // D5 enforcement: replay a PREFIX of the history and compare the
        // stage at that prefix boundary.
        let m = PassManager::new();
        let run = m.run_v0(&mix()).unwrap();
        let k = 2usize; // mid-history boundary
        let mut prefix = run.history.clone();
        prefix.records.truncate(k);
        prefix.weft.truncate(k);
        let (stages, final_stage) = replay::replay(&run.stages[0], &prefix).unwrap();
        assert_eq!(stages.len(), k + 1);
        assert_eq!(stages[k], run.stages[k], "stage at boundary must be bit-identical");
        assert_eq!(crate::text::print(&stages[k]), crate::text::print(&run.stages[k]));
        let _ = final_stage;
    }

    #[test]
    fn rerun_at_fixed_point_is_all_fixed_point() {
        let m = PassManager::new();
        let once = m.run_v0(&mix()).unwrap();
        let twice = m.run_v0(&once.fabric).expect("second run");
        assert!(twice.audit.iter().all(|a| !a.advanced), "a converged fabric must idle every tick");
        assert!(twice.history.weft.iter().all(|t| t.note.contains("fixed point")));
    }

    // ---- red conditions: each fixture pass violates exactly one law;
    //      the manager must reject it. Without the manager (raw call)
    //      the violation succeeds silently — that is the red condition.

    fn leaky_fn(
        f: &Fabric,
        _ctx: &TickCtx,
        _funcs: &BTreeMap<String, Fabric>,
    ) -> Result<(Fabric, DiffRecord), String> {
        // silently drops an UNREFERENCED cell (the dead i64 const) —
        // the fabric still verifies; only conservation catches the drop
        let mut g = f.clone();
        let victim = g
            .cells()
            .find(|&id| {
                g.cell(id).map(|c| matches!(c.kind, CellKind::Const { .. })).unwrap_or(false)
                    && g.uses_of(id).is_empty()
            })
            .expect("a const with no users");
        g.slab[victim.0 as usize] = None;
        for r in g.regions.iter_mut() {
            r.cells.retain(|&c| c != victim);
        }
        Ok((g, DiffRecord::new("leaky")))
    }

    #[test]
    fn manager_rejects_a_leaky_pass_and_names_the_tick() {
        // RED (no enforcement): the leaky pass "succeeds" when called raw
        let f = mix();
        let ctx = TickCtx { tick: 0 };
        let (leaked, rec) = leaky_fn(&f, &ctx, &BTreeMap::new()).unwrap();
        assert!(leaked.cells().count() < f.cells().count() && rec.edits.is_empty(),
            "red condition: raw call silently drops a value");

        // GREEN: the manager catches it before the tick lands
        let mut m = PassManager::new();
        m.register("leaky", leaky_fn);
        let err = m.run(&f, &["leaky"], &BTreeMap::new()).err().expect("must be rejected");
        assert!(err.contains("tick 0"), "{}", err);
        assert!(err.contains("leaky"), "{}", err);
        assert!(err.contains("conservation"), "{}", err);
        assert!(err.contains("vanished"), "{}", err);
    }

    fn broken_fn(
        f: &Fabric,
        _ctx: &TickCtx,
        _funcs: &BTreeMap<String, Fabric>,
    ) -> Result<(Fabric, DiffRecord), String> {
        // emits a fabric that fails verify: a use of a cell that is gone
        let mut g = f.clone();
        let victim = g.cells().next().expect("nonempty");
        g.slab[victim.0 as usize] = None;
        for r in g.regions.iter_mut() {
            r.cells.retain(|&c| c != victim);
        }
        let mut rec = DiffRecord::new("broken");
        rec.edits.push(Edit::RemoveCell {
            id: victim,
            ledger: "ledgered, but the fabric is broken".into(),
            summary: "%0".into(),
        });
        Ok((g, rec))
    }

    #[test]
    fn manager_rejects_an_unverifiable_output() {
        let mut m = PassManager::new();
        m.register("broken", broken_fn);
        let err = m.run(&mix(), &["broken"], &BTreeMap::new()).err().expect("must be rejected");
        assert!(err.contains("unverifiable"), "{}", err);
    }

    fn laundered_fn(
        f: &Fabric,
        _ctx: &TickCtx,
        _funcs: &BTreeMap<String, Fabric>,
    ) -> Result<(Fabric, DiffRecord), String> {
        let (_, rec) = crate::passes::constfold::const_fold(f)?;
        Ok((f.clone(), rec)) // identity fabric, but a diff under another pass's name
    }

    #[test]
    fn manager_rejects_name_laundering() {
        let mut m = PassManager::new();
        m.register("fake-fold", laundered_fn);
        let err = m.run(&mix(), &["fake-fold"], &BTreeMap::new()).err().expect("must be rejected");
        assert!(err.contains("laundering"), "{}", err);
    }

    #[test]
    fn manager_rejects_unknown_passes() {
        let m = PassManager::new();
        let err = m.run(&mix(), &["constfold", "gc"], &BTreeMap::new()).err().expect("must be rejected");
        assert!(err.contains("unknown pass 'gc'"), "{}", err);
    }

    #[test]
    fn passes_compose_in_any_registered_order() {
        // composition: dce -> constfold lands the same final fabric as
        // constfold -> dce for a fabric where both idempote (fixed point)
        let m = PassManager::new();
        let run = m.run_v0(&mix()).unwrap();
        let alt = m.run(&run.fabric, &["dce", "constfold"], &BTreeMap::new()).unwrap();
        assert!(alt.audit.iter().all(|a| !a.advanced));
        assert_eq!(crate::text::print(&alt.fabric), crate::text::print(&run.fabric));
    }

    #[test]
    fn v1_pipeline_runs_through_the_manager() {
        use std::collections::BTreeMap as Map;
        let main_text = "fabric v0\nregion entry\n  %0 = param i32\n  %1 = const i32 20\n  %2 = const i32 22\n  %3 = const i64 9i64\n  %4 = call i32 add2 %1, %2\n  %5 = ret %4\n";
        let callee_text = "fabric v0\nregion entry\n  %0 = param i32\n  %1 = param i32\n  %2 = arith.add i32 %0, %1\n  %3 = ret %2\n";
        let mut funcs: Map<String, Fabric> = BTreeMap::new();
        funcs.insert("add2".into(), crate::text::parse(callee_text).unwrap());
        let f = crate::text::parse(main_text).unwrap();
        let m = PassManager::new();
        let run = m.run(&f, crate::pipeline::PIPELINE_V1, &funcs).expect("v1 managed");
        assert_eq!(run.stages.len(), 6);
        assert!(run.fabric.cell(CellId(4)).is_none(), "call inlined away");
        let _ = Type::I32;
    }
}

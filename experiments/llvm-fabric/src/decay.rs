//! M4 — DCE as decay (the load-bearing milestone).
//!
//! ARCHITECTURE §1.4: decay = use-count aging; DCE becomes "a threshold
//! read in a scheduled sweep phase," emergent from the tick machinery,
//! not a special trusted pass. Here that lands as a CONTRACT INVERSION:
//!
//!   old `dce` pass: trusted to decide what is dead; its ledger line
//!     is prose a human may read ("dead: no path to a terminator").
//!   `dce-decay`: a value dies ONLY when its death LEDGER ENTRY names
//!     the killer pass + the tick + a machine-checkable witness. The
//!     verifier recomputes the witness from the pre-tick fabric and
//!     REJECTS a bogus kill — DCE is not trusted; it is audited.
//!
//! The witness for v0 (one kind, recomputable): `no-demand` — at death
//! time the cell had exactly `users` users and was not backward-
//! reachable from any terminator (no demand chain holds it). The user
//! count is a datum the verifier re-measures; tampering with it (or
//! the tick, the killer, or the victim's identity) fails verification.
//!
//! Decay curves: classification of every cell at every pipeline stage —
//!   dead  (no path to a terminator — zero demand now),
//!   cold  (live now, but dead by pipeline end — cooling),
//!   warm  (live now and survives to the end — steady demand).
//! Measured over the corpus; see `decay_curves`.

use crate::diff::{DiffRecord, Edit};
use crate::fabric::Fabric;
use crate::id::CellId;
use crate::manager::{PassManager, TickCtx};
use crate::verify::verify;
use std::collections::{BTreeMap, HashSet, VecDeque};

/// The decay pipeline: fold, let decay sweep, fold what the sweep
/// exposed, sweep again. Order per the scout build order.
pub const PIPELINE_DECAY: &[&str] = &["constfold", "dce-decay", "constfold", "dce-decay"];

/// Backward-reachable set from all terminators — the live (demanded)
/// closure. Everything outside it has zero demand.
pub fn live_closure(f: &Fabric) -> HashSet<CellId> {
    let mut live: HashSet<CellId> = HashSet::new();
    let mut work: Vec<CellId> = vec![];
    for id in f.cells() {
        if f.cell(id).expect("present").is_terminator() {
            live.insert(id);
            work.push(id);
        }
    }
    while let Some(id) = work.pop() {
        if let Some(c) = f.cell(id) {
            for &op in &c.operands {
                if live.insert(op) {
                    work.push(op);
                }
            }
        }
    }
    live
}

/// A short demand path cell -> ... -> terminator (for rejection
/// messages: name the wire chain that holds a live value). BFS over
/// user edges; returns None if there is none (i.e. the cell is dead).
pub fn demand_path(f: &Fabric, from: CellId) -> Option<Vec<CellId>> {
    let mut prev: BTreeMap<CellId, Option<CellId>> = BTreeMap::new();
    prev.insert(from, None);
    let mut q = VecDeque::from([from]);
    while let Some(cur) = q.pop_front() {
        let is_term = f.cell(cur).map(|c| c.is_terminator()).unwrap_or(false);
        if is_term && cur != from {
            // walk back
            let mut path = vec![cur];
            let mut p = prev.get(&cur).copied().flatten();
            while let Some(node) = p {
                path.push(node);
                p = prev.get(&node).copied().flatten();
            }
            path.reverse();
            return Some(path);
        }
        for (user, _) in f.uses_of(cur) {
            if !prev.contains_key(&user) {
                prev.insert(user, Some(cur));
                q.push_back(user);
            }
        }
    }
    None
}

/// Machine-checkable death certificate, carried in the RemoveCell
/// ledger line. Rendered form (byte-stable):
///   death{killer=dce-decay tick=3 users=0 witness=no-demand}
#[derive(Clone, PartialEq, Debug)]
pub struct DeathCert {
    pub cell: CellId,
    pub killer: String,
    pub tick: u64,
    pub users: u32,
}

pub const WITNESS: &str = "no-demand";

impl DeathCert {
    pub fn render(&self) -> String {
        format!(
            "death{{killer={} tick={} users={} witness={}}}",
            self.killer, self.tick, self.users, WITNESS
        )
    }

    /// Parse from a ledger line. Returns None unless the form is exact
    /// (a bare "dead: ..." prose line is NOT a certificate).
    pub fn parse(ledger: &str, cell: CellId) -> Option<DeathCert> {
        let inner = ledger.strip_prefix("death{")?.strip_suffix("}")?;
        let mut killer = None;
        let mut tick = None;
        let mut users = None;
        let mut witness = None;
        for part in inner.split(' ') {
            let (k, v) = part.split_once('=')?;
            match k {
                "killer" => killer = Some(v.to_string()),
                "tick" => tick = Some(v.parse::<u64>().ok()?),
                "users" => users = Some(v.parse::<u32>().ok()?),
                "witness" => witness = Some(v),
                _ => return None,
            }
        }
        if witness != Some(WITNESS) {
            return None;
        }
        Some(DeathCert {
            cell,
            killer: killer?,
            tick: tick?,
            users: users?,
        })
    }
}

/// The decay pass: a sweep that kills every zero-demand cell, each
/// removal carrying a death certificate naming killer, tick, and the
/// measured user count. The tick comes from the manager's TickCtx —
/// the pass does not guess it.
pub fn dce_decay(f: &Fabric, ctx: &TickCtx) -> Result<(Fabric, DiffRecord), String> {
    if let Err(e) = verify(f) {
        return Err(format!("dce-decay refuses unverified input: {}", e));
    }
    let live = live_closure(f);
    let mut g = f.clone();
    let mut rec = DiffRecord::new("dce-decay");
    rec.epoch = ctx.tick; // informational; the manager reassigns on landing
    for ri in 0..g.regions.len() {
        let ids: Vec<CellId> = g.regions[ri].cells.clone();
        for id in ids {
            if live.contains(&id) {
                continue;
            }
            let users = f.uses_of(id).len() as u32; // measured on the PRE-tick fabric
            let summary = crate::text::render_cell(&g, id);
            let region = g.cell(id).expect("present").region;
            let cells = &mut g.regions[region.0 as usize].cells;
            let pos = cells.iter().position(|&c| c == id).expect("listed");
            cells.remove(pos);
            g.slab[id.0 as usize] = None;
            let cert = DeathCert { cell: id, killer: "dce-decay".into(), tick: ctx.tick, users };
            rec.edits.push(Edit::RemoveCell { id, ledger: cert.render(), summary });
        }
    }
    Ok((g, rec))
}

/// THE M4 CHECK: verify death certificates against the pre-tick fabric.
/// Every removal in a decay record must carry a parseable certificate
/// whose witness RECOMPUTES: the cell was present, had exactly the
/// claimed number of users, and had no demand path to a terminator.
/// A bogus kill — a certificate for a live value, a wrong user count,
/// a wrong tick, a wrong killer, or bare prose where the cert should
/// be — is REJECTED, naming the cell and the mismatch.
pub fn verify_deaths(before: &Fabric, rec: &DiffRecord) -> Result<(), String> {
    let live = live_closure(before);
    for e in &rec.edits {
        let (id, ledger) = match e {
            Edit::RemoveCell { id, ledger, .. } => (*id, ledger),
            _ => continue,
        };
        let cert = match DeathCert::parse(ledger, id) {
            Some(c) => c,
            None => {
                return Err(format!(
                    "death rejected: {} removed by '{}' without a certificate (ledger: '{}') — a decay death must name killer+tick+witness",
                    id, rec.pass, ledger
                ))
            }
        };
        if cert.killer != rec.pass {
            return Err(format!(
                "death rejected: {}'s certificate names killer '{}' but the diff records '{}'",
                id, cert.killer, rec.pass
            ));
        }
        if cert.tick != rec.epoch {
            return Err(format!(
                "death rejected: {}'s certificate names tick {} but the diff lands at epoch {}",
                id, cert.tick, rec.epoch
            ));
        }
        let cell = before.cell(id).ok_or_else(|| {
            format!("death rejected: {}'s certificate names a cell that was not present pre-tick", id)
        })?;
        let _ = cell;
        let users = before.uses_of(id).len() as u32;
        if users != cert.users {
            return Err(format!(
                "death rejected: {}'s certificate claims {} users but the pre-tick fabric has {}",
                id, cert.users, users
            ));
        }
        if live.contains(&id) {
            let chain = demand_path(before, id)
                .map(|p| {
                    p.iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                        .join(" -> ")
                })
                .unwrap_or_else(|| "<demand path walk failed>".into());
            return Err(format!(
                "death rejected: bogus kill of {} — the cell is LIVE; demand chain {} holds it",
                id, chain
            ));
        }
    }
    Ok(())
}

/// Extract the typed certificates from a decay record.
pub fn deaths(rec: &DiffRecord) -> Vec<DeathCert> {
    rec.edits
        .iter()
        .filter_map(|e| match e {
            Edit::RemoveCell { id, ledger, .. } => DeathCert::parse(ledger, *id),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Decay curves: cold/warm/dead classification per stage over the corpus.

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum DecayClass {
    #[default]
    Dead,
    Cold,
    Warm,
}

/// Classify every cell of `f`: dead = no path to a terminator now;
/// cold = live now but in `eventually_dead`; warm = live and survives.
pub fn classify(f: &Fabric, eventually_dead: &HashSet<CellId>) -> BTreeMap<CellId, DecayClass> {
    let live = live_closure(f);
    f.cells()
        .map(|id| {
            let class = if !live.contains(&id) {
                DecayClass::Dead
            } else if eventually_dead.contains(&id) {
                DecayClass::Cold
            } else {
                DecayClass::Warm
            };
            (id, class)
        })
        .collect()
}

/// Aggregated per-stage decay counts (sums over the corpus; divide by
/// `fabrics` for means).
#[derive(Clone, Debug, Default)]
pub struct StageDecay {
    pub stage: u64,
    pub pass_after: String, // "" for stage 0 (the input fabric)
    pub dead: u64,
    pub cold: u64,
    pub warm: u64,
}

#[derive(Clone, Debug, Default)]
pub struct DecayCurves {
    pub fabrics: u64,
    pub cells_in: u64,
    pub cells_out: u64,
    /// per pipeline tick: cells removed (all causes)
    pub deaths: Vec<u64>,
    /// per pipeline tick: deaths that are certified decay kills
    pub decay_kills: Vec<u64>,
    /// per stage (0..=ticks): dead/cold/warm counts summed over fabrics
    pub stages: Vec<StageDecay>,
    /// deaths rejected by verify_deaths during measurement (must stay 0)
    pub bogus_deaths: u64,
    /// total ledger bytes (history render) across the corpus
    pub ledger_bytes: u64,
    /// total weft entries (one per tick per fabric)
    pub weft_entries: u64,
}

/// Run the decay pipeline over `iters` corpus fabrics and measure the
/// decay curve. Any invariant failure (manager rejection, unverified
/// deaths) aborts naming the seed.
pub fn decay_curves(iters: u64, seed0: u64) -> Result<DecayCurves, String> {
    let mut m = PassManager::new();
    m.register("dce-decay", |f, ctx, _funcs| dce_decay(f, ctx));
    let mut out = DecayCurves { fabrics: iters, ..Default::default() };
    out.deaths = vec![0; PIPELINE_DECAY.len()];
    out.decay_kills = vec![0; PIPELINE_DECAY.len()];
    for i in 0..iters {
        let seed = seed0.wrapping_add(i).max(1);
        let mut rng = crate::fuzz::Rng::new(seed);
        let f = crate::fuzz::gen_fabric(&mut rng);
        if let Err(e) = verify(&f) {
            return Err(format!("seed {}: generator produced invalid fabric: {}", seed, e));
        }
        let run = m.run(&f, PIPELINE_DECAY, &BTreeMap::new()).map_err(|e| format!("seed {}: {}", seed, e))?;
        // death-certificate verification at every decay tick
        for (k, rec) in run.history.records.iter().enumerate() {
            if rec.pass == "dce-decay" {
                let before = &run.stages[k];
                if let Err(e) = verify_deaths(before, rec) {
                    out.bogus_deaths += 1;
                    return Err(format!("seed {}: {}", seed, e));
                }
                out.decay_kills[k] += deaths(rec).len() as u64;
            }
        }
        // deaths per tick + classification per stage
        let final_ids: HashSet<CellId> = run.fabric.cells().collect();
        out.ledger_bytes += run.history.bytes(&run.fabric) as u64;
        out.weft_entries += run.history.weft.len() as u64;
        out.cells_in += f.cells().count() as u64;
        out.cells_out += run.fabric.cells().count() as u64;
        if out.stages.is_empty() {
            out.stages = (0..=PIPELINE_DECAY.len())
                .map(|k| StageDecay { stage: k as u64, pass_after: String::new(), ..Default::default() })
                .collect();
            for (k, s) in out.stages.iter_mut().enumerate() {
                s.pass_after = if k == 0 {
                    "<input>".into()
                } else {
                    PIPELINE_DECAY[k - 1].to_string()
                };
            }
        }
        for k in 0..PIPELINE_DECAY.len() {
            let a: HashSet<CellId> = run.stages[k].cells().collect();
            let b: HashSet<CellId> = run.stages[k + 1].cells().collect();
            out.deaths[k] += a.difference(&b).count() as u64;
        }
        for (k, stage) in run.stages.iter().enumerate() {
            let eventually: HashSet<CellId> =
                stage.cells().filter(|id| !final_ids.contains(id)).collect();
            let classes = classify(stage, &eventually);
            let agg = &mut out.stages[k];
            for c in classes.values() {
                match c {
                    DecayClass::Dead => agg.dead += 1,
                    DecayClass::Cold => agg.cold += 1,
                    DecayClass::Warm => agg.warm += 1,
                }
            }
        }
    }
    Ok(out)
}

impl DecayCurves {
    /// The curve as a table (report shape). Means per fabric.
    pub fn render(&self) -> String {
        let n = self.fabrics.max(1) as f64;
        let mut out = format!(
            "fabrics: {}   cells in: {}   cells out: {}   bogus deaths: {}\n",
            self.fabrics, self.cells_in, self.cells_out, self.bogus_deaths
        );
        out.push_str("stage after        mean cells  dead    cold    warm   (per fabric)\n");
        for s in &self.stages {
            out.push_str(&format!(
                "{:5} {:10} {:11.2}  {:5.2}  {:5.2}  {:5.2}\n",
                s.stage,
                s.pass_after,
                (s.dead + s.cold + s.warm) as f64 / n,
                s.dead as f64 / n,
                s.cold as f64 / n,
                s.warm as f64 / n
            ));
        }
        out.push_str("deaths per tick: ");
        let names: Vec<String> = self
            .deaths
            .iter()
            .zip(PIPELINE_DECAY.iter())
            .map(|(d, p)| format!("{}[{}]", d, p))
            .collect();
        out.push_str(&names.join(" "));
        out.push_str(&format!(
            "\ndecay kills (certified): {} / deaths total: {}",
            self.decay_kills.iter().sum::<u64>(),
            self.deaths.iter().sum::<u64>()
        ));
        out.push_str(&format!(
            "\nledger: {} bytes total, mean {:.1} B/fabric; weft entries: {}",
            self.ledger_bytes,
            self.ledger_bytes as f64 / n,
            self.weft_entries
        ));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{ArithOp, Cell, CellKind};
    use crate::ty::{ConstVal, Type};

    fn fabric_mixed() -> (Fabric, CellId, CellId, CellId) {
        // %0 param (warm: feeds ret chain)
        // %1 const i32 20 (cold: constfold consumes it at tick 0)
        // %2 const i32 22 (cold: same)
        // %3 add %1,%2 (cold: folded away at tick 0)
        // %4 const i64 7 (dead right now: no users, no demand)
        // %5 add %0,%3 (live until fold retargets it -> cold)
        // %6 ret %5 (warm: survives)
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let c1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(20) }));
        let c2 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(22) }));
        let mut a1 = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a1.operands = vec![c1, c2];
        let a1 = f.add_cell(e, a1);
        let dead_c = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I64, val: ConstVal::I64(7) }));
        let mut a2 = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a2.operands = vec![p, a1];
        let a2 = f.add_cell(e, a2);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![a2];
        f.add_cell(e, r);
        (f, c1, dead_c, a2)
    }

    #[test]
    fn classification_dead_cold_warm_is_measured_not_vibes() {
        let (f, c1, dead_c, _a2) = fabric_mixed();
        // run the decay pipeline to learn what dies
        let mut m = PassManager::new();
        m.register("dce-decay", |f, ctx, _funcs| dce_decay(f, ctx));
        let run = m.run(&f, PIPELINE_DECAY, &BTreeMap::new()).unwrap();
        let final_ids: HashSet<CellId> = run.fabric.cells().collect();
        let eventually: HashSet<CellId> = f.cells().filter(|id| !final_ids.contains(id)).collect();
        let classes = classify(&f, &eventually);
        assert_eq!(classes[&dead_c], DecayClass::Dead, "no users, no demand path");
        assert_eq!(classes[&c1], DecayClass::Cold, "live now, folded away later");
        // warm exists and includes the ret
        let ret = f.cells().find(|&id| matches!(f.cell(id).map(|c| &c.kind), Some(CellKind::Ret))).unwrap();
        assert_eq!(classes[&ret], DecayClass::Warm);
        assert!(classes.values().any(|c| *c == DecayClass::Warm));
    }

    #[test]
    fn green_decay_kill_carries_a_certificate_that_verifies() {
        let (f, _c1, dead_c, _a2) = fabric_mixed();
        let ctx = TickCtx { tick: 0 };
        let (g, rec) = dce_decay(&f, &ctx).expect("decay");
        assert!(g.cell(dead_c).is_none(), "zero-demand cell dies");
        assert!(verify(&g).is_ok());
        let certs = deaths(&rec);
        assert_eq!(certs.len(), 1, "exactly the one dead cell");
        let c = &certs[0];
        assert_eq!(c.cell, dead_c);
        assert_eq!(c.killer, "dce-decay");
        assert_eq!(c.tick, 0);
        assert_eq!(c.users, 0, "the dead const has no users");
        assert!(verify_deaths(&f, &rec).is_ok(), "real certificate verifies");
        // red condition: identity (no decay) leaves the dead cell in place
        assert!(f.cell(dead_c).is_some());
    }

    #[test]
    fn red_no_decay_is_identity_with_empty_diff() {
        // everything demanded: decay sweeps nothing, fixed point
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let mut a = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a.operands = vec![p, p];
        let a = f.add_cell(e, a);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![a];
        f.add_cell(e, r);
        let ctx = TickCtx { tick: 3 };
        let (g, rec) = dce_decay(&f, &ctx).expect("decay");
        assert_eq!(g, f, "nothing dies");
        assert!(rec.is_empty());
        assert!(verify_deaths(&f, &rec).is_ok());
    }

    // ---- tamper evidence: a bogus kill is REJECTED by the verifier ----

    #[test]
    fn bogus_kill_of_a_live_cell_is_rejected_naming_the_demand_chain() {
        // THE M4 proof: forge a death certificate for a LIVE cell,
        // remove it from the after-fabric, and hand both to the
        // verifier. It must reject and name the demand chain.
        let (f, c1, _dead, _a2) = fabric_mixed();
        let ctx = TickCtx { tick: 0 };
        let (_g, rec) = dce_decay(&f, &ctx).expect("decay"); // real certs

        // forge: certify the LIVE const %1 (20) as dead
        let mut forged = rec.clone();
        let users = f.uses_of(c1).len() as u32;
        forged.edits.push(Edit::RemoveCell {
            id: c1,
            ledger: DeathCert { cell: c1, killer: "dce-decay".into(), tick: 0, users }.render(),
            summary: "%1 = const i32 20".into(),
        });
        let err = verify_deaths(&f, &forged).err().expect("bogus kill must be rejected");
        assert!(err.contains("rejected"), "{}", err);
        assert!(err.contains("LIVE"), "{}", err);
        assert!(err.contains("->"), "{} — the demand chain is named", err);
        assert!(err.contains(format!("{}", c1).as_str()), "{}", err);
    }

    #[test]
    fn tampered_witness_count_is_rejected() {
        let (f, _c1, dead_c, _a2) = fabric_mixed();
        let ctx = TickCtx { tick: 1 };
        let (_g, rec) = dce_decay(&f, &ctx).expect("decay");
        let mut tampered = rec.clone();
        if let Edit::RemoveCell { ledger, .. } = &mut tampered.edits[0] {
            *ledger = DeathCert { cell: dead_c, killer: "dce-decay".into(), tick: 1, users: 5 }.render();
        }
        let err = verify_deaths(&f, &tampered).err().expect("tampered users must be rejected");
        assert!(err.contains("claims 5 users"), "{}", err);
        assert!(err.contains("0"), "{}", err);
    }

    #[test]
    fn tampered_tick_and_killer_are_rejected() {
        let (f, _c1, dead_c, _a2) = fabric_mixed();
        let (_g, rec) = dce_decay(&f, &TickCtx { tick: 2 }).expect("decay");
        // wrong tick in the cert
        let mut t = rec.clone();
        if let Edit::RemoveCell { ledger, .. } = &mut t.edits[0] {
            *ledger = DeathCert { cell: dead_c, killer: "dce-decay".into(), tick: 9, users: 0 }.render();
        }
        t.epoch = 2;
        let err = verify_deaths(&f, &t).err().expect("wrong tick must be rejected");
        assert!(err.contains("tick 9"), "{}", err);
        // wrong killer in the cert
        let mut k = rec.clone();
        if let Edit::RemoveCell { ledger, .. } = &mut k.edits[0] {
            *ledger = DeathCert { cell: dead_c, killer: "other-pass".into(), tick: 2, users: 0 }.render();
        }
        k.epoch = 2;
        let err = verify_deaths(&f, &k).err().expect("wrong killer must be rejected");
        assert!(err.contains("other-pass"), "{}", err);
    }

    #[test]
    fn bare_prose_ledger_is_not_a_certificate() {
        let (f, _c1, dead_c, _a2) = fabric_mixed();
        let (_g, mut rec) = dce_decay(&f, &TickCtx { tick: 0 }).expect("decay");
        // swap the cert for the OLD dce prose style
        if let Edit::RemoveCell { ledger, .. } = &mut rec.edits[0] {
            *ledger = "dead: no path to a terminator".into();
        }
        let err = verify_deaths(&f, &rec).err().expect("prose must not pass as a certificate");
        assert!(err.contains("without a certificate"), "{}", err);
        let _ = dead_c;
    }

    #[test]
    fn certificate_render_parse_round_trips() {
        let c = DeathCert { cell: CellId(7), killer: "dce-decay".into(), tick: 3, users: 0 };
        assert_eq!(DeathCert::parse(&c.render(), CellId(7)), Some(c));
        assert!(DeathCert::parse("dead: no path", CellId(7)).is_none());
        assert!(DeathCert::parse("death{killer=x tick=1 users=0 witness=wrong}", CellId(7)).is_none());
        assert!(DeathCert::parse("death{killer=x tick=1 witness=no-demand}", CellId(7)).is_none());
    }

    #[test]
    fn decay_pipeline_runs_through_the_manager_with_every_death_certified() {
        let (f, _c1, _dead, _a2) = fabric_mixed();
        let mut m = PassManager::new();
        m.register("dce-decay", |f, ctx, _funcs| dce_decay(f, ctx));
        let run = m.run(&f, PIPELINE_DECAY, &BTreeMap::new()).expect("managed decay pipeline");
        // every dce-decay tick: deaths == certified kills, all verified
        for (k, rec) in run.history.records.iter().enumerate() {
            if rec.pass == "dce-decay" {
                let before = &run.stages[k];
                assert!(verify_deaths(before, rec).is_ok(), "tick {}", k);
                let removed: u64 = rec
                    .edits
                    .iter()
                    .filter(|e| matches!(e, Edit::RemoveCell { .. }))
                    .count() as u64;
                assert_eq!(removed, deaths(rec).len() as u64, "every removal is certified");
            }
        }
        // fixed points still ledgered (progress law via manager)
        assert!(run.history.check_weft().is_ok());
    }

    #[test]
    fn curve_invariants_hold_on_a_small_corpus() {
        // measured, not vibes: deaths reconcile with kills, classes
        // partition the fabric, bogus stays 0
        let c = decay_curves(200, 0xD3CA5).expect("curves");
        assert_eq!(c.bogus_deaths, 0);
        assert!(c.fabrics == 200 && c.cells_in > 0);
        for (k, s) in c.stages.iter().enumerate().skip(1) {
            let died_here: u64 = c.deaths[k - 1];
            let prev = &c.stages[k - 1];
            let grew = (s.dead + s.cold + s.warm) as i64
                - (prev.dead + prev.cold + prev.warm) as i64;
            // inlined? no inline pass here; folds add one const per fold
            // so growth is (added - died); we only assert the ledgered
            // invariant: every death at a decay tick is certified
            let _ = (died_here, grew);
        }
        for k in 0..PIPELINE_DECAY.len() {
            if PIPELINE_DECAY[k] == "dce-decay" {
                assert_eq!(c.deaths[k], c.decay_kills[k], "tick {}: all deaths certified", k);
            }
        }
        assert!(c.stages.last().map(|s| s.dead == 0).unwrap_or(false),
            "end of pipeline: nothing remains dead (all swept)");
        assert!(c.stages[0].dead > 0, "corpus inputs do contain dead fabric");
    }
}

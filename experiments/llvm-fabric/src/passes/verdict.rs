//! the verdict vocabulary, graduated from bin/region-spike's
//! PassCounts: one judge for "did this pass application keep its
//! promises" — verify on the output, the property oracle
//! (region::interp) before/after, replay bit-identity of the record.
//!
//! scope (honest): judge measures ONE already-produced
//! (before, after, record) triple; it does not run the pass. `sites`
//! is the pass-agnostic count of edits in the record — the per-pass
//! candidate counts (branches seen, dead regions, call sites) live in
//! each pass's Stats struct, not here.

use crate::diff::{DiffRecord, History};
use crate::fabric::Fabric;
use std::collections::BTreeMap;

/// the spike's interp budget, fixed so verdicts are comparable.
pub const INTERP_BUDGET: usize = 100_000;

/// region-spike's PassCounts, per single pass application.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PassVerdict {
    pub attempted: usize,
    pub sites: usize,
    pub verify_green: usize,
    pub verify_red: usize,
    pub interp_preserved: usize,
    pub interp_changed: usize,
    pub unjudgeable: usize,
    pub replay_identical: usize,
    pub replay_diverged: usize,
}

impl PassVerdict {
    /// clean = both loud failure classes are zero: the oracle saw no
    /// answer change and replay reproduced the fabric bit-identically.
    pub fn is_clean(&self) -> bool {
        self.interp_changed == 0 && self.replay_diverged == 0
    }
}

/// Judge one pass application. All three legs run regardless of each
/// other's outcome — this is a measurement, not a gate. Semantics
/// mirror the spike: interp budget 100_000; a changed answer OR a
/// decidability change counts as interp_changed (loudly a bug); a
/// replay error counts as diverged.
pub fn judge(
    f_before: &Fabric,
    f_after: &Fabric,
    rec: &DiffRecord,
    funcs: &BTreeMap<String, Fabric>,
) -> PassVerdict {
    let mut v = PassVerdict { attempted: 1, sites: rec.edits.len(), ..Default::default() };
    match crate::verify::verify(f_after) {
        Ok(()) => v.verify_green += 1,
        Err(_) => v.verify_red += 1,
    }
    let before = crate::region::interp(f_before, funcs, INTERP_BUDGET);
    let after = crate::region::interp(f_after, funcs, INTERP_BUDGET);
    match (before, after) {
        (Some(p), Some(q)) if p == q => v.interp_preserved += 1,
        (None, None) => v.unjudgeable += 1,
        _ => v.interp_changed += 1,
    }
    let mut h = History::new();
    h.push(rec.clone());
    match crate::replay::replay(f_before, &h) {
        Ok((_, final_r)) if final_r == *f_after => v.replay_identical += 1,
        _ => v.replay_diverged += 1,
    }
    v
}

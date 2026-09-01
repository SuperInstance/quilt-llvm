//! region-DCE, graduated from the R2 spike (region.rs pass B) into the
//! maintained pass set: every region unreachable from entry is removed
//! — cells leave with ledgered RemoveCell edits, the region via
//! RegionRemoved (ids compact) — with phi-join maintenance for live
//! phis that joined on the removed regions (the removal dce.rs
//! deferred).
//!
//! thin wrapper: the transform lives in region::region_dce; this
//! module only fixes the pass name and the (fabric, diff) shape the
//! pipeline expects. pass_stats exposes the RegionDceStats variant.

use crate::diff::DiffRecord;
use crate::fabric::Fabric;
use crate::region::RegionDceStats;

pub const PASS_NAME: &str = "region-dce";

/// pipeline shape: stats dropped.
pub fn pass(f: &Fabric) -> Result<(Fabric, DiffRecord), String> {
    let (g, rec, _st) = pass_stats(f)?;
    Ok((g, rec))
}

/// measurement shape: the RegionDceStats ride along.
pub fn pass_stats(f: &Fabric) -> Result<(Fabric, DiffRecord, RegionDceStats), String> {
    crate::region::region_dce(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::verify;
    use std::collections::BTreeMap;

    /// entry -> live; 'dead' is unreachable, referenced by live's phi
    /// (region.rs's with_unreachable fixture). The answer is 7.
    fn with_unreachable() -> Fabric {
        let text = "fabric v0\n\
region entry\n\
  %0 = const i32 7\n\
  %1 = jump live\n\
region dead\n\
  %2 = const i32 99\n\
  %3 = jump live\n\
region live\n\
  %4 = phi [entry: %0] [dead: %2]\n\
  %5 = ret %4\n";
        crate::text::parse(text).expect("unreachable fabric parses")
    }

    #[test]
    fn green_dce_verifies_conserves_and_judges_clean() {
        let f = with_unreachable();
        assert!(verify(&f).is_ok(), "test fabric must verify first");
        let (g, rec) = pass(&f).expect("region-dce");
        assert_ne!(g, f, "red without the pass: identity would fail below");
        assert_eq!(g.regions.len(), 2, "entry + live remain");
        assert!(verify(&g).is_ok(), "verify green after the removal");
        assert!(
            crate::conserve::population_audit(&f, &g, &rec).is_ok(),
            "population audit holds"
        );
        let v = crate::passes::verdict::judge(&f, &g, &rec, &BTreeMap::new());
        assert!(v.is_clean(), "verdict must be clean: {:?}", v);
        assert_eq!(v.interp_preserved, 1, "answer 7 preserved");
        assert_eq!(v.replay_identical, 1);
        // the stats leg saw the removal
        let (_g2, _r2, st) = pass_stats(&f).expect("stats");
        assert_eq!(st.regions_removed, 1);
        assert_eq!(st.cells_removed, 2);
    }
}

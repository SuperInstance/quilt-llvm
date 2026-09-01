//! CFG-graft inline, graduated from the R2 spike (region.rs pass C)
//! into the maintained pass set: a call to an eligible callee (acyclic
//! entry, uniform typed rets, entry-confined uses) is replaced by a
//! graft of the callee's whole CFG — continuation region, region_map,
//! Ret -> Jump(K), return phi in K (the multi-region inlining
//! passes/inline.rs defers to exactly this vocabulary).
//!
//! thin wrapper: the transform lives in region::cfg_graft_inline; this
//! module only fixes the pass name and the (fabric, diff) shape the
//! pipeline expects. pass_stats exposes the InlineStats variant.

use crate::diff::DiffRecord;
use crate::fabric::Fabric;
use crate::region::InlineStats;
use std::collections::BTreeMap;

pub const PASS_NAME: &str = "cfg-inline";

/// pipeline shape: stats dropped.
pub fn pass(
    f: &Fabric,
    funcs: &BTreeMap<String, Fabric>,
) -> Result<(Fabric, DiffRecord), String> {
    let (g, rec, _st) = pass_stats(f, funcs)?;
    Ok((g, rec))
}

/// measurement shape: the InlineStats ride along.
pub fn pass_stats(
    f: &Fabric,
    funcs: &BTreeMap<String, Fabric>,
) -> Result<(Fabric, DiffRecord, InlineStats), String> {
    crate::region::cfg_graft_inline(f, funcs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::verify;

    /// main calls add2 with const args (pipeline.rs v1_tests' prog()):
    /// add2(20, 22) = 42, judgeable by interp on both sides.
    fn prog() -> (Fabric, BTreeMap<String, Fabric>) {
        let main_text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = const i32 20\n\
  %2 = const i32 22\n\
  %3 = const i64 9i64\n\
  %4 = call i32 add2 %1, %2\n\
  %5 = ret %4\n";
        let callee_text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = param i32\n\
  %2 = arith.add i32 %0, %1\n\
  %3 = ret %2\n";
        let mut funcs = BTreeMap::new();
        funcs.insert("add2".to_string(), crate::text::parse(callee_text).unwrap());
        (crate::text::parse(main_text).unwrap(), funcs)
    }

    #[test]
    fn green_inline_verifies_conserves_and_judges_clean() {
        let (f, funcs) = prog();
        assert!(verify(&f).is_ok(), "test fabric must verify first");
        let (g, rec) = pass(&f, &funcs).expect("cfg-inline");
        assert_ne!(g, f, "red without the pass: identity would fail below");
        assert!(verify(&g).is_ok(), "verify green after the graft");
        assert!(
            crate::conserve::population_audit(&f, &g, &rec).is_ok(),
            "population audit holds"
        );
        // the call is gone; add2(20, 22) = 42 before and after
        assert!(g.cells().all(|id| !matches!(
            g.cell(id).map(|c| &c.kind),
            Some(crate::cell::CellKind::Call { .. })
        )));
        let v = crate::passes::verdict::judge(&f, &g, &rec, &funcs);
        assert!(v.is_clean(), "verdict must be clean: {:?}", v);
        assert_eq!(v.interp_preserved, 1, "answer 42 preserved across the graft");
        assert_eq!(v.replay_identical, 1);
        // the stats leg saw the inline
        let (_g2, _r2, st) = pass_stats(&f, &funcs).expect("stats");
        assert_eq!(st.inlined, 1);
    }
}

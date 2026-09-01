//! const-branch fold, graduated from the R2 spike (region.rs pass A)
//! into the maintained pass set: a Branch whose condition is a
//! dataflow constant becomes a Jump on the taken arm, with phi-join
//! maintenance in the dropped arm (the surgery constfold.rs deferred).
//!
//! thin wrapper: the transform lives in region::const_branch_fold;
//! this module only fixes the pass name and the (fabric, diff) shape
//! the pipeline expects. pass_stats exposes the FoldStats variant for
//! measurement.

use crate::diff::DiffRecord;
use crate::fabric::Fabric;
use crate::region::FoldStats;

pub const PASS_NAME: &str = "const-branch-fold";

/// pipeline shape: stats dropped.
pub fn pass(f: &Fabric) -> Result<(Fabric, DiffRecord), String> {
    let (g, rec, _st) = pass_stats(f)?;
    Ok((g, rec))
}

/// measurement shape: the FoldStats ride along.
pub fn pass_stats(f: &Fabric) -> Result<(Fabric, DiffRecord, FoldStats), String> {
    crate::region::const_branch_fold(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::verify;
    use std::collections::BTreeMap;

    /// diamond with a CONST condition: entry br(true) -> t/el -> j;
    /// arms carry different consts; ret the phi (region.rs's
    /// const_diamond fixture). The answer is 1, not 2.
    fn const_diamond() -> Fabric {
        let text = "fabric v0\n\
region entry\n\
  %0 = const i1 true\n\
  %1 = br %0, t, el\n\
region t\n\
  %2 = const i32 1\n\
  %3 = jump j\n\
region el\n\
  %4 = const i32 2\n\
  %5 = jump j\n\
region j\n\
  %6 = phi [t: %2] [el: %4]\n\
  %7 = ret %6\n";
        crate::text::parse(text).expect("const diamond parses")
    }

    #[test]
    fn green_fold_verifies_conserves_and_judges_clean() {
        let f = const_diamond();
        assert!(verify(&f).is_ok(), "test fabric must verify first");
        let (g, rec) = pass(&f).expect("fold");
        assert_ne!(g, f, "red without the pass: identity would fail below");
        assert!(verify(&g).is_ok(), "verify green after the fold");
        assert!(
            crate::conserve::population_audit(&f, &g, &rec).is_ok(),
            "population audit holds"
        );
        let v = crate::passes::verdict::judge(&f, &g, &rec, &BTreeMap::new());
        assert!(v.is_clean(), "verdict must be clean: {:?}", v);
        assert_eq!(v.verify_green, 1);
        assert_eq!(v.interp_preserved, 1, "answer 1 preserved");
        assert_eq!(v.replay_identical, 1);
        // the stats leg sees the fold too
        let (_g2, _r2, st) = pass_stats(&f).expect("stats");
        assert_eq!(st.folded, 1, "the const branch folded");
    }
}

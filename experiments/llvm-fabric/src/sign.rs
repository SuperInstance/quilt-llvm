//! Fabric signatures and the Weft — the hash-chained tick ledger.
//!
//! Reverse-walk round-3 keeper (REVERSE-ACTUALIZATION §4, Round 3):
//! "snapshot the fabric signature at every tick from the very first
//! pass run, even with nothing to do with them yet — nearly free to
//! record and impossible to recover retroactively once passes churn."
//! And round-4's logic critique: progress must be MECHANICAL (a tick
//! either advances the fabric or declares fixed point as a ledger
//! entry) "or it launders into vibes."
//!
//! Fiber discipline (THESIS-V3, via the scout report): a hash must
//! declare which equivalence it observes. This one observes the
//! CANONICAL TEXT of the fabric — two fabrics with equal `print()` get
//! equal signatures; anything the canonical text distinguishes, the
//! signature distinguishes. It is FNV-1a 64-bit: NOT cryptographic,
//! collision-resistant only by accident; an identical signature is a
//! structural-equality claim about the printed form, never a deeper
//! identity claim (the phantom-hash law, scout's quilt-verilog
//! warning, carried verbatim). Tamper DETECTION via the chain is the
//! claim; tamper resistance is not.

use crate::fabric::Fabric;

/// FNV-1a 64-bit. Zero dependencies on purpose.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// The fabric signature: FNV-1a over the canonical text.
/// Observable: `text::print(f)` byte-for-byte. Cheap: one print pass
/// plus one hash pass (measured in bench; see EXPERIMENTS.md).
pub fn fabric_sig(f: &Fabric) -> u64 {
    fnv1a64(crate::text::print(f).as_bytes())
}

/// One Weft entry: what happened at one tick, hash-chained to the
/// previous tick. The progress law lives here — `advanced` is derived
/// MECHANICALLY from the diff (edits > 0), never asserted by the pass
/// author.
#[derive(Clone, PartialEq, Debug)]
pub struct TickSig {
    pub epoch: u64,
    pub pass: &'static str,
    /// signature of the fabric AFTER this tick
    pub sig: u64,
    /// chained hash: fnv(prev.chain ++ epoch ++ pass ++ sig)
    pub chain: u64,
    /// true iff the tick's diff carried at least one edit
    pub advanced: bool,
    /// the progress ledger line: "advanced (N edits)" or
    /// "fixed point — no edits fired"
    pub note: String,
}

impl TickSig {
    /// Chain step: binds this tick to everything before it.
    pub fn chain_step(prev_chain: Option<u64>, epoch: u64, pass: &str, sig: u64) -> u64 {
        let mut bytes = Vec::with_capacity(8 + 64);
        if let Some(p) = prev_chain {
            bytes.extend_from_slice(&p.to_le_bytes());
        }
        bytes.extend_from_slice(&epoch.to_le_bytes());
        bytes.extend_from_slice(pass.as_bytes());
        bytes.extend_from_slice(b"|");
        bytes.extend_from_slice(&sig.to_le_bytes());
        fnv1a64(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, CellKind};
    use crate::id::CellId;

    fn one(i: i32) -> Fabric {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        f.add_cell(
            e,
            Cell::new(e, CellKind::Const { ty: crate::ty::Type::I32, val: crate::ty::ConstVal::I32(i) }),
        );
        f.add_cell(e, Cell::new(e, CellKind::Ret));
        f
    }

    #[test]
    fn signature_observes_canonical_text() {
        let a = one(1);
        let b = one(1);
        let c = one(2);
        assert_eq!(fabric_sig(&a), fabric_sig(&b), "equal prints => equal sigs");
        assert_ne!(fabric_sig(&a), fabric_sig(&c), "different consts => different prints => different sigs");
    }

    #[test]
    fn fnv_is_deterministic_and_spreads() {
        let x = fnv1a64(b"quilt");
        assert_eq!(x, fnv1a64(b"quilt"));
        assert_ne!(x, fnv1a64(b"quilt "));
        assert_ne!(x, 0);
    }

    #[test]
    fn chain_step_binds_the_past() {
        let s = 12345u64;
        let c1 = TickSig::chain_step(None, 0, "a", s);
        let c2 = TickSig::chain_step(Some(c1), 1, "a", s);
        // same tick content but different predecessor chain => different chain
        let c0_alt = TickSig::chain_step(None, 0, "z", s);
        let c2_alt = TickSig::chain_step(Some(c0_alt), 1, "a", s);
        assert_ne!(c2, c2_alt);
        let _ = CellId(0);
    }
}

//! WeftMesh — the MerkleMesh pattern applied INSIDE one fabric fleet:
//! one Merkle root over the Weft's per-tick entry hashes.
//!
//! Cross-pollination spike #1 (docs/CROSS-POLLINATION.md, "MerkleMesh ×
//! Weft"; full study + measurements in docs/phase/MERKLE-WEFT.md).
//! MerkleMesh (SuperInstance, TS) aggregates many boat journals into a
//! fleet root with `leaf = sha256(canonical({kind, cell_id, chain_hash,
//! entries}))`, nodes `sha256(canonical({kind, left, right}))`, leaves
//! sorted by cell id, odd levels duplicating the last node (the Bitcoin
//! convention), and inclusion proofs as sibling paths
//! (`MerkleMesh/src/mesh.ts`). This module is that pattern, one level
//! down in granularity: the "journals" are per-fabric Wefts, the
//! leaves are per-tick `TickSig` records, and one root attests every
//! tick of every fabric at ledger close.
//!
//! Why SHA-256 here when the Weft chain is FNV-1a-64: the Weft chain's
//! honest claim is tamper DETECTION, not resistance (sign.rs header),
//! and FNV-64 gives an editor a 2^-64-ish collision shortcut per link.
//! The root's job is to be a tamper TRIPWIRE over the closed ledger, so
//! it hashes with SHA-256 (implemented below, zero dependencies on
//! purpose, NIST-vector-pinned like MerkleMesh's `src/sha256.ts`). The
//! root is only as honest as its publication point — see the study
//! note; a root nobody published protects nobody.
//!
//! Canonical form: MerkleMesh hashes canonical JSON with serde_json/ryū
//! number semantics — their whole porting hazard. The Weft has no JSON
//! dependency, so leaves/nodes hash a Rust-native, length-prefixed,
//! domain-separated byte form. Unambiguous by construction (no field
//! can bleed into the next), version-tagged `weft-mesh/leaf/1` /
//! `weft-mesh/node/1` like MerkleMesh's `merklemesh/*/1` kinds. If the
//! form ever changes, bump the tag — an old root must never silently
//! verify against a new form.
//!
//! What this is NOT (honest boundary, argued with measurements in the
//! phase doc): the chain walk stays the ground truth. `verify_chain`
//! binds every recorded signature to the ACTUAL stage fabric — a
//! semantic check no amount of hashing recorded bytes can replicate. A
//! root over a self-consistent forgery verifies perfectly; only the
//! walk (with stages) catches a ledger that lies about reality. The
//! root's value: (1) O(1) post-close tamper tripwire over the record
//! BYTES (including fields the chain never covers, like the progress
//! note), (2) O(log N) spot-inclusion proofs against a published root,
//! vs O(N) re-walks. Chain walk = did the entries tell the truth;
//! root = are these still the entries that closed.

use crate::sign::TickSig;

// ---------- SHA-256 (zero-dependency, NIST-pinned) ----------

const SHA_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256, compact and dependency-free (the MerkleMesh discipline:
/// its `src/sha256.ts` is zero-dependency too; both are pinned to the
/// same NIST vectors in their test suites).
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    // padding: 0x80, zeros, 64-bit big-endian bit length
    let bitlen = (bytes.len() as u64).wrapping_mul(8);
    let mut msg = bytes.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());

    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

// ---------- the mesh ----------

/// Canonical leaf preimage: domain tag, then every recorded field,
/// length-prefixed where variable. Covers MORE than the chain does —
/// the chain binds (epoch, pass, sig) only; the leaf also commits to
/// the progress note and the `advanced` bit, so note-edits (invisible
/// to the chain) trip the root.
pub fn leaf_bytes(fabric: &str, t: &TickSig) -> Vec<u8> {
    let mut v = Vec::with_capacity(64 + fabric.len() + t.pass.len() + t.note.len());
    v.extend_from_slice(b"weft-mesh/leaf/1");
    v.push(0);
    v.extend_from_slice(&(fabric.len() as u64).to_le_bytes());
    v.extend_from_slice(fabric.as_bytes());
    v.extend_from_slice(&t.epoch.to_le_bytes());
    v.extend_from_slice(&(t.pass.len() as u64).to_le_bytes());
    v.extend_from_slice(t.pass.as_bytes());
    v.extend_from_slice(&t.sig.to_le_bytes());
    v.extend_from_slice(&t.chain.to_le_bytes());
    v.push(t.advanced as u8);
    v.extend_from_slice(&(t.note.len() as u64).to_le_bytes());
    v.extend_from_slice(t.note.as_bytes());
    v
}

pub fn leaf_hash(fabric: &str, t: &TickSig) -> [u8; 32] {
    sha256(&leaf_bytes(fabric, t))
}

fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut v = Vec::with_capacity(17 + 64);
    v.extend_from_slice(b"weft-mesh/node/1");
    v.push(0);
    v.extend_from_slice(left);
    v.extend_from_slice(right);
    sha256(&v)
}

/// A closed-ledger Merkle tree over Weft entries. Entry order is the
/// ledger's own: fabric key ascending, epoch ascending within a fabric
/// (MerkleMesh sorts leaves by cell id for the same determinism
/// reason — the closed ledger IS the sorted order, so we take entries
/// as given and every build over the same bytes yields the same root).
#[derive(Clone, Debug)]
pub struct WeftMesh {
    /// levels[0] = leaf hashes; each level halves (odd → last node
    /// pairs with itself, the Bitcoin convention); root = last level.
    levels: Vec<Vec<[u8; 32]>>,
    pub root: [u8; 32],
    pub leaves: usize,
}

/// One sibling path from a leaf to the root. `sib_left` says the
/// sibling sits on the prover's left (fold sibling-first); otherwise
/// fold self-first.
#[derive(Clone, Debug)]
pub struct InclusionProof {
    pub fabric: String,
    pub epoch: u64,
    pub leaf: [u8; 32],
    pub siblings: Vec<([u8; 32], bool)>,
    pub leaf_count: usize,
    pub root: [u8; 32],
}

impl WeftMesh {
    pub fn build(entries: &[(&str, &TickSig)]) -> Result<WeftMesh, String> {
        let mut level0: Vec<[u8; 32]> = Vec::new();
        for (fabric, t) in entries {
            level0.push(leaf_hash(fabric, t));
        }
        if level0.is_empty() {
            return Err("cannot mesh an empty ledger".to_string());
        }
        let mut levels = vec![level0];
        let mut level = &levels[0];
        while level.len() > 1 {
            let mut next: Vec<[u8; 32]> = Vec::with_capacity((level.len() + 1) / 2);
            let mut i = 0;
            while i < level.len() {
                let l = level[i];
                let r = if i + 1 < level.len() { level[i + 1] } else { l };
                next.push(node_hash(&l, &r));
                i += 2;
            }
            levels.push(next);
            level = levels.last().unwrap();
        }
        let root = levels[levels.len() - 1][0];
        let leaves = levels[0].len();
        Ok(WeftMesh { levels, root, leaves })
    }

    pub fn depth(&self) -> usize {
        self.levels.len() - 1
    }

    /// Inclusion proof for flat index `idx` (the entry's position in
    /// the closed ledger order).
    pub fn prove(&self, fabric: &str, t: &TickSig, idx: usize) -> Result<InclusionProof, String> {
        if idx >= self.leaves {
            return Err(format!("index {} outside ledger of {} entries", idx, self.leaves));
        }
        if leaf_hash(fabric, t) != self.levels[0][idx] {
            return Err(format!("entry {} is not the entry at that index — wrong ledger?", idx));
        }
        let mut siblings = Vec::new();
        let mut i = idx;
        for lvl in 0..self.levels.len() - 1 {
            let level = &self.levels[lvl];
            let sib = i ^ 1;
            if sib < level.len() {
                siblings.push((level[sib], sib < i));
            } else {
                // lone node at this level: sibling is itself (duplicate rule)
                siblings.push((level[i], false));
            }
            i >>= 1;
        }
        Ok(InclusionProof {
            fabric: fabric.to_string(),
            epoch: t.epoch,
            leaf: self.levels[0][idx],
            siblings,
            leaf_count: self.leaves,
            root: self.root,
        })
    }
}

/// Verify: fold the sibling path from the claimed entry's leaf hash to
/// a root; check it equals the published root. O(log N) hashes.
pub fn verify_inclusion(fabric: &str, t: &TickSig, proof: &InclusionProof, published_root: &[u8; 32]) -> bool {
    let mut cur = leaf_hash(fabric, t);
    if cur != proof.leaf {
        return false;
    }
    for (sib, sib_left) in &proof.siblings {
        cur = if *sib_left { node_hash(sib, &cur) } else { node_hash(&cur, sib) };
    }
    cur == *published_root
}

/// The ledger-side chain re-walk WITHOUT stage fabrics: recompute every
/// chain link from the recorded entries (epoch, pass, sig) and require
/// gapless re-linking. This is the O(N) integrity walk a root-compare
/// can stand in for — NOT `verify_chain`, which additionally binds
/// each sig to the actual stage (the semantic ground truth).
pub fn relink_walk(wefts: &[(&str, &[TickSig])]) -> Result<(), String> {
    for (fabric, w) in wefts {
        let mut prev: Option<u64> = None;
        for (i, t) in w.iter().enumerate() {
            if t.epoch != i as u64 {
                return Err(format!("ledger {}: epoch gap at {} (={})", fabric, i, t.epoch));
            }
            let want = TickSig::chain_step(prev, t.epoch, t.pass, t.sig);
            if want != t.chain {
                return Err(format!("ledger {}: entry {} does not re-link", fabric, i));
            }
            prev = Some(t.chain);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(epoch: u64, pass: &'static str, sig: u64, prev: Option<u64>, advanced: bool) -> TickSig {
        TickSig {
            epoch,
            pass,
            sig,
            chain: TickSig::chain_step(prev, epoch, pass, sig),
            advanced,
            note: if advanced { "advanced (1 edits)".into() } else { "fixed point — no edits fired".into() },
        }
    }

    fn small_ledger() -> Vec<(&'static str, TickSig)> {
        // three fabrics, chains linked within each fabric (ledger order:
        // fabric ascending, epoch ascending — how a close writes them)
        let groups: &[(&str, &[u64])] = &[
            ("f000", &[0x1111, 0x2222]),
            ("f001", &[0x3333, 0x4444]),
            ("f002", &[0x5555]), // odd leaf count: Bitcoin duplicate rule fires
        ];
        let mut out = Vec::new();
        for (k, sigs) in groups {
            let mut prev: Option<u64> = None;
            for (e, &sig) in sigs.iter().enumerate() {
                let t = tick(e as u64, "constfold", sig, prev, true);
                prev = Some(t.chain);
                out.push((*k, t));
            }
        }
        out
    }

    #[test]
    fn sha256_nist_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // one million 'a' — the million-block edge
        let m = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&sha256(&m)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn root_is_deterministic_and_order_sensitive() {
        let l = small_ledger();
        let refs: Vec<(&str, &TickSig)> = l.iter().map(|(k, t)| (*k, t)).collect();
        let a = WeftMesh::build(&refs).unwrap();
        let b = WeftMesh::build(&refs).unwrap();
        assert_eq!(a.root, b.root, "same closed ledger => same root");
        // swapping two entries changes the root (order IS meaning)
        let mut swapped = refs.clone();
        swapped.swap(0, 2);
        let c = WeftMesh::build(&swapped).unwrap();
        assert_ne!(a.root, c.root);
    }

    #[test]
    fn empty_ledger_rejected() {
        let empty: [(&str, &TickSig); 0] = [];
        assert!(WeftMesh::build(&empty).is_err());
    }

    #[test]
    fn tampered_entry_trips_the_root() {
        let l = small_ledger();
        let refs: Vec<(&str, &TickSig)> = l.iter().map(|(k, t)| (*k, t)).collect();
        let honest = WeftMesh::build(&refs).unwrap();
        for field in ["sig", "note", "epoch", "chain", "advanced", "pass"] {
            let mut l2 = l.clone();
            match field {
                "sig" => l2[1].1.sig ^= 1,
                "note" => l2[1].1.note = "advanced (2 edits)".into(),
                "epoch" => l2[1].1.epoch = 7,
                "chain" => l2[1].1.chain ^= 1,
                "advanced" => l2[1].1.advanced = !l2[1].1.advanced,
                "pass" => l2[1].1.pass = "dce",
                _ => unreachable!(),
            }
            let refs2: Vec<(&str, &TickSig)> = l2.iter().map(|(k, t)| (*k, t)).collect();
            let tampered = WeftMesh::build(&refs2).unwrap();
            assert_ne!(honest.root, tampered.root, "editing {} must trip the root", field);
        }
    }

    #[test]
    fn note_edit_is_invisible_to_the_chain_but_not_to_the_root() {
        // the chain covers (epoch, pass, sig); a note edit re-links fine
        let l = small_ledger();
        let wefts: Vec<(&str, Vec<TickSig>)> = vec![
            ("f000", vec![l[0].1.clone(), l[1].1.clone()]),
            ("f001", vec![l[2].1.clone(), l[3].1.clone()]),
            ("f002", vec![l[4].1.clone()]),
        ];
        let refs: Vec<(&str, &TickSig)> = flat_refs(&wefts);
        let honest = WeftMesh::build(&refs).unwrap();
        let mut edited = wefts;
        edited[0].1[1].note = "advanced (2 edits)".into();
        let walked: Vec<(&str, &[TickSig])> =
            edited.iter().map(|(k, w)| (*k, w.as_slice())).collect();
        assert!(relink_walk(&walked).is_ok(), "chain re-walk cannot see a note edit");
        let tampered = WeftMesh::build(&flat_refs(&edited)).unwrap();
        assert_ne!(honest.root, tampered.root, "but the root can");
    }

    fn flat_refs<'a>(wefts: &'a [(&'a str, Vec<TickSig>)]) -> Vec<(&'a str, &'a TickSig)> {
        wefts.iter().flat_map(|(k, w)| w.iter().map(move |t| (*k, t))).collect()
    }

    #[test]
    fn inclusion_proof_is_log_n_and_verifies() {
        let l = small_ledger();
        let refs: Vec<(&str, &TickSig)> = l.iter().map(|(k, t)| (*k, t)).collect();
        let mesh = WeftMesh::build(&refs).unwrap();
        let proof = mesh.prove("f001", &l[3].1, 3).unwrap();
        assert_eq!(proof.siblings.len(), 3, "5 leaves => depth ceil(log2 5) = 3");
        assert!(verify_inclusion("f001", &l[3].1, &proof, &mesh.root));
        // 40k-entry-shaped depth sanity: depth grows as log2(N)
        let big: Vec<(&str, TickSig)> = (0..40_000)
            .map(|i| ("f", tick(i as u64 % 4, "constfold", i as u64, None, true)))
            .collect();
        let refs_big: Vec<(&str, &TickSig)> = big.iter().map(|(k, t)| (*k, t)).collect();
        let mesh_big = WeftMesh::build(&refs_big).unwrap();
        let proof_big = mesh_big.prove("f", &big[12_345].1, 12_345).unwrap();
        assert_eq!(proof_big.siblings.len(), 16, "40k leaves => depth ceil(log2 40000) = 16");
        assert!(verify_inclusion("f", &big[12_345].1, &proof_big, &mesh_big.root));
    }

    #[test]
    fn proofs_reject_forgery() {
        let l = small_ledger();
        let refs: Vec<(&str, &TickSig)> = l.iter().map(|(k, t)| (*k, t)).collect();
        let mesh = WeftMesh::build(&refs).unwrap();
        let proof = mesh.prove("f001", &l[3].1, 3).unwrap();
        // tampered entry presented with the honest proof
        let mut evil = l[3].1.clone();
        evil.sig ^= 1;
        assert!(!verify_inclusion("f001", &evil, &proof, &mesh.root));
        // honest entry, forged sibling (flip one hash byte at the first level)
        let mut forged = proof.clone();
        forged.siblings[0].0[3] ^= 1;
        assert!(!verify_inclusion("f001", &l[3].1, &forged, &mesh.root));
        // honest proof against a different published root
        let l2 = small_ledger();
        let mut other = l2.clone();
        other[0].1.sig ^= 0xffff;
        let refs2: Vec<(&str, &TickSig)> = other.iter().map(|(k, t)| (*k, t)).collect();
        let other_root = WeftMesh::build(&refs2).unwrap().root;
        assert!(!verify_inclusion("f001", &l[3].1, &proof, &other_root));
        // prove() itself refuses an entry that is not at that index
        assert!(mesh.prove("f001", &l[2].1, 3).is_err());
    }

    #[test]
    fn rechained_forgery_passes_the_walk_fails_the_root() {
        // the attack the root exists for: edit an entry AND recompute
        // the whole chain so the ledger re-links self-consistently
        let l = small_ledger();
        let mut forged = l.clone();
        forged[1].1.sig ^= 1; // lie about the tick-1 fabric signature
        let mut f0: Vec<TickSig> = forged[..2].iter().map(|(_, t)| t.clone()).collect();
        let mut f1: Vec<TickSig> = forged[2..4].iter().map(|(_, t)| t.clone()).collect();
        let mut f2: Vec<TickSig> = forged[4..].iter().map(|(_, t)| t.clone()).collect();
        for w in [&mut f0, &mut f1, &mut f2] {
            let mut prev: Option<u64> = None;
            for t in w.iter_mut() {
                t.chain = TickSig::chain_step(prev, t.epoch, t.pass, t.sig);
                prev = Some(t.chain);
            }
        }
        let wefts: Vec<(&str, &[TickSig])> = vec![("f000", &f0), ("f001", &f1), ("f002", &f2)];
        assert!(relink_walk(&wefts).is_ok(), "self-consistent forgery re-links");
        let refs_honest: Vec<(&str, &TickSig)> = l.iter().map(|(k, t)| (*k, t)).collect();
        let refs_forge: Vec<(&str, &TickSig)> = forged.iter().map(|(k, t)| (*k, t)).collect();
        let honest = WeftMesh::build(&refs_honest).unwrap();
        let forged_root = WeftMesh::build(&refs_forge).unwrap();
        assert_ne!(honest.root, forged_root.root, "but the published root trips it");
    }

    #[test]
    fn single_entry_mesh() {
        let t = tick(0, "constfold", 42, None, true);
        let mesh = WeftMesh::build(&[("f000", &t)]).unwrap();
        assert_eq!(mesh.root, leaf_hash("f000", &t), "one leaf: the root IS the leaf");
        let p = mesh.prove("f000", &t, 0).unwrap();
        assert!(p.siblings.is_empty());
        assert!(verify_inclusion("f000", &t, &p, &mesh.root));
    }

    #[test]
    fn corpus_agreement_walk_vs_root() {
        // real pipeline histories (250 fabrics): the full walk passes,
        // EVERY entry proves into the root, and a one-sig tamper trips
        // BOTH detectors. The 10k/40k measured run is the bin
        // (weftmesh); this pins the same protocol in the suite.
        let n = 250;
        let mut keys = Vec::new();
        let mut wefts = Vec::new();
        for i in 0..n {
            let mut rng = crate::fuzz::Rng::new((1u64).wrapping_add(i as u64));
            let f = crate::fuzz::gen_fabric(&mut rng);
            let (_, history, stages) = crate::pipeline::run(&f).expect("pipeline");
            assert!(history.check_weft().is_ok());
            assert!(history.verify_chain(&stages).is_ok(), "ground truth must hold");
            keys.push(format!("f{:05}", i));
            wefts.push(history.weft);
        }
        let flat: Vec<(&str, &TickSig)> =
            keys.iter().zip(wefts.iter()).flat_map(|(k, w)| w.iter().map(move |t| (k.as_str(), t))).collect();
        assert!(flat.len() > 3 * n, "pipeline ticks must land in the ledger");
        let mesh = WeftMesh::build(&flat).unwrap();
        let rebuilt = WeftMesh::build(&flat).unwrap();
        assert_eq!(mesh.root, rebuilt.root);
        for (i, (k, t)) in flat.iter().enumerate() {
            let p = mesh.prove(k, t, i).unwrap();
            assert!(verify_inclusion(k, t, &p, &mesh.root), "entry {}/{} must prove", i, k);
        }
        // one-sig tamper trips the root AND the chain walk
        let mut evil = wefts.clone();
        evil[n / 2][1].sig ^= 1;
        let evil_flat: Vec<(&str, &TickSig)> =
            keys.iter().zip(evil.iter()).flat_map(|(k, w)| w.iter().map(move |t| (k.as_str(), t))).collect();
        let evil_root = WeftMesh::build(&evil_flat).unwrap().root;
        assert_ne!(mesh.root, evil_root);
        let walked: Vec<(&str, &[TickSig])> =
            keys.iter().zip(evil.iter()).map(|(k, w)| (k.as_str(), w.as_slice())).collect();
        assert!(relink_walk(&walked).is_err(), "sig edit must also trip the chain");
    }
}

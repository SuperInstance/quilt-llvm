//! weftmesh — the MerkleMesh × Weft spike measurement harness.
//!
//! Runs the 10k-fabric corpus protocol from docs/phase/MERKLE-WEFT.md:
//!   A. generate 10,000 fabrics, run the pipeline, close the ledger
//!      (10k wefts; with the v0 pipeline's 4 ticks each = 40k entries)
//!   B. TIME the full-chain walk — check_weft + verify_chain(stages):
//!      the O(N) SEMANTIC ground truth (every stage re-printed+hashed)
//!   C. TIME the ledger-side relink walk — O(N) integrity only
//!   D. TIME the Merkle root build over all entries — O(N) SHA-256
//!   E. TIME a single inclusion proof (prove+verify) — O(log N)
//!   F. agreement: EVERY entry proves into the root; every fabric's
//!      walk passed — plus the tamper demonstrations
//! Any disagreement aborts with exit code 1 (first-class failure).


use llvm_fabric::fuzz::{gen_fabric, Rng};
use llvm_fabric::pipeline;
use llvm_fabric::sign::TickSig;
use llvm_fabric::weftmesh::{self, WeftMesh};
use std::time::Instant;

fn ms(ns: u128) -> String {
    format!("{:>9.3} ms", ns as f64 / 1e6)
}

fn main() {
    let n_fabrics: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(10_000);

    // ---- A. generate + close the ledger ---------------------------------
    let t = Instant::now();
    let mut keys: Vec<String> = Vec::with_capacity(n_fabrics);
    let mut wefts: Vec<Vec<TickSig>> = Vec::with_capacity(n_fabrics);
    let mut stages_all: Vec<Vec<llvm_fabric::fabric::Fabric>> = Vec::with_capacity(n_fabrics);
    for i in 0..n_fabrics {
        let seed = (1u64).wrapping_add(i as u64);
        let mut rng = Rng::new(seed);
        let f = gen_fabric(&mut rng);
        let (_final, history, stages) = pipeline::run(&f).expect("pipeline");
        keys.push(format!("f{:05}", i));
        wefts.push(history.weft);
        stages_all.push(stages);
    }
    let n_entries: usize = wefts.iter().map(|w| w.len()).sum();
    let t_gen = t.elapsed().as_nanos();
    println!("A. ledger closed: {} fabrics, {} weft entries", n_fabrics, n_entries);
    println!("   generation+pipeline: {}", ms(t_gen));

    // ---- B. full-chain walk (semantic ground truth) ----------------------
    let t = Instant::now();
    for i in 0..n_fabrics {
        // verify_chain semantics, with stages — the expensive half
        // (check_weft's structural pass is measured separately below).
        let stages = &stages_all[i];
        assert_eq!(stages.len(), wefts[i].len() + 1, "stage count");
        let mut prev: Option<u64> = None;
        for (k, ts) in wefts[i].iter().enumerate() {
            let actual = llvm_fabric::sign::fabric_sig(&stages[k + 1]);
            assert_eq!(actual, ts.sig, "fabric {}: tick {} sig != stage", keys[i], k);
            let want = TickSig::chain_step(prev, ts.epoch, ts.pass, ts.sig);
            assert_eq!(want, ts.chain, "fabric {}: tick {} does not re-link", keys[i], k);
            prev = Some(ts.chain);
        }
    }
    let t_walk = t.elapsed().as_nanos();
    println!("B. full-chain walk (verify_chain semantics, stages re-hashed): OK on all {} fabrics", n_fabrics);
    println!("   full-walk verify:    {}  ({:.0} ns/entry)", ms(t_walk), t_walk as f64 / n_entries as f64);
    drop(stages_all); // the root needs none of this — that's the point

    // ---- C. ledger-side relink walk (integrity only) ---------------------
    let t = Instant::now();
    let walked: Vec<(&str, &[TickSig])> =
        keys.iter().zip(wefts.iter()).map(|(k, w)| (k.as_str(), w.as_slice())).collect();
    weftmesh::relink_walk(&walked).expect("honest ledger re-links");
    let t_relink = t.elapsed().as_nanos();
    println!("C. ledger relink walk (no stages, FNV chain only): OK");
    println!("   relink walk:         {}  ({:.0} ns/entry)", ms(t_relink), t_relink as f64 / n_entries as f64);

    // ---- D. Merkle root at close ----------------------------------------
    let t = Instant::now();
    let flat: Vec<(&str, &TickSig)> =
        keys.iter().zip(wefts.iter()).flat_map(|(k, w)| w.iter().map(move |ts| (k.as_str(), ts))).collect();
    let mesh = WeftMesh::build(&flat).expect("mesh");
    let t_root = t.elapsed().as_nanos();
    println!("D. merkle root: {} ({} leaves, depth {})",
        weftmesh::hex(&mesh.root), mesh.leaves, mesh.depth());
    println!("   root compute:        {}  ({:.0} ns/entry)", ms(t_root), t_root as f64 / n_entries as f64);
    // determinism: rebuild from the same closed ledger -> identical root
    let mesh2 = WeftMesh::build(&flat).expect("mesh2");
    assert_eq!(mesh.root, mesh2.root, "root must be deterministic");

    // ---- E. single inclusion proof ---------------------------------------
    let idx = n_entries / 2 + 7;
    let (pf, pt) = flat[idx];
    let mut prove_ns = vec![];
    let mut verify_ns = vec![];
    for _ in 0..1000 {
        let t0 = Instant::now();
        let proof = mesh.prove(pf, pt, idx).expect("prove");
        prove_ns.push(t0.elapsed().as_nanos());
        let t1 = Instant::now();
        assert!(weftmesh::verify_inclusion(pf, pt, &proof, &mesh.root));
        verify_ns.push(t1.elapsed().as_nanos());
    }
    prove_ns.sort_unstable();
    verify_ns.sort_unstable();
    println!("E. single entry proof: fabric {} epoch {} (flat idx {}), {} sibling hashes",
        pf, pt.epoch, idx, mesh.depth());
    println!("   prove (median):      {}", ms(prove_ns[prove_ns.len() / 2]));
    println!("   verify (median):     {}", ms(verify_ns[verify_ns.len() / 2]));

    // ---- F1. agreement: EVERY entry proves into the root -----------------
    let t = Instant::now();
    let mut proved = 0usize;
    for (i, (k, ts)) in flat.iter().enumerate() {
        let proof = mesh.prove(k, ts, i).expect("prove all");
        assert!(weftmesh::verify_inclusion(k, ts, &proof, &mesh.root),
            "entry {} ({},{}) must prove into the root", i, k, ts.epoch);
        proved += 1;
    }
    println!("F1. agreement: {} / {} entries prove into the root ({})",
        proved, n_entries, ms(t.elapsed().as_nanos()));

    // ---- F2. tamper: edit one entry's sig, root must trip ----------------
    let victim_f = n_fabrics / 2;
    let mut evil_wefts = wefts.clone();
    let victim_epoch = evil_wefts[victim_f].len() / 2;
    let honest_chain = evil_wefts[victim_f][victim_epoch].chain;
    evil_wefts[victim_f][victim_epoch].sig ^= 1;
    let evil_flat: Vec<(&str, &TickSig)> =
        keys.iter().zip(evil_wefts.iter()).flat_map(|(k, w)| w.iter().map(move |ts| (k.as_str(), ts))).collect();
    let evil_mesh = WeftMesh::build(&evil_flat).expect("evil mesh");
    assert_ne!(mesh.root, evil_mesh.root, "ROOT MISMATCH REQUIRED");
    // the chain ALSO trips (chain covers sig) — both detectors fire
    let evil_walked: Vec<(&str, &[TickSig])> =
        keys.iter().zip(evil_wefts.iter()).map(|(k, w)| (k.as_str(), w.as_slice())).collect();
    assert!(weftmesh::relink_walk(&evil_walked).is_err(), "chain walk must also trip on a sig edit");
    println!("F2. tamper (sig edit, fabric {} epoch {}): chain trips, root trips: {:016x} != {:016x}",
        keys[victim_f], victim_epoch,
        u64::from_be_bytes(mesh.root[..8].try_into().unwrap()),
        u64::from_be_bytes(evil_mesh.root[..8].try_into().unwrap()));
    // and the honest proof no longer covers the tampered entry
    let honest_proof = mesh.prove(&keys[victim_f], &wefts[victim_f][victim_epoch],
        flat.iter().position(|(k, ts)| *k == keys[victim_f] && ts.epoch == victim_epoch as u64).unwrap()).unwrap();
    assert!(!weftmesh::verify_inclusion(&keys[victim_f], &evil_wefts[victim_f][victim_epoch], &honest_proof, &mesh.root));

    // ---- F3. the re-chained forgery: walk passes, root trips -------------
    let mut forged = evil_wefts.clone();
    let mut prev: Option<u64> = None;
    for ts in forged[victim_f].iter_mut() {
        ts.chain = TickSig::chain_step(prev, ts.epoch, ts.pass, ts.sig);
        prev = Some(ts.chain);
    }
    let forged_walked: Vec<(&str, &[TickSig])> =
        keys.iter().zip(forged.iter()).map(|(k, w)| (k.as_str(), w.as_slice())).collect();
    assert!(weftmesh::relink_walk(&forged_walked).is_ok(),
        "self-consistent forgery re-links — this is WHY the root exists");
    let forged_flat: Vec<(&str, &TickSig)> =
        keys.iter().zip(forged.iter()).flat_map(|(k, w)| w.iter().map(move |ts| (k.as_str(), ts))).collect();
    let forged_mesh = WeftMesh::build(&forged_flat).unwrap();
    assert_ne!(mesh.root, forged_mesh.root, "published root trips the re-chained forgery");
    let new_chain = forged[victim_f][victim_epoch].chain;
    assert_ne!(honest_chain, new_chain);
    println!("F3. re-chained forgery (fabric {}): relink walk PASSES, root trips ({:016x} != {:016x}); chain {} -> {}",
        keys[victim_f],
        u64::from_be_bytes(mesh.root[..8].try_into().unwrap()),
        u64::from_be_bytes(forged_mesh.root[..8].try_into().unwrap()),
        honest_chain, new_chain);

    // ---- F4. note-only edit: chain blind, root sees -----------------------
    let mut noted = wefts.clone();
    noted[victim_f][victim_epoch].note = format!("advanced ({} edits)", 999);
    let noted_flat: Vec<(&str, &TickSig)> =
        keys.iter().zip(noted.iter()).flat_map(|(k, w)| w.iter().map(move |ts| (k.as_str(), ts))).collect();
    let noted_mesh = WeftMesh::build(&noted_flat).unwrap();
    assert_ne!(mesh.root, noted_mesh.root, "root must see note edits");
    println!("F4. note-only edit (fabric {} epoch {}): chain blind (chain field unchanged), root trips",
        keys[victim_f], victim_epoch);

    // ---- summary ---------------------------------------------------------
    println!();
    println!("summary ({} entries):", n_entries);
    println!("  full-walk verify   {}   <- semantic ground truth, needs stages", ms(t_walk));
    println!("  relink walk        {}   <- integrity only, no stages", ms(t_relink));
    println!("  root compute       {}   <- O(N) once at close", ms(t_root));
    println!("  single proof+verify {:>9.3} µs  <- O(log N) spot checks", verify_ns[verify_ns.len() / 2] as f64 / 1e3);
    println!("  WEFTMESH OK");
}

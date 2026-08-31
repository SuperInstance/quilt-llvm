# MERKLE-WEFT — one root over the Weft (cross-pollination finding #1)

Branch `r2-merkle-weft` (side lane R2, from cross-pollination finding #1 in
`docs/CROSS-POLLINATION.md`, 2026-08-30). One lane, one worktree:
`/home/eileen/projects/quilt-llvm-wt-merkle`. New module
`experiments/llvm-fabric/src/weftmesh.rs` (+ measurement bin
`src/bin/weftmesh.rs`); suite green at 153 tests.

## The question

The Weft ledger is verified today by walking the full hash chain
(`diff.rs::verify_chain`: every entry re-linked, every recorded signature
re-hashed against the actual stage fabric). MerkleMesh suggests batch
attestation: can N ledger entries be attested by ONE root hash so
spot-verification is O(log N) instead of O(N)?

**Short answer: yes for inclusion, no for replacement — and with one
falsified sub-hypothesis along the way (the root is NOT the cheapest
whole-ledger integrity check; the plain FNV relink walk is).** The chain
walk stays the ground truth; the root is a publication anchor + tamper
tripwire + O(log N) spot-proof index. Measurements below.

## 1. Study note — what MerkleMesh actually does (read, not guessed)

Repo: **SuperInstance/MerkleMesh** (public; fork of an unstarted stub
`fuad403273/MerkleMesh` — the working system was written fresh for the
fleet, per its own README "Lineage"; first real commit `bb5a88d`
"MerkleMesh v0.1.0", 2026-08-29). TypeScript, zero runtime dependencies.
981 lines across `src/`.

- **`src/ledger.ts`** — verifies one quilt cell-ledger journal (JSONL,
  format `quilt-cell-ledger/1`): walk entries, recompute every
  `seal = sha256_hex(canonical(entry))`, check `prev_hash` links from the
  genesis commit; the journal's `chain_hash` is the head seal (or the
  genesis commit when empty). This is *the same shape* as our Weft walk,
  different hash discipline (SHA-256 canonical-JSON vs our FNV-1a-64
  byte chain).
- **`src/mesh.ts`** — the aggregation: `leaf = sha256(canonical({kind:
  "merklemesh/leaf/1", cell_id, chain_hash, entries}))`, `node =
  sha256(canonical({kind: "merklemesh/node/1", left, right}))`, leaves
  sorted by cell_id (UTF-8 byte order — determinism regardless of
  directory order), **odd levels duplicate the last node (the Bitcoin
  convention)**, duplicate cell ids rejected. Inclusion proof = bottom-up
  sibling path; `verifyProof` folds the path leaf→root and compares;
  `verifyInclusion` additionally re-verifies the journal chain and that
  the proof commits to the journal's *current* chain hash.
- **`src/canonical.ts` + `src/sha256.ts`** — the porting-hazard core:
  quilt hashes canonical JSON with serde_json/ryū semantics (`40.0` is a
  float and must render `40.0`); JS `JSON.parse` erases that, so they
  carry a number-preserving parser (raw lexemes) and a zero-dependency
  SHA-256. Honest boundary in the README: floats outside plain-decimal
  range (|x| < 1e-5 or ≥ 1e16) are rejected rather than hashed wrong.
- **Tests**: `test/{sha256,canonical,ledger,mesh,cli}.test.ts` = 4 + 10 +
  15 + 10 + 10 = **49** (the README's claim; counted, and the repo's
  `npm test` gates them). NIST SHA-256 vectors, pinned canonical vectors
  from quilt-core's own suite, five Rust-generated golden journals with
  exact chain hashes, tamper-at-the-right-seq tests, mesh determinism /
  root sensitivity / proof rejection, CLI failure exits.

**Granularity shift this lane makes:** MerkleMesh puts ONE leaf per
journal (leaf = the whole journal's chain hash; "one fleet, one root"
across boats). WeftMesh puts one leaf per **tick entry** — the same
construction one level down, so the root attests every tick of every
fabric in one 32-byte anchor, and a single tick — not just a whole
fabric — can be proven present in O(log N).

## 2. The spike — `weftmesh.rs`

Construction (MerkleMesh's rules, Rust-native form):

- `leaf = sha256("weft-mesh/leaf/1" ‖ len-prefixed {fabric, epoch, pass,
  sig, chain, advanced, note})` — **the leaf covers fields the chain
  never hashes** (the progress `note`, the `advanced` bit). Domain-tagged
  and version-suffixed like MerkleMesh's kinds; bump the tag on any form
  change.
- `node = sha256("weft-mesh/node/1" ‖ left ‖ right)`; odd levels
  duplicate the last node (Bitcoin convention, exactly `mesh.ts`).
- Ledger order = fabric key ascending, epoch ascending (the close's own
  order; MerkleMesh's sort-by-cell_id determinism rule, applied at close).
- **Canonical form divergence, on purpose:** no canonical JSON. Length-
  prefixed fields are unambiguous by construction — the entire class of
  ryū/`40.0` number-semantics hazards MerkleMesh had to engineer around
  cannot arise. The Weft has no JSON dependency to match.
- Zero external dependencies (house law): SHA-256 implemented in-module
  (~70 lines), pinned to NIST vectors incl. the million-`a` block, same
  discipline as `src/sha256.ts`.
- Why SHA-256 when the Weft chain is FNV-1a-64: the root's job is tamper
  tripwire over a *closed* ledger; FNV-64 offers an attacker-chosen-input
  collision shortcut (birthday space 2^64, precomputation free). The
  Weft's honest "detection, not resistance" stance stays the chain's
  stance; the anchor gets the stronger hash.

API: `WeftMesh::build(&[(fabric, &TickSig)])` → `{root, depth, leaves}`;
`mesh.prove(fabric, tick, idx)` → sibling path (refuses an entry that
isn't at that index); `verify_inclusion(fabric, tick, &proof, &root)`;
`relink_walk(&[(fabric, &[TickSig])])` = the ledger-side O(N) chain
re-walk *without* stages (the thing a root might replace). 10 tests in
the module; the measurement protocol is `src/bin/weftmesh.rs` (arg = #
fabrics, asserts everything, exit 1 on any disagreement — first-class
failure).

## 3. Measured — 10k-fabric corpus, 40,000 entries

`cargo run --release --bin weftmesh -- 10000` (seeds 1..=10000,
`fuzz::gen_fabric` → `pipeline::run`, v0 4-tick wefts; two runs, both
asserted; WSL2, release):

| check | time | per entry | needs stages? |
|---|---|---|---|
| **full-walk verify** (`verify_chain` semantics: re-hash every stage + re-link) | 178–207 ms | ~4.5–5.2 µs | yes (the fabrics) |
| **relink walk** (chain re-link only, FNV) | 5.8–7.4 ms | ~144–186 ns | no |
| **Merkle root compute** (40k SHA-256 leaves + tree) | 75–112 ms | ~2.1–2.8 µs | no |
| **single inclusion proof** (prove+verify, median of 1000) | 8–16 µs | — (16 sibling hashes, O(log N)) | no |

Root over the corpus: `c869701229f831d5…` (depth 16). Determinism
asserted: rebuild → identical root; three full runs → identical root
(full root recorded at the end of this doc; per-run timing above spans
the three runs — the spread is machine noise, not seed variance).

**The falsified sub-hypothesis:** "root replaces the walk because it's
faster." It isn't — SHA-256 root compute (~80 ms) is ~14× the plain FNV
relink walk (~6 ms) for whole-ledger integrity. The root is not the
cheap integrity check; the relink walk already is. What the root buys
that the relink walk cannot, per tamper class:

- **F2 — one sig edit** (fabric f05000, epoch 2): chain trips AND root
  trips (`c869701229f831d5… ≠ afc11669033f70da…`). Both detectors fire;
  no surprise.
- **F3 — the re-chained forgery** (edit the sig, recompute the whole
  chain so it re-links self-consistently): relink walk **PASSES**
  (forged chain tip `5628952514640834195 → 6055168267424892026`), root
  **trips** against the published root (`≠ a040e42405b80172`). Without
  an anchored root, a self-consistent re-chain defeats the ledger-side
  walk entirely. This is the attack the root exists for.
- **F4 — note-only edit** ("advanced (3 edits)" → "(999 edits)"): chain
  is structurally blind (chain covers epoch/pass/sig only — chain field
  literally unchanged), root trips. The leaf covers the whole record,
  so the root's byte coverage is strictly wider than the chain's.
- **F1 — agreement:** all 40,000/40,000 entries prove into the root
  (each proof verified against the root); the full semantic walk passes
  on all 10,000 fabrics. The two attestation mechanisms agree on the
  honest ledger and on every tamper class above. (Also pinned at 250
  fabrics inside `cargo test` as `corpus_agreement_walk_vs_root`.)

## 4. Verdict — tripwire + accelerator, not a replacement

**The chain walk stays the ground truth.** `verify_chain` (with stages)
binds every recorded signature to the *actual fabric that existed* — a
semantic claim. No root over recorded bytes can replicate it: F3 run
backwards is the proof — a ledger forged *before* close hashes to a
perfectly valid root. The root attests "these are the entries that
closed"; only the stage-walk attests "the entries told the truth."

What the root *should* replace: the post-close byte-integrity role of
the relink walk — not for speed (it's slower) but for coverage (notes,
advanced bits) and anchoring: one 32-byte root in the run manifest (or
the EXPERIMENTS ledger, or a git tag — the reachability-of-every-hash
doctrine in `docs/DOCTRINE.md`) makes every later audit an O(N)-recompute
+ O(1)-compare, immune to re-chaining, and every single-entry question an
O(log N) proof: 10 µs vs 178 ms for one tick's attestation (~15,000×),
which is the batch-attestation win MerkleMesh promised, measured.

Cost at close: ~2.1 µs/entry on top of the run — ~0.05% of the ~4.5
µs/entry the pipeline already spends on the semantic walk. Cheap enough
to always record; `weftmesh.rs` is spike-grade (no streaming, tree kept
in memory — ~3 MB at 40k entries; O(N) memory is the honest ceiling to
note before any fleet-scale adoption).

**Failures first-class:** three compile-fix rounds (slice-vs-iterator
API on `build`, `&'static str` on the tick helper, per-fabric re-chaining
in the forgery test — my first forgery re-chained across fabrics, which
`relink_walk` rightly rejected; chains never cross fabrics). The
bin asserts on every claim above and exits non-zero on disagreement.

## 5. What would graduate this from spike

1. `weftmesh::close` called from the manager at run end; root written
   into the run's EXPERIMENTS entry (the DOCTRINE anchor).
2. The TS side: `weft_export` emitting the Weft as MerkleMesh JSONL (the
   cross-pollination doc's original wiring sketch) so MerkleMesh can
   mesh fabric-roots the way it meshes boat journals — one root of
   roots. Requires agreeing a canonical JSON form for TickSig, i.e.
   walking straight into the ryū hazard MerkleMesh already mapped; their
   `canonical.ts` is the map.
3. Incremental root updates for live (non-closed) runs, if ever needed —
   not built; the close-time root is the doctrine-clean version.

Root of this run's corpus, for the record: the full root of the
10,000-seed corpus (seeds 1..=10000, v0 pipeline) is deterministic
across runs:

```
c869701229f831d5b18860fe9b075d37f5701bd7172189e54ae15b54bdf139ee
```

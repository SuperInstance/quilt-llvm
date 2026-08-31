# CROSS-POLLINATION.md — org-wide survey

Scout pass 2026-08-30 (retry 3; a partial draft from a dead attempt was
verified and extended — every file citation below was re-opened this pass).
Scope: the SuperInstance org (`gh repo list`, 400 cap) plus live local trees
under `/home/eileen/projects/` and shallow clones under `/tmp/scout/`.
Excludes repos already covered by prior scouts (quilt-verilog, quilt-scratch,
quilt, quilt-rust, zeroclaw-dissertation, edge-compiler, ternary-spiral,
tit_quilt_elixir, quilt-conformance, quilt-opt, quilt-linker, batten-spline
as a prior-scout subject). Undersell is the rule: everything below cites
files I actually opened; nothing is claimed to work until wired and measured.

Note: the task brief mentioned `wave-*` repos — no such repos exist in the
org listing (closest: elephant's `scripts/wave3_s4_analyze.py`,
`scripts/wave4_s1_pilots.py`, which are experiment-journal scripts, not a
wave-* repo family).

Directions: **in** = machinery quilt-llvm/quilt-scratch can absorb;
**out** = quilt-llvm's new tech (Weft hash-chained ledger, conservation
verifier, provenance walks, TA two-tier lesson store, batten kernel) that
older repos would benefit from; **both**; **none** = honest nothing.

## Survey table

| Repo | What exists (files opened) | Direction |
|---|---|---|
| MerkleMesh (TS) | Dependency-free merkle aggregation + inclusion proofs over quilt cell-ledger JSONL journals; `src/ledger.ts`, `src/mesh.ts`, `src/canonical.ts`, `src/sha256.ts`; 49 tests, "bit-for-bit Rust-compatible TS port" | **both** |
| superinstance-cocapn (Rust) | Fleet conservation audit: per-ship `ConservationState{γ,η,c}`, fleet-wide `Σγ+Ση=C` balance check, routing (`src/cocapn.rs`, `src/routing.rs`, `src/types.rs`; clone in /tmp/scout) | **both** |
| tit-quilt (py) | Terminal-as-graph: cells, `witness[]` on every value return, **provenance-integrity law — `FORGET` tombstones, never deletes** (`tit_quilt/engine.py`, `store.py`, `cells.py`; README §laws) | **both** |
| cuda-constraint-engine (CUDA/C) | Batched bounds/equality constraint checking on GPU, INT8–FP64, async streams, CUDA graphs; `include/constraint_engine.h`, `src/` (clone in /tmp/scout). "1B+ constraints/sec" is the repo's own pitch — unmeasured by us | **in** (measure first) |
| mud-arena (py) | Agent gym with a **genetic algorithm engine**: `src/evolve.py` (575 lines, `class EvolutionEngine` with `initialize/evaluate/select`, `class Script` fitness objects) — no dedicated test file found for it; tournament scoring in dashboard | **in** (surprise — see #4) |
| ta / router (py, local) | TA thought-engine + router: `model_selector.py`, `confidence.py`, `boundary_tracker.py`, `cloud_cascade.py`; two-tier lesson store; 551 tests | **both** |
| saddle (TS) | Double-entry bookkeeping per cell, append-only outcome ledger, frozen alignment states; `src/canonical.ts`, `src/cells.ts`, `src/hash.ts`, `src/frozens.ts` | **both** |
| quilt-pincher (TS) | Reflex engine from quilt cells; **three-tier compute table** (Cloud <50ms LLM-compile / Workstation / ESP32 no-LLM, `README.md` §tiers); "LLM as compiler" = compile once, execute zero-LLM; `src/{core,cells,adapters,platforms}` | **both** (light) |
| quilt-substrate (py) | The ancestor layer: fog-of-war decay + `DeckhandWitness` JSONL witness log (`src/quilt_substrate/substrate.py`, `plugins/deckhand_witness.py`) — witness has **no hash chain** (grep: 0 hits) | **out** |
| conservation-enforcer-rs (Rust) | 282-line `src/lib.rs`: budget/replenish model, `EnforcementResult{allowed,blocked}`, policy files — output-side only, policy bytecode is a placeholder | **out** (we're ahead) |
| fleet-twin / fleet-embed | Vectorize index over fleet corpora (Workers+Vectorize) and local Candle embedding server (OpenAI-compatible) | **out** |
| murmur-agent (TS) | All-night thinking git-agent; every thought a commit; knowledge tensor | **out** (light) |
| quilt-mhs (Rust) | Quilt runtimes as/exposing MHS devices; conformance suite culture, `crates/`, `PORTING.md` | **in** (test culture) |
| quilt-deck (local) | Three-backend cosim; `cosim/COSIM-VERDICT.md` + README labels the iverilog path **"EXPERIMENTAL / UNVERIFIED: cosim does not complete locally (measured 2026-08-30)"** | **in** (doc pattern) |
| ternary-tenforward (Rust) | Beat-based simultaneous multi-agent dialogue, Z₃/RPS/Fibonacci-Pisano theory; 102 inline `#[test]`s in `src/lib.rs` | **none** for a compiler; culture ok |
| flow-state (py) | `SplineObserver` — directory watcher extracting entropy/complexity feature traces (`flow_state/observer.py`, `models.py`) | **in** (light; maybe) |
| elephant (py) | Experiment-journal repo: `scripts/wave3_s4_analyze.py`, `wave4_s1_pilots.py`, slope regressions, tap-nights scripts | **none** |
| luciddreamer-ai (TS Worker) | Autonomous 30-min writing cycle, ranked stream, persistent knowledge graph | **none** |
| shoal (JS) | "Conservation-bounded semantic search" but the tree is prompts/`out/*.raw` artifacts + `test/smoke.js`; early | **none** (watch) |
| git-native-mud (py) | World-as-repo, commits-as-actions, immutable history | **none** (kin philosophy; no wiring worth its cost today) |
| fleet-functions / fleet-discovery | Semantic capability search; falsification-driven research wheel | **none** (no direct wiring) |
| fleet-homunculus, plato-portal, quilt-engine-ports, SmartCRDT, AgentGossip | Reviewed at README level; no compiler-relevant machinery found | **none** |

Also checked locally (may be ahead of GitHub): `quilt-deck`, `the-tap`,
`mud-arena`, `batten-spline`, `ta`.

## Top 5 opportunities, file-level wiring sketches

### 1. MerkleMesh × Weft — one root for every fabric journal (both)

**In:** MerkleMesh already verifies hash-chained JSONL cell-ledgers and
proves inclusion with sibling paths (`src/mesh.ts`). Weft
(`experiments/llvm-fabric/src/sign.rs`, b7f1ddf: fabric signature every tick,
hash-chained) emits exactly that shape per pipeline run.
Wiring: a `weft_export` dump in `experiments/llvm-fabric` that writes each
run's Weft chain as MerkleMesh JSONL; import `MerkleMesh/src/canonical.ts`
byte rules into a Rust-side test so the export is bit-compatible (the repo
claims an existing Rust-compatible TS port — verify, don't assume).

**Out:** quilt-llvm's canonical-hash discipline (reachability of every claimed
hash in git log, per DOCTRINE.md) is the pattern MerkleMesh's "one fleet, one
root" pitch wants for CI: a daily fleet root over all quilt-family journals.

Risk: canonicalization drift between TS and Rust is the classic silent
corruption bug; needs a cross-impl fixture suite before any root is trusted.

### 2. tit-quilt tombstones × M4 DCE-as-decay (both)

tit-quilt already shipped the law M4 wants: "Provenance integrity law.
Nothing witness-referenced is ever destroyed. `FORGET` never deletes: it
tombstones — cell identity, version, witness" (`tit_quilt/engine.py`,
README §laws). quilt-llvm's open M4 (DCE-as-decay, load-bearing) should
absorb the **tombstone record shape** before designing its own: DCE'd cells
become tombstones carrying (id, version, last-witness-refs), retrievable by
provenance walks (`experiments/llvm-fabric/src/prov.rs`).

**Out:** tit-quilt's `witness[]` on every value return is pre-Weft — plain
references, no chain. Wiring: hash-chain the witness log the way
`sign.rs` chains tick signatures; a ~30-line change to `store.py`'s append
path plus one integrity test.

Risk: tit-quilt is a fast-moving prototype; coordinate or accept churn.

### 3. superinstance-cocapn × conserve.rs — conservation law, fleet scale (both)

**In:** cocapn's `Σγ + Ση = fleet_C` audit (`src/cocapn.rs`) is the same law
quilt-llvm enforces per-fabric in `experiments/llvm-fabric/src/conserve.rs`
(delivered or dropped-with-ledger-entry). Absorb the *tolerance + health
state* vocabulary (Healthy/Degraded/Down) for verifier reporting.

**Out:** make `conserve.rs` emit per-pass conservation ledger entries in
cocapn's `ConservationState{γ,η,c}` shape so a compile pipeline can register
as a cocapn "ship"; the Cocapn gains a compile-fleet-wide balance check for
free. File-level: a small `impl From<ConservationEntry> for …` mapping, or a
JSON sidecar if we keep crates decoupled.

Risk: cocapn is Python-era vocabulary on a Rust skeleton; check it compiles
before promising interop.

### 4. mud-arena's genetic engine × fuzz.rs — evolving the fuzz corpus (in, surprise find)

The surprise: mud-arena isn't just a game gym — it ships a genetic engine,
`src/evolve.py`: `EvolutionEngine.initialize/evaluate/select` over `Script`
rule-vectors (575 lines; caveat: no dedicated tests found for it — treat as
unverified machinery). `llvm-fabric/src/fuzz.rs` currently seeds random fabrics
(10k corpus, 255k cells walked, xorshift PRNG). Wiring: breed fuzz programs toward *verifier failures* —
fitness = (pass crashed? conservation violated? verifier disagreement between
passes?) via tournament-style selection over `gen_fabric` seed/parameter
genes. This is coverage-guided fuzzing built from parts the fleet already owns.

Risk: breeding toward crashes finds shallow bugs first; cap generations and
log every fitness number. Honestly labeled experiment.

### 5. batten-spline / ta lesson store × batten kernel (both)

**In:** batten-spike (`experiments/batten-spike/src/kernel.rs`) reimplemented
a ~40-line Nadaraya–Watson RBF kernel rather than depending on batten-spline.
Compare against the real package (`batten-spline` py, `kernel` module) on the
same 800/200 corpus; if parity holds, cite it; if it beats the toy, port the
delta.

**Out:** batten-spike's verified-outcome routing (outcome zeroed if output
fails `verify.rs`) is exactly the ground-truth discipline ta's router lesson
store (`ta/router/`, two-tier lessons) lacks — lessons there are model-judged,
not verifier-judged. Wiring: a `ta/router/lesson_import.py` path that accepts
batten-spike run records (`run-output.txt` JSONL) as tier-1 verified lessons.

Risk: batten-spline is pip-installed Python; kernel parity claim must be a
test, not a sentence.

## Measure-first bench (real but not top-5 yet)

- **cuda-constraint-engine × verify.rs**: serialize `verify.rs` value-range
  checks as batched bounds uploads (`ce_upload_bounds_i32`); structural checks
  stay on CPU. Only pays at large batch — the serial verifier may already be
  fast enough. Toy first, measure, per DOCTRINE. First target: batten-spike
  fuzz runs (1000 fabrics/run).
- **conservation-enforcer-rs**: 282 lines, output-side-only, placeholder
  bytecode. Out-direction: quilt-llvm's ledger-entry-on-drop vocabulary
  (`conserve.rs`) is strictly richer than its `allowed/blocked` result; a
  port-upgrade note or PR is cheap goodwill. In-direction: its
  budget/replenish API (`remaining_budget`, `replenish_budget`) is a tidy
  shape for verifier budget cascades.
- **quilt-substrate `DeckhandWitness`**: no hash chain (verified by grep).
  Out-direction: same Weft retrofit as tit-quilt #2 — pick one, prove the
  pattern, then offer it to the other.
- **quilt-pincher**: its three-tier table (Cloud/Workstation/ESP32, LLM at
  compile-time only) is the vocabulary batten-spike's verification cascade
  (cheap-tier vs full-walk) should adopt when documenting tier boundaries.
  Out-direction: conservation-checked reflex caches (a `conserve.rs`-shaped
  gate on pincher's LLM-compiled reflex entries).

## Doc/test culture worth copying (no wiring)

- **quilt-deck**: "EXPERIMENTAL / UNVERIFIED" with measured date and failure
  mode right in the README + a dedicated `cosim/COSIM-VERDICT.md` — best
  honest-labeling example in the org.
- **quilt-mhs**: conformance-suite-as-product (`PORTING.md`) pattern for the
  day quilt-llvm grows backends.
- **ta**: 551 tests, fresh-venv-zero-installs quickstart claim — the bar for
  our TUTORIAL.md.
- **ternary-tenforward**: theory-heavy README, but 102 inline tests in
  `src/lib.rs` back the "experiments proved" claims better than most.

## Attribution

Extra pass run via `claude -p` in tmux (session `scout-xpoll`), 2026-08-30:
verified 5/5 local file citations it sampled (`fuzz.rs`, `verify.rs`,
`conserve.rs`, `kernel.rs`, `prov.rs` — all exist and match descriptions);
flagged 2 overclaims, both folded in above (cuda perf number now attributed
to the repo's pitch; mud-arena GA engine now cited to `src/evolve.py` with
its missing-tests caveat). It confirmed the "none" verdicts stand.
All other surveying done directly this pass.

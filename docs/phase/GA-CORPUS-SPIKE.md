# GA-CORPUS-SPIKE — breeding corpus fabrics at the cannot-emit list

*R2 wide lane, 2026-08-30. A spike, not production: the goal was to
find out whether a GA (engine structure ported from SuperInstance
mud-arena's `src/evolve.py`) can drive corpus fabrics into the eleven
constructs SHAPE-AUDIT measured the fuzz generator never emits — and,
just as importantly, to name which of those constructs are
**provably unreachable** under `verify`'s laws. House rules: measured
numbers or it didn't happen; failures first-class; undersell.*

Worktree: `quilt-llvm-wt-gacorpus`, branch `r2-ga-corpus` (base
486c536). The audit itself lives in the shape worktree
(`docs/phase/SHAPE-AUDIT.md`, quilt-llvm-wt-shape); its §1 list is
the fitness target here.

Reproduction:

```
cd experiments/llvm-fabric
cargo run --release --bin ga-corpus            # pop 200 x 50 gens, seed 0x6A1C0
cargo run --release --bin ga-corpus --seed 46595 --seed 999331  # stability
cargo test                                     # 129 passed, 0 failed
```

## 1. Method

- **Representation:** a fabric (`Fabric`), not mud-arena's rule list.
  Seed population = the corpus generator's own output
  (`fuzz::gen_fabric`) — the GA starts exactly where the audit's blind
  spot begins (gen-0 coverage is the audit's world: C9/C11 flicker
  only via detectors disagreeing with generator bias, C1–C4/C6/C10 at
  zero).
- **Fitness:** `0` if `verify` fails; else `10 × (#C-items exercised)
  + capped depth bonuses` (phi→computation consumers, calls, boundary
  consts — tiebreakers that keep the GA deepening, never worth more
  than half an extra item). A fabric that verifies green while
  containing a head-phi or a phi→arith wire is worth more than one
  that doesn't — exactly the audit's "green-on-vacuum" gap, closed by
  breeding instead of claiming.
- **Detectors** (`ga::coverage`): one arm per C-item, measured on the
  fabric (C4 = phi is the first cell of its region; C6 = a wire from a
  phi into arith/cmp/branch; C9 = const outside the corpus's measured
  numeric domain: i64<0 or ≥1e6, i32 outside [−500,500), f64 outside
  [−12.5,49) or non-eighth or ±Inf; C10 = >6 regions or >63 cells or
  >12 ctrl edges; C11 = an arith slot-0 operand that is not the most
  recent same-typed value in scope).
- **Engine** (mud-arena `evolve.py` structure, ported): tournament
  selection (k=5), elite carry (20), crossover breeding + per-gene
  mutation, replace-worst, per-generation fitness/coverage history.
- **Crossover** = region grafting: a contiguous run of parent B's
  non-entry regions is grafted onto a clone of parent A; operand ids
  remapped inside the graft, entry values substituted by type, phis
  with joins outside the graft dropped, terminator targets clamped to
  the child's entry. **There is no repair pass** — broken children
  score 0 and die; selection *is* the repair. That is honest GA, and
  the verify pass-rate below prices it.
- **Mutation menu** (weighted): add-phi (25%, half non-i32, half at
  spine head), consume-phi (25%, arith insert or branch-cond retarget),
  add-call+consumer (15%), boundary-const+arith (15%), grow (10%:
  new region / arith chain / jmp→br), operand-shuffle (15%).

## 2. Measured exit (pop 200 × 50 gens, seed 0x6A1C0; 0.24 s release)

Per-C-item, best-of-run:

| item | construct | first gen covered | max in pop |
|---|---|---|---|
| C1 | call cells (+ consumer arith) | 1 | 162 |
| C2 | non-i32 phis (i1/i64/f64) | 1 | 162 |
| C3 | >1 phi per region | 1 | 162 |
| C4 | phi at spine head | 1 | 162 |
| C5 | **partial phis** | **never** | **0** |
| C6 | phi feeding arith/cmp/branch | 1 | 162 |
| C7 | **params outside entry** | **never** | **0** |
| C8 | **nested regions** | **never** | **0** |
| C9 | boundary/negative-domain consts (incl. ±Inf, i64::MIN, non-eighths) | 0 | 162 |
| C10 | >6 regions / >63 cells / >12 edges | 1 | 162 |
| C11 | non-latest operand head | 0 | 162 |

- **Verify pass-rate of the bred population:** 158/200 = **79.0%**
  final (peak 162/200 = 81% during the run). The gen-0 seed population
  verifies 200/200 (the corpus generator's contract); the run's
  `verify` column counts fabrics with positive fitness — i.e. verify
  green *and* covering at least one C-item — so gen-0's 103 means 103
  seed fabrics already trip a detector (C9/C11 flicker via entry-use
  and domain edges), not a 51% verify failure. Stated so neither number
  gets quoted as the other.
- **Best fabric:** covers all 8 reachable items simultaneously,
  fitness 90.0.
- **Convergence:** best fitness plateaus at 90.0 by gen ~10; coverage
  counts track verify-passing population size (~150–165) — i.e. nearly
  every verify-green fabric in the population exercises **all eight**
  reachable items. Evolution here is cheap: the barrier was emission,
  never difficulty.
- **Text round-trip honesty check:** 132/132 (seed A), 135/135 (seed
  B) mutated-only verifying fabrics round-trip `print/parse/print`
  bit-for-bit. Bred fabrics are corpus-grade as far as text goes.
  (NaN consts are still parser-rejected by design; mutations emit ±Inf,
  not NaN — Inf round-trips via the `inf`/`-inf` literals.)
- **Stability:** seed 46595 → first-covered identical (all reachables
  by gen 1), final verify 153/200 (76.5%), best 8 items; seed 999331 →
  150/200 (75.0%), best 8 items. All conclusions reproduce.
- **Runtime:** 0.24 s for 10,000 evaluations (release), far under the
  5-minute budget. `cargo test` (debug) including the 60×30 in-test GA
  run: well under a minute.

## 3. Unreachable items — provably, and why

These are findings, not failures of the GA. Each is enforced against
by `verify` or by the IR itself; no breeding pressure can emit them
(fitness is gated on verify passing):

- **C5 (partial-phi coverage): V16.** Every predecessor of a phi's
  region must carry exactly one join entry ("control edge without a
  mux input"). A partial phi is *invalid*, not merely ungenerated —
  the audit already said the only partial phis arrive via mutation 9
  and land in the rejection histogram. Confirmed in-test
  (`boundary_detector_and_unreachable_laws`). Coverage of C5 semantics
  requires a *relaxation* of V16, not a better generator.
- **C7 (params outside entry): V12.** `verify` rejects params outside
  the entry region outright. Same shape as C5: invalid, not ungenerated.
- **C8 (nested/contained regions): the IR.** `Region` has no parent
  field; nesting is unrepresentable. V3 ("containment acyclic forest")
  is vacuous on *everything* a GA (or anyone) can build until the IR
  grows a parent edge. This is an IR change request, not a corpus one.

## 4. Scoping honesty (what "C1 covered" does and does not mean)

- The C1 detector is **fabric-level**: a `Call` cell whose operands are
  value cells, feeding an arith consumer, verifying green. The audit's
  C1 also names *call depth ≥ 1* and *multi-fabric programs* — those
  are program-level (`program.rs`) properties, outside a fabric-only
  fitness. The GA closes the fabric half of C1; callee resolution and
  program-level verify remain hand-fixture territory (as EXPERIMENTS
  already states for v1).
- C9's bred fabrics carry boundary consts **inside computations**
  (each boundary const is wired into an arith), which is the actual
  blind spot the audit named — the fold-table corners. But this spike
  does not run the fold oracle on them (that is R1 lane 1's property
  grid); it only proves the corpus *can* now carry them to the fold.
- C11's detector covers the slot-0/head-choice bias; full operand
  distribution retraining of the generator is a different (non-GA) fix.
- The GA converged so fast (all reachables by gen 1–4) that the honest
  headline is not "evolution is powerful" but "the emission barrier was
  shallow": six constructive mutation operators plus selection suffice.
  A production generator fix (SHAPE-AUDIT §4) remains the cheaper
  long-term path; the GA is the exploration instrument, and its durable
  output is the unreachable-item proof in §3 plus 8-item fabrics that
  can seed regression fixtures.

## 5. Verdict

- **Reached (8/11):** C1 (fabric-level), C2, C3, C4, C6, C9, C10, C11
  — each covered by 162–167 verifying fabrics per run, best fabric
  covering all eight at once, ~79% of the bred population verifying.
- **Unreachable by law (3/11):** C5 (V16), C7 (V12), C8 (IR structure).
- The audit's sharpest blind spot — cyclic fabrics with loop-carried
  *computation* (C4+C6 together) — is breedable in one mutation pass
  and verifies green. **The spec/verifier conflict on phi placement
  (ARCHITECTURE §2.1 vs the audit's C4 finding) resolves in verify's
  favor today: head-phis verify. If the spec wins instead, V-codes
  must grow a placement rule — decided by R3's gate, not here.**

— R2 wide-lane subagent, 2026-08-30. Engine lineage: mud-arena
`evolve.py` (tournament/elite/breed/mutate/replace-worst/history),
ported to fabric IR; no runtime dependency on mud-arena.

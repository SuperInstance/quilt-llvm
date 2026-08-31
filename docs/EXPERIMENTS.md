# EXPERIMENTS — llvm-fabric v0 spike + v1 lane

**What this is:** the experimentation leg of the keel (README.md). The keel
claims a cell-model IR with inspectable history is worth building. This
spike tests those claims with running code. Everything below was measured
on this machine (WSL2, rustc 1.97.1, debug build unless noted). Every
claim cites the test or command that produced it. Where something is a
toy, it says toy.

**Layout:** §1–6 are the v0 spike as measured at f9dcd85 (kept verbatim —
the doc's own N4 law; where v1 changed the facts, the v1 section says
so, and stale v0 claims carry a pointer). §7 is the v0 ledger. **§8+
is the v1 lane** (control wires, inlining, the Weft) — the current edge.

Code: `experiments/llvm-fabric/` (zero-dependency Rust crate).
Run everything yourself:

```
cd experiments/llvm-fabric
cargo test                          # 99 tests, all green (was 64 at f9dcd85)
cargo run --release -- fuzz         # 10,000-fabric corpus (now with phis)
cargo run --release -- bench        # size/serialize/signature numbers
cargo run --release -- pipeline examples/foldme.fabric
cargo run --release -- inline examples/inlineme.v1fabric   # v1 lane
```

---

## 1. What was built

| module | lines (incl. tests) | what it is |
|---|---|---|
| `src/fabric.rs` | 267 | the fabric: region/cell slab, stable never-reused ids, `wires()` use edges |
| `src/cell.rs` | 133 | cell kinds: param / const / arith / cmp / branch / jump / phi (wire join) / ret |
| `src/text.rs` | 383 | canonical text format + parser with line-numbered errors |
| `src/verify.rs` | 623 | verifier, codes V00–V15, one unit test per code |
| `src/fuzz.rs` | 554 | seeded PRNG, valid-by-construction generator, 9 mutations, corpus harness |
| `src/diff.rs` | 199 | N4 history: machine-applicable edits, ledger entries, epoch numbering |
| `src/conserve.rs` | 146 | conservation law as a mechanical check |
| `src/replay.rs` | 176 | edit applicator with tamper detection |
| `src/passes/constfold.rs` | 347 | constant folding (arith/cmp; cascades; checked folds) |
| `src/passes/dce.rs` | 168 | dead-code elimination (terminator-rooted liveness) |
| `src/prov.rs` | 232 | provenance walk + transform provenance (121 lines excl. tests) |
| `src/pipeline.rs` | 105 | fold → dce → fold → dce, history per pass |
| `src/bench.rs` | 342 | toy shapes + plain-SSA baseline + measurement |

4,045 lines total, ~1/3 of that tests. `cargo test`: **64 passed, 0 failed**
(run `cargo test` in `experiments/llvm-fabric/`).

Design law held throughout: **ids are never reused**. Removed cells leave
`None` holes in the slab; history and provenance still refer to them.

### The IR in one glance

```
fabric v0
region entry
  %0 = param i32
  %1 = const i32 42
  %2 = arith.add i32 %0, %1
  %3 = cmp.lt %2, %1
  %5 = br %3, then, else
region then
  %4 = const i64 1i64
  %6 = jump join
...
region join
  %9 = phi [then: %4] [else: %7]
  %10 = ret %9
```

Every cell — terminators included — carries its explicit id, so
print→parse→print is the identity (`text::tests::print_then_parse_then_print_is_identity`)
and removed-cell ids never silently shift (`text::tests::ids_survive_holes_on_roundtrip`).

---

## 2. Verifier + fuzz corpus

Codes V00–V15, each with a unit test that constructs the exact fault
(`verify::tests::v00_no_regions` … `verify::tests::v15_ret_of_non_value`).
The corpus invariant: **verify either accepts or rejects with a precise,
non-empty reason — never panics.**

`cargo run --release -- fuzz` (seed 0xFAB1C), 10,000 iterations:

```
iters:                 10000
valid (by generator):  10000
cells provenance-walked: 239691
roundtrip failures:    0
prov failures:         0
replay failures:       0
panics:                0  (a panic crashes this process; 0 here means none)
mutated:               4540
  still valid:         1370
  rejected:            3170 (by code)
    V01 1323
    V03 644
    V04 675
    V08 191
    V09 38
    V10 34
    V11 232
    V12 28
    V15 5
```

Honest notes:
- 1,370 of 4,540 mutated fabrics were **still valid** — mutations are
  corruptions, not guarantees of invalidity. Counted, not hidden.
- **v1 correction (see §7.2):** the v0 generator NEVER generated a phi —
  the phi step consulted `predecessors()` before terminators existed,
  so it always saw "no predecessors." The 239,691 walks above never
  touched a phi; the phi-relevant codes (V05/V06/V14) were exercised
  only by unit tests. Fixed in v1; the corpus numbers above are
  historical.
- The full sweep (verify + text roundtrip + per-cell provenance + 4-pass
  pipeline + replay + conservation) runs in ~2.7 s debug / ~0.6 s release
  for 10,000 fabrics.
- The corpus **found a real verifier gap**: `ret` originally accepted
  terminator operands (no type check rejects them). The provenance check
  tripped on it (seed 1026844); the fix became code V15 with its own
  test. This is the fuzz-to-verifier loop working as intended.

---

## 3. The two passes

Both are pure functions `fabric → (fabric, diff)`, refuse unverified
input, leave verified output, and hold the conservation law
(`conserve::check` after each).

**Red/green discipline.** Tests assert specific transforms, so they fail
if the pass does nothing. Verified empirically at commit time: replacing
each pass body with `return Ok((f.clone(), DiffRecord::new(...)))` made
exactly these five tests fail —
`green_folds_add_and_cascades`, `green_folds_cmp`,
`folds_inside_phi_operands_stay_in_scope`, `green_removes_dead_keeps_live`,
`dead_phi_goes_its_operands_follow` — while the identity-expectation and
refuse-unverified tests stayed green. (Method recorded in commit
c05cc84's message.)

A fold in action (`cargo run -- pipeline examples/foldme.fabric`):

```
epoch 0 pass constfold
  + %7 @ entry[2] :: %7 = const i32 42
  ~ %4.0: %2 -> %7
  ~ %6.0: %2 -> %7
  - %2 (folded into %7) :: %2 = arith.add i32 %0, %1
  + %8 @ entry[4] :: %8 = const i1 true
  - %4 (folded into %8) :: %4 = cmp.lt %7, %3
epoch 1 pass dce
  - %0 (dead: no path to a terminator) :: %0 = const i32 20
  ...
== final fabric ==
fabric v0
region entry
  %7 = const i32 42
  %6 = ret %7
final verifies: yes
conservation: holds
```

v0 scope, stated plainly:
- const folding covers arith and cmp only. **Const branches are not
  folded**: replacing a branch with a jump changes predecessor sets, so
  phis in the not-taken region and downstream must drop joins — CFG
  surgery deferred to v1.
- integer folds are checked; overflow, `INT_MIN / -1`, and divide-by-zero
  skip folding (`overflow_skips_fold`, `div_by_zero_const_is_left_alone`).
- DCE roots at terminators; **unreachable regions are not removed**
  (their terminators count as roots). Region-DCE is v1.
- No NaN anywhere: NaN literals are unparseable and NaN-producing folds
  are skipped. This is a text-format limitation, not a principled choice.

---

## 4. The experiment

### (a) Provenance walk

From any value, reconstruct its full def chain by walking operand wires
backwards. Implementation: 121 lines excluding tests (`src/prov.rs`).

`cargo run -- prov examples/demo.fabric 9`:

```
%9 = phi [then: %4] [else: %7]
  %4 = const i64 1i64
  %7 = const i64 2i64
```

Measured on the corpus: **239,691 walks (every cell of every valid fabric),
100% passed `check_prov`** (terminates, every leaf is a param/const, no
invented cells). `fuzz` output above; enforced by
`fuzz::tests::corpus_400_iters_no_panic_all_invariants`.

The honest comparison, in two parts:

1. **Use-def walking is easy in LLVM too.** `llvm::Value::operands()` in
   C++ (or %names in .ll text) gives you the same walk. We do not claim
   otherwise. A recursive printer over LLVM operands is comparable in
   size to our 121 lines (not measured — no LLVM build in this lane;
   booked as a gap).
2. **What LLVM IR does not carry: the value's history.** Our history
   answers "tell me about %2" *after %2 has been folded away*:

```
prov_history(%2) = [(0, "constfold", "removed (folded into %7)")]
prov_history(%5) = [(1, "dce", "removed (dead: no path to a terminator)")]
```

   Tested in `prov::tests::history_provenance_tracks_dropped_cells`.
   Textual .ll has no pass ledger; `llvm::Value` has no record of which
   pass created or deleted it. That is the actual differentiator, and it
   falls out of N4 rather than needing a provenance subsystem.

**Surprise (booked):** data provenance does not cross control edges. The
phi's walk above never reaches `%0` (the param feeding the branch
condition), because the branch is control, not a data wire
(`prov::tests::provenance_of_phi_reaches_all_roots` asserts exactly this
split). For "everything that influenced this value," v1 needs explicit
control wires.

### (b) Pass-history replay

Run the 4-pass pipeline, then replay the history over the original
fabric and compare every intermediate stage.

Measured on the corpus: **10,000 / 10,000 fabrics replayed with all 5
stages bit-identical** — structural equality (`PartialEq`) *and*
canonical-text equality — 50,000 stage comparisons, zero divergences
(`fuzz` output: `replay failures: 0`; per-fabric test:
`pipeline::tests::replay_reproduces_every_stage_bit_identically`;
CLI: `cargo run -- replay examples/foldme.fabric` →
"5 stages reproduced bit-identically").

Replay also **detects tampering**, because edits are validated on apply:

- forged cell id → "AddCell %99 is not the next free id (2) — history is
  forged or out of order" (`replay::tests::forged_id_is_rejected`)
- edited retarget → "Retarget: %1.0 is %0 but history says %9 — history
  does not match the fabric" (`replay::tests::tampered_retarget_is_rejected`)

The cost, measured (see table in (c)): history bytes scale with **work
done, not result size**. foldchain-400: 21.8 KB in → 66 B out, 105.6 KB
of history. N4 is not free; it is a deliberate trade (auditability for
bytes). v1 needs checkpointing/compaction to bound it.

### (c) IR size and serialize time vs plain SSA text

`cargo run --release -- bench` (release build; medians of 21 runs):
"base" = our minimal LLVM-.ll-flavored printer for the same fabric — a
format comparison, not an implementation comparison.

```
shape                cells   fabric-B  base-B  ratio  print-us  baseprint-us  parse-us  verify-us
chain-50                53      1558    1184   1.32      7.3          6.8      14.0       1.3
chain-200              203      6263    4838   1.29     26.6         24.6      48.1      14.1
chain-800              803     25463   19838   1.28    112.6        100.8     236.4     170.0
diamonds-10             93      2452    2261   1.08     10.2          8.7      29.1       3.7
diamonds-40            363     10301    9512   1.08     40.4         34.2     125.2      42.6
diamonds-160          1443     42970   39633   1.08    163.4        136.9     647.4     628.3
dag-50                  51      1523    1163   1.31      7.3          6.9      14.6       1.5
dag-200                203      6454    5029    1.28     28.3         27.7      53.2      28.4
dag-800                803     26556   20931   1.27    110.0        106.3     233.2     228.0

history overhead (4-pass pipeline on foldchain-N):
shape          orig-B  final-B  history-B  hist/final
foldchain-20        1059       60       4926        82.1
foldchain-100       5342       64      25632       400.5
foldchain-400      21842       66     105643      1600.7
```

And real LLVM text sizes for the same shapes, via llvmlite
(`tools/llvm_ll_bytes.py` — byte sizes only; timings across
implementations are not comparable):

| shape | fabric-B | plain-SSA base-B | real LLVM .ll B | fabric / LLVM |
|---|---|---|---|---|
| chain-50 | 1,558 | 1,184 | 1,577 | 0.99× |
| chain-800 | 25,463 | 19,838 | 24,733 | 1.03× |
| diamonds-160 | 42,970 | 39,633 | 40,778 | 1.05× |

Readings, undersold:
- The fabric text is 1.08–1.32× our minimal baseline and roughly
  **parity with real llvmlite .ll output** (caveat: llvmlite quotes
  value names, inflating LLVM's side slightly). The explicit
  `region`/`fabric` vocabulary and `%id =`-on-terminators cost ~25–30%
  over minimal SSA text on straight-line code, ~8% on CFG-heavy code.
- Printing is within ~15% of the minimal baseline (same code path
  overheads), and parsing is ~2× printing (tokenizing + slab placement).
- **Verifier is superlinear**: 1.3 µs at 53 cells → 170 µs at 803 cells
  (~130× for 16× cells). `uses_of`/`predecessors` are linear scans.
  v1 debt: maintained use lists and pred/succ tables → O(n) verify.
- These are single-machine, debug-crates numbers; treat as orders of
  magnitude. `bench::tests::shapes_verify` guards the shapes themselves.

---

## 5. Failures, limitations, surprises (first-class)

1. **Verifier gap V15** (ret accepted non-value cells) — found by the
   corpus, fixed with a test. The good kind of failure.
2. **Data provenance doesn't cross control edges** — a phi's def chain
   never reaches the branch condition that selected it. v1: explicit
   control wires.
3. **Const-branch folding deferred** — needs phi-join maintenance when
   predecessor edges disappear. The pass framework has no region-edit
   vocabulary yet; that's the real v1 blocker for branch folding and
   region-DCE alike.
4. **History cost scales with work**: up to 1,600× the final fabric's
   bytes (foldchain-400). Not a bug — the conservation ledger IS the
   feature — but it needs compaction/checkpoints before anything long
   runs.
5. **Verifier O(n²)-ish** on linear scans (measured above).
6. **30% of mutations left fabrics valid** — the mutation suite is
   blunt; fine for panic-hunting, not a typed-negative corpus.
7. **v0 scope rules are stricter than dominance**: non-phi uses may only
   see same-region-earlier or entry values; phi operands must be defined
   in the join region or entry. This rejected legitimate
   dominating-block uses (the diamonds shape couldn't thread values
   between diamonds and had arms recompute from the entry param — visible
   in `bench::diamonds`). Real dominance analysis is v1.
8. **NaN absent by design** — unparseable literal, folds skipped.
9. **No float `Eq` caution**: `PartialEq` on f64 means `0.0 == -0.0`;
   "bit-identical" claims are structural-equality claims, stated as such.

---

## 6. What v1 should change

Ordered by what the experiments actually showed:

1. **Region/edit vocabulary in diffs** (RegionAdded/Removed,
   JoinDropped) — unlocks const-branch folding and region-DCE, the two
   biggest v0 pass gaps.
2. **Control wires** — make "what influenced this value" include control,
   making provenance complete for CFG-shaped code.
3. **Maintained use/pred tables** — O(n) verification; the measured
   superlinearity will bite real programs.
4. **History compaction** — checkpoint epochs (full-fabric snapshots)
   with deltas between; bound memory while keeping the ledger law.
5. **Dominance-based scope rules** — replace the entry-only exception
   with real dominance, unblocking natural CFG shapes.
6. **A real negative corpus** — typed invalid fabrics per code, instead
   of (only) random mutations.
7. Then, and only then, more passes: inlining on fabrics, GVN-lite. The
   keel lists them; nothing here earns them yet.

## 7. Ledger

- All commits under `experiments/llvm-fabric/` + this file; reachable in
  `git log --oneline` (8c14c29 → 8d21e4f at time of writing).
- `cargo test` green before every commit (64/64 at HEAD).
- Nothing deleted; archives are renames only (none needed yet).

---

# Appendix B — batten-spike: verified-outcome routing over pass pipelines (2026-08-30)

Code: `experiments/batten-spike/` (zero-dependency Rust crate + pinned
vendored copy of llvm-fabric @ `2e5469e` — the live llvm-fabric tree had
uncommitted WIP at spike time and did not compile; the pin keeps this
reproducible). Run it: `cd experiments/batten-spike && cargo test` (13
green) && `cargo run --release` (prints everything below; full output
also at `experiments/batten-spike/run-output.txt`).

## B.1 Setup (all toy, labeled)

- **Corpus:** 800 train / 200 test fabrics from llvm-fabric's seeded fuzz
  generator (disjoint seed ranges: 1–800, 100000–100199).
- **Pipelines:** `none`, `fold`, `fold>dce`, `dce>fold`, `full`
  (= fold>dce>fold>dce, the llvm-fabric default).
- **Cost (toy):** cells processed = sum of input cell counts per pass run.
- **Benefit (toy):** relative size reduction; zeroed if output fails the
  verifier (nothing failed — every pipeline stayed verify-clean on all
  1000 fabrics, 5000 runs).
- **Score:** `utility − 0.05 × rel_cost`.
- **Battens:** one spline per (pipeline × {utility, cost}) over
  standardized features `[ln(cells), arith_frac, const_frac, depth/cells]`
  — cheap, one-walk features, no pass runs. Kernel = minimal Rust port of
  batten-spline's Nadaraya–Watson/RBF estimator (reimplemented, not
  imported: Python-vs-Rust, and CascadeRouter's one-confidence→3-target
  API doesn't fit per-candidate argmax; age decay dropped — static corpus).
  800 × 5 = **4000 verified outcome battens**.

## B.2 Numbers (release build, WSL2, rustc 1.97.1)

Fog-scale sweep, 200 held-out fabrics:

| fog_scale | accuracy vs oracle | regret (mean score) | cost saved vs always-full | trivial baseline |
|---|---|---|---|---|
| 0.25 | **123/200 = 61.5%** | 0.0176 | **26.2%** (9827 vs 13310 cells) | 54.0% |
| 0.50 | 113/200 = 56.5% | 0.0150 | 23.3% | 54.0% |
| 1.00 | 104/200 = 52.0% | 0.0167 | 22.3% | 54.0% |
| 2.00 | 92/200 = 46.0% | 0.0188 | 21.8% | 54.0% |

- Trivial baseline = always pick the train-majority pipeline (`dce>fold`).
- Cost vs the oracle-cheapest choice: 14.3% overhead at fog 0.25
  (9827 vs 8595 cells).
- Utility captured at fog 0.25: routed mean 0.7081 vs oracle 0.7155
  (−1.0%).

## B.3 Where routing fails (the fog analysis)

- The oracle only ever picks two pipelines: `dce>fold` (108) vs
  `fold>dce` (92). `none`/`fold` never win (the fuzz corpus always
  contains dead code, so DCE always pays); `full` never wins (same
  result as the 2-pass pipelines, strictly more work). The routing
  problem collapsed to a **near-tie binary choice** — mean regret is
  0.018 precisely because the two candidates are almost equivalent.
- Misroutes are almost entirely `fold>dce` chosen where `dce>fold` was
  best (66/77 at fog 0.25). Fog density separates correct from misrouted
  only at larger kernels: at fog_scale 1.0–2.0, mean fog at misroutes
  (0.33–0.36) exceeds fog at correct routes (0.26–0.31) — **fog does
  predict wrongness, the epistemology's core claim, weakly confirmed**.
  At fog 0.25 the separation vanishes (0.323 vs 0.305) — accuracy there
  comes from a tighter kernel, not from fog being informative.
- The 4-dim feature vector under-determines the choice: fabrics with
  near-identical size/op-mix/depth features get different oracle picks,
  so some misroutes are feature-space fog that no kernel width fixes.

## B.4 Verdict (undersold)

**In this toy, batten-routing earns only part of its keep.** It beats
the trivial always-majority policy by +7.5 points (61.5% vs 54.0%) and
saves 26.2% cost vs always-running-full while capturing 99% of utility —
but only at the tightest kernel, the margin is thin, and the two
contenders were near-ties anyway (regret 0.018). Fog density flagged
failures only in the regime where accuracy was worse. For a real
compiler, the honest read: routing between near-equivalent pipelines is
not worth a batten store; routing **cheap-tier vs full-walk verification**
(REVERSE-ACTUALIZATION §3(a)'s actual cascade) has orders of magnitude
more cost spread than this toy's 5 pipelines did, which is where the
method should be retried. Toy caveats, all of them: cell-count cost,
size-reduction benefit, fuzz corpus (uniform-ish shapes), λ=0.05 chosen
by hand, feature set hand-picked.

## B.5 Ledger

- `cargo test` green (13/13) before every commit; llvm-fabric vendor pin
  @ `2e5469e` (76/76 green).
- Nothing outside `experiments/batten-spike/` touched except this
  appendix.

---

# v1 LANE — control wires, inlining, the Weft

Started 2026-08-30, same machine, same rules: red/green per pass,
reachable hashes, conservation, N4 append-only, measured numbers, cargo
test green before every commit. Suite: **64 → 99 tests** (26 ctrl/verify,
7 program, 7 inline, 5 weft/sign, 1 v1-pipeline, plus fixture updates —
every commit green; the v0 suite stayed green throughout except one
fixture semantic change, §8.4).

## 8. Control wires — provenance crosses the control edge

v0 booked the gap (§4(a) Surprise, §5.2): *"data provenance does not
cross control edges — a phi's walk never reaches the branch condition."*
v1 closes it (`src/ctrl.rs`, 406 lines incl. tests).

- **Ctrl edges are explicit**: one edge per (terminator → gated region);
  terminators are the only ctrl-wire source (ARCHITECTURE §1.1); a phi
  is a mux whose select lines are the incoming terminator wires ([K-r1]).
- **The full walk** (`ctrl::full_provenance`) = the v0 data walk + the
  **backward control closure** of the queried cell's region (every
  terminator from which the region is reachable), each carrying its
  condition's data subtree. The money shot, `demo.fabric` phi %9:

```
v0 data walk                     v1 full walk
%9 = phi [then: %4] [else: %7]   %9 = phi [then: %4] [else: %7]
  %4 = const i64 1i64              %4 = const i64 1i64
  %7 = const i64 2i64              %7 = const i64 2i64
                                   ctrl: %5 = br %3, then, else   <- the edge crossed
                                     %3 = cmp.lt %2, %1
                                       %2 = arith.add i32 %0, %1
                                         %0 = param i32           <- v0 said unreachable
                                         %1 = const i32 42
                                   ctrl: %6 = jump join            <- the mux select lines
                                   ctrl: %8 = jump join
```

  Tested as the red/green pair: v0's
  `prov::tests::provenance_of_phi_reaches_all_roots` (asserts the param
  is NOT reached — kept green) and v1's
  `ctrl::tests::full_provenance_of_phi_crosses_the_control_edge`
  (asserts it IS, plus the jumps). Both hold; the walks answer different
  questions, and both are now queryable.

- **Why ctrl expands only at the root** (documented theorem in ctrl.rs):
  every data ancestor lives in the same region (same closure), the entry
  region (empty closure), or a phi join region P — a *predecessor* — so
  closure(P) ⊆ closure(root). The root's closure covers everything;
  nothing is approximated. This is also why the walk is cheap.

- **Loop-carried influences are real and are CUT with a marker.** The
  corpus (seed 1026845) found a genuine cycle: `add → its own region's
  gate → branch cond → back to the add`. In a cyclic fabric, a value's
  iteration-k+1 self truly depends on its iteration-k self. The walk
  marks the re-entry `revisit: %8 ...` instead of diverging or erroring
  (`ctrl::tests::loop_carried_influence_is_cut_with_a_marker`).
  Markers are only reachable when the region graph is cyclic — in
  acyclic region graphs they cannot appear (argued in ctrl.rs).

- **V16 (control well-formedness):** a phi must carry a join for EVERY
  real predecessor. A control edge without a mux input is exactly
  quilt-scratch's "silent no-op wire" (TILE-CONTRACT, scout's sharpest
  attack finding) — a value could arrive on a path the phi never
  selects. Reject/accept unit tests + a targeted mutation (drop a
  join+operand pair) put it in the corpus path.

- **V17 (operand acyclicity):** the corpus (seed 1032071) found that a
  mutated self-joining phi (`%10 = phi [r1: %10]`) sends v0's
  `Cell::ty_of` into **infinite recursion inside verify** (phi type =
  first operand's type). V17 runs an iterative cycle check over the
  operand graph BEFORE any ty_of call. Two unit tests (self-cycle,
  indirect cycle through arith).

**Corpus (10,000 fabrics, seed 0xFAB1C, release, 2.7 s):**

```
phis generated:        15333   <- v0 generated ZERO (see 8.3)
cells provenance-walked: 255446
ctrl-prov failures:    0       <- full walk holds on every cell of every fabric
prov failures:         0
roundtrip failures:    0
weft failures:         0       <- see 8.3
replay failures:       0
panics:                0
mutated:               4430
  still valid:         1367
  rejected:            3063 (by code)
    V01 1196  V02 11  V03 615  V04 613  V06 10  V07 12  V08 149
    V09 34    V10 26  V11 173  V12 14   V13 6   V14 1   V15 1
    V16 175   V17 27
```

(v0's histogram at f9dcd85: V01 1323, V03 644, V04 675, V08 191, V09 38,
V10 34, V11 232, V12 28, V15 5 — different because the corpus now
generates phis and mutation 10 exists.)

## 8.1 The third pass: inlining on fabrics

`src/passes/inline.rs` (372 lines incl. tests) + `src/program.rs` (379)
+ `CellKind::Call` + V18 + program codes P00–P04. The scout's build
order said inlining is hardest and ships last; it shipped last.

- **Programs**: `fabric v1` multi-function format (fn bodies delegate
  to the v0 parser; line numbers offset-adjusted — tested).
  Program-level verification checks what a lone fabric cannot: callee
  exists (P01), arity (P02), argument types (P03), return-type
  agreement (P04). Each code has a unit test.
- **The graft**: callee params are NOT grafted — uses rewire to caller
  args at graft time. The callee's `ret` is not grafted — call uses
  retarget to the mapped return value. Everything else grafts in order
  under fresh ids before the call site. The call cell is removed WITH a
  conservation ledger entry naming the graft:
  `- %4 (inlined 'add2': 1 cells grafted, 2 params bound to caller
  args, ret -> %7)`.
- **v1 scope, stated**: straight-line callees only (single region,
  acyclic entry, exactly-one-value return). CFG-grafting needs the
  region-edit diff vocabulary v0 explicitly deferred (§6.1) — still
  open. Ineligible callees are SKIPPED WITH RECORDED NOTES (the diff
  record carries `notes`; "a skip without a note would be the silent
  no-op" — tested for all four skip kinds).
- **The payoff shot** (`provenance_crosses_the_inline_boundary`):

```
before inline                      after inline
%3 = ret %2                        %8 = arith.add i32 %9, %0
  %2 = call i32 add2 %0, %1          %9 = const i32 42          <- 20+22, folded THROUGH the graft
                                     %0 = param i32             <- caller root, through the graft
```

  The walk crosses the inline boundary in both directions: forward
  (fold cascades through grafted cells) and backward (the inlined
  result's def chain runs into the caller's call-site values). And
  `prov_history(%2)` keeps the story after removal: "tick 2 inline:
  removed (inlined 'add2' ...)".
- **v1 pipeline**: constfold → dce → inline → constfold → dce; the
  second fold crosses the boundary (tested:
  `v1_pipeline_folds_through_the_inline_boundary`; replay reproduces
  all 6 stages bit-identically). Nested calls inline on the next sweep
  (tested). CLI: `cargo run -- inline examples/inlineme.v1fabric`.

**Inline diff sizes, measured** (`cargo run --release --bin inlinebench`;
caller with N call sites to a callee with B body cells, v1 pipeline):

```
callsites  calleebody  cells     orig-B  final-B  history-B  hist/final
1          1           4 -> 4       105      111        294        2.6
4          4           7 -> 19      191      635       1578        2.5
8          8           11 -> 67     307     2315       4926        2.1
16         16          19 -> 259    554     9036      17393        1.9
```

"Large diffs" confirmed — but note the contrast with v0's foldchain
(1,600× final): inline history scales ~2× final because it ADDS cells
(with per-cell AddCell + ledger) rather than deleting cascades. All
shapes verify + conserve + weft-hold at the end of the run (asserted in
the tool itself).

## 8.2 The Weft — signature every tick + the progress law

`src/sign.rs` (120 lines) + History extension (`diff.rs`). Reverse-walk
round-3 keeper: record the fabric signature at EVERY tick from day one —
"nearly free to record and impossible to recover retroactively." And
round-4's logic critique: progress must be mechanical, "or it launders
into vibes."

- **Signature**: FNV-1a 64 over the CANONICAL TEXT (print byte-for-byte
  — the fiber declared, THESIS-V3 discipline). NOT cryptographic; the
  claim is tamper *detection* via chain + replay, never resistance (the
  phantom-hash law from quilt-verilog, carried in the module doc).
- **TickSig { epoch, pass, sig, chain, advanced, note }**, one per
  tick, chained: chain_i = fnv(chain_{i-1} ‖ epoch ‖ pass ‖ sig).
  Progress is DERIVED from the diff: edits > 0 → "advanced (N edits)";
  else → "fixed point — no edits fired". No pass-author judgment
  exists in the API. The pipeline output reads:

```
tick 0 constfold sig=186d44749d0c55d8 chain=bbf3788c226f7f64 :: fixed point — no edits fired
tick 1 dce       sig=73d5135f996980bb chain=7f0e1f990be63119 :: advanced (1 edits)
tick 2 inline    sig=4f951584dd7dc027 chain=bd3cd683d4c4e0ae :: advanced (6 edits)
tick 3 constfold sig=7c0d2caa872628c2 chain=872a46f45e0c2a80 :: advanced (3 edits)
tick 4 dce       sig=d3752070c04723cd chain=0e34ba352bf432bb :: advanced (2 edits)
```

- **check_weft** — the law, checkable: all-or-nothing (a partial Weft
  is a violation: "weft covers 3/4 ticks"), gapless epochs, non-empty
  notes, non-advancing ticks must declare fixed point. v0-style
  push-only histories are labeled pre-law and allowed (empty weft).
- **verify_chain** — recomputes per-stage signatures against replayed
  stages and re-links the chain; a tampered stage fails naming its
  tick (`tampered_stage_breaks_the_chain_with_the_tick_number`).
- The corpus enforces both on every pipeline run (10,000/10,000 pass;
  `weft failures: 0`).

**Signature overhead, measured** (bench, sig-us column, medians of 21,
release — signature = one canonical print + one FNV pass):

```
shape          cells  print-us  sig-us   sig/print
chain-50         53       8.1     7.8       0.96
chain-800       803     122.6   116.5       0.95
diamonds-160   1443     171.7   176.6       1.03
dag-800         803     134.5   126.1       0.94
```

≈ one print pass per tick. At v0's 4-tick pipeline that is 4 prints on
top of ~4 verifies — and verify is 1.3–1.5× print at these sizes, so
the Weft roughly doubles-to-triples nothing: it adds ~1/5 of a
pipeline's existing per-tick cost. Honest caveat: it is O(fabric) per
tick with no incrementality (a Merkle/cell-hash scheme is the v2 move,
scout pass 1's mitigation); at 10k-cell fabrics with 100-tick
pipelines this will need it.

## 8.3 Failures, findings, surprises (first-class)

1. **The v0 corpus never generated a phi** (generator ordering bug:
   predecessors consulted before terminators placed; 0 phis in 2,000
   fabrics, measured). The "239,691 provenance walks" never walked a
   phi. Found while wiring ctrl-walk corpus coverage; fixed by deriving
   preds from the planned terminator table; regression guard
   (`st.phis > 0`) in the corpus test.
2. **V17/ty_of infinite recursion** — a latent v0 crash-on-cycle in
   verify, unreachable only because of (1). The fuzz-to-verifier loop
   worked exactly as designed: the corpus found it within 10k once
   phis existed.
3. **Loop-carried provenance cycles are REAL** — not corruption. The
   walk now marks them. This changed the module's contract from "Err on
   cycle" to "cut with a marker" (§8).
4. **One v0 fixture modified** (`v12_use_before_def_same_region`): the
   original fixture was accidentally a V17 cycle (self-referencing
   add), so V17 fired before V12. The fixture now uses a later cell —
   same V12 intent, no longer accidentally testing V17. Semantic change
   documented in the test body and here.
5. **Inlining is straight-line only in v1** — the region-edit diff
   vocabulary (RegionAdded/JoinDropped, §6.1 of v0) remains the real
   blocker for CFG-grafting and const-branch folding alike. Not
   laundered: skipped callees say so in recorded notes.
6. **Signature is whole-fabric per tick** — measured cheap now, not
   incremental; booked as v2 (§9).
7. **ctrl walk cost**: the root-only expansion exists because expanding
   at every tree node was measured unusable (~500× corpus slowdown
   during development; the theorem in §8 makes root-only complete, not
   approximate). Corpus time 2.7 s / 10k fabrics (v0: 0.6 s — the delta
   buys phi generation + full-walk + weft checks on every fabric).

## 8.4 What v2 should change

1. **Region/edit vocabulary in diffs** — still #1 (unlocks CFG-graft
   inlining, const-branch folding, region-DCE).
2. **Incremental signatures** (Merkle per cell / content-addressed) —
   bounded Weft cost at scale.
3. **Maintained use/pred tables** — v0's O(n²)-ish verify is now the
   largest measured cost in the pipeline (diamonds-160: 1.5 ms verify
   vs 0.17 ms print).
4. **Dominance-based scope rules** (v0 §6.5) — unchanged.
5. **Interpreter + differential harness vs a reference compiler**
   (ARCHITECTURE M2) — the ground truth that makes transforms like
   inline *semantically* judged, not just structurally.
6. **A negative corpus per code** — V16/V17 joined the mutant battery
   by mutation; golden mutants per code remain the goal.

## 8.5 Ledger (v1)

- Commits: control wires (V16/V17 + generator fix) → calls/program/
  inline → Weft/signatures/progress law → this file. Reachable in
  `git log`; cargo test green before each (99/99 at HEAD).
- Nothing deleted; the v0 sections of this file are the v0 record
  (N4 applies to docs too).

## 9. M3/M4 — the ledger pass manager and DCE-as-decay

**Commits:** `d321793` (M3 manager) → `7d3bae2` (M4 decay) → `7378df3`
(hardening). Suite 99/99 before → **121/121 after** (12 manager tests,
10 decay tests; the hardening round deepened manager coverage +3).
Both milestones green against ARCHITECTURE §4's exit criteria, with the
caveats in §9.4.

### 9.1 What was built

**M3 — the manager (`src/manager.rs`).** A driver where enforcement
moves from pass authors to the machine. A pass is a pure function
`fabric -> (fabric, diff)`; the manager mechanically rejects, before
the tick lands, a pass that emits an unverifiable fabric, drops or
conjures values without ledger entries (`conserve`), or records its
diff under a name other than the scheduled one. Every outcome lands as
a Weft entry — advance (edits > 0, derived, never asserted) or fixed
point — via the already-proven `push_tick`. Post-run, in-manager: weft
law, chain-vs-stages, pipeline-wide conservation, and bit-identical
replay (including replay-from-mid-history, D5). Registered passes
compose in any order (`run(f, &[...names])`); the CLI grew a
`manager FILE [PASSES...]` verb.

**M4 — DCE as decay (`src/decay.rs`).** The load-bearing inversion: a
value dies ONLY when its death ledger entry is a machine-checkable
certificate naming **killer pass + tick + witness** — rendered form
`death{killer=dce-decay tick=3 users=0 witness=no-demand}`. The
verifier (`verify_deaths`) recomputes the witness from the pre-tick
fabric: the cell was present, had exactly the claimed user count, and
had no demand path to a terminator. A bogus kill is REJECTED naming
the demand chain. Bare prose (`"dead: no path to a terminator"` — the
old `dce` ledger style) no longer passes as a decay death. The tick in
a certificate comes from the manager's `TickCtx`, not the pass's
guess; a mismatch between the two is itself a rejection cause.

Honesty first: the **liveness criterion is still reachability**, not
the Hebbian use-count aging of ARCHITECTURE §1.4. What M4 delivers is
the certificate/verifier machinery — the death-contract inversion —
with reachability as the one v0 witness kind. Aging-based witnesses
are v2 debt (§9.5).

### 9.2 Numbers (release build, WSL2, rustc 1.97.1, 24 cores)

Corpus (unchanged generator, `./llvm-fabric fuzz --iters 10000`):
10,000/10,000 valid, 15,333 phis, 255,446 provenance-walked cells,
0 roundtrip / prov / ctrl / weft / replay failures.

Decay curves (`./llvm-fabric decay-curve --iters 10000`, seed
0xD3CA5, pipeline `constfold → dce-decay → constfold → dce-decay`,
1.1 s wall for the whole 10k):

```
fabrics: 10000   cells in: 255198   cells out: 70765   bogus deaths: 0
stage after        mean cells  dead    cold    warm   (per fabric)
    0 <input>          25.52  17.02   2.09   6.41
    1 constfold        25.52  18.44   0.00   7.08
    2 dce-decay         7.08   0.00   0.00   7.08
    3 constfold         7.08   0.00   0.00   7.08
    4 dce-decay         7.08   0.00   0.00   7.08
deaths per tick: 50296[constfold] 184433[dce-decay] 0[constfold] 0[dce-decay]
decay kills (certified): 184433 / deaths total: 234729
ledger: 23510335 bytes total, mean 2351.0 B/fabric; weft entries: 40000
```

Readings, undersold: 66.7% of generated fabric is dead on arrival
(the generator overproduces dead cells — a fuzz artifact, not a claim
about real programs); constfold converts the 2.09 cold cells/fabric
into dead (fold consumes them; their removals are ledgered as
"folded into" — consumed-with-derivation, not decay kills); the first
dce-decay sweep removes every dead cell (18.44/fabric mean) and the
rest of the pipeline is a measured fixed point (0 deaths at ticks
3–4 across all 10k fabrics). Deaths reconcile exactly: 50,296
transform-consumed + 184,433 certified decay kills = 234,729 total;
0 deaths anywhere in the run lacked a certificate that verified
(`bogus deaths: 0`). Ledger overhead: 2,351 B/fabric mean (~92 B per
cell-tick of paperwork; compaction is §9.5).

### 9.3 The tamper-rejection proof (M4 exit criterion)

`cargo run --release --example bogus_kill` — a forged certificate for
a LIVE cell, with a correct user count, handed to the verifier:

```
forged ledger entry: death{killer=dce-decay tick=0 users=1 witness=no-demand}
verifier says: death rejected: bogus kill of %1 — the cell is LIVE; demand chain %1 -> %2 -> %3 holds it
REJECTED (exit 0)
```

The same rejection fires, with distinct messages, for a wrong user
count ("claims 5 users but the pre-tick fabric has 0"), a wrong tick
("names tick 9 but the diff lands at epoch 2"), a wrong killer
("names killer 'other-pass' but the diff records 'dce-decay'"), and
bare prose ("without a certificate"). All five are red/green tests in
`decay.rs`.

### 9.4 Failures, findings, surprises (first-class)

1. **The manager's law order is observable.** Our first leaky-pass
   fixture dropped a USED cell; `verify` (V01 dangling operand)
   rejected it before conservation could, so the conservation test
   failed its own intent. Fixed by dropping an unreferenced const.
   Manager order is verify → conserve → land; both are mechanical,
   and a test must target the law it claims to test.
2. **Rust pedantry cost 20 minutes**: `BTreeMap::get_key_value`
   inference picked the `Borrow<&'static str>` candidate and demanded
   the pipeline slice outlive `'static`. Fix: the audit carries
   `rec.pass` (the pass's own declaration, already proven equal to the
   scheduled name). Zero semantics changed.
3. **The corpus is dead-heavy** (66.7% dead on arrival). Decay curves
   on it measure the machinery, not realistic programs. Booked.
4. **Cold/warm classification is post-hoc** — "cold at stage k" uses
   the final fabric to know what dies later. It is an autopsy, not a
   prediction; labeled as such in the code. A predictive demand
   analysis is v2.
5. **Two drop classes coexist and both conserve**: constfold's
   "folded into" (consumed-with-derivation) and dce-decay's death
   certificates. Only the latter is bound to `verify_deaths`; the
   former is bound to `conserve`. This is the three-way law (delivered
   / consumed / dropped-with-entry) showing up in the numbers.
6. **The whole corpus converges in 2 ticks** — the pipeline's second
   fold/decay pair is always a fixed point on generated fabrics. A
   cheaper steady-state detector (stop-on-idle) would halve the tick
   count without losing any ledger guarantee; booked.
7. **Ledger size dwarfs the fabric** (2.4 KB mean vs ~25 cells).
   Known problem, now with a number attached to this pipeline shape.

### 9.5 What v2 should change (M3/M4 additions to the v1 list)

1. **Aging-based witnesses** — implement ARCHITECTURE §1.4's
   use-count aging as a second witness kind; the certificate/verifier
   shape is ready for multi-kind witnesses.
2. **Predictive cold/warm** — pre-mortem liveness instead of autopsy.
3. **Stop-on-idle scheduling** — measured: ticks 3–4 are always fixed
   points on the corpus.
4. **Ledger compaction with certificates intact** (the
   REVERSE-ACTUALIZATION amputation problem, now priced at ~92 B per
   cell-tick on this corpus).
5. **Region-level decay** — unreachable regions still shield their
   terminators (v1 debt, unchanged).
6. **Cross-run tamper evidence** — the chain is recomputed in-run;
   binding it to the on-disk fabric file (QUF-shaped format) is the
   next step toward audit-without-replay. FNV-1a remains
   non-cryptographic (sign.rs caveat, unchanged).

### 9.6 Ledger (M3/M4)

- Commits `d321793` (manager), `7d3bae2` (decay), `7378df3`
  (verification-lane hardening), reachable in `git log`; cargo test
  green before each (99 → 108 → 118 → 121). Nothing deleted; §8 and
  earlier sections are the prior record.
- The independent verification pass over both commits is booked in
  §9.7, including its one wrong claim and the red test that corrected
  it.

## 9.7 Independent verification lane (Claude Code, Sonnet 5)

`claude -p` review of `manager.rs` + `decay.rs`, run in tmux session
`llvm-m34` after both commits, plus a confirmation round after the
hardening commit `7378df3` (transcript `/tmp/llvm-m34-verify.out`,
ephemeral; findings below are the durable record).

**Round 1 verdict (code inspection): no bypasses found.** Solid: name
matching, verify-on-output, post-run replay, strict certificate
parsing, killer/tick/users re-verification, bogus-kill rejection,
fixed-point ledgering, user-count re-measurement. Concrete findings,
all closed in `7378df3`:

- replay-from-mid-history tested one prefix (k=2) → now every boundary;
- no overlapping-diff test → double `RemoveCell` for one id now proven
  rejected (by replay reconciliation);
- `register` shadowing undocumented → now last-wins, and a shadowing
  pass must record under the registered name or the laundering check
  fires (the test's first form failed as designed and proved it).

**The lane was also wrong once, and the correction is load-bearing:**
it claimed `conserve::check` catches a phantom removal (diff lists a
removal the fabric did not perform). It does not — conserve checks
vanished/appeared cells against the edit lists, not listed-vs-performed;
the phantom passes conserve and is caught by post-run **replay**
(`manager_rejects_a_phantom_edit_via_replay_reconciliation`). Red test
added; the lane accepted the correction in round 2. Verification lanes
are fallible; that is why their findings are red tests, not prose.

**Round 2 final verdict (quoted, condensed):** "M3 and M4 prove the
defense-in-depth mechanically… No bypass found in manager or decay
logic itself, but the whole system sits atop three unverified
substrates: `verify()` correctness, `conserve()` sufficiency for its
layer, and `live_closure` completeness (is_terminator + operands
encoding all live dependencies)." The three substrates are the repo's
own tested layers (`verify.rs` 30+ mutant tests, `conserve.rs` suite,
ctrl-wire coverage in §8) — delegation, not absence — except the third,
which maps exactly to the booked region-decay debt (unreachable
regions shield their terminators; §9.5 item 5). Suite count the lane
reported from inspection (12+10 in the new files); the D7 re-run
command and count: `cargo test --release` → **121 passed, 0 failed**.

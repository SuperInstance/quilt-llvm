# EXPERIMENTS — llvm-fabric v0 spike

**What this is:** the experimentation leg of the keel (README.md). The keel
claims a cell-model IR with inspectable history is worth building. This
spike tests those claims with running code. Everything below was measured
on this machine (WSL2, rustc 1.97.1, debug build unless noted). Every
claim cites the test or command that produced it. Where something is a
toy, it says toy.

Code: `experiments/llvm-fabric/` (zero-dependency Rust crate).
Run everything yourself:

```
cd experiments/llvm-fabric
cargo test                          # 64 tests, all green
cargo run --release -- fuzz         # 10,000-fabric corpus
cargo run --release -- bench        # size/serialize numbers
cargo run --release -- pipeline examples/foldme.fabric
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

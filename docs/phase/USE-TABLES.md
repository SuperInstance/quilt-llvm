# USE-TABLES — R2 lane B: maintained use/pred/succ tables

*Landed 2026-08-30 on branch `r2-use-tables` (worktree
`quilt-llvm-wt-usetables`), branch point `a556e4f`. House rules:
measured or it didn't happen; judgment labeled; undersell.*

**What this lane is.** GATE-W2 §4 KEEP: `predecessors()` and
`uses_of()` were linear scans over the whole fabric (fabric.rs@`a556e4f`;
EXPERIMENTS §4c). The tables are an **R3 prerequisite** — R3's
region-edit vocabulary will hammer pred/succ queries on every region
add, remove, and join drop — not a performance win. The cost argument
was measured weak by the gate (§3: the corpus never exceeds 63 cells,
C10); the structural argument is the reason this lane exists.

**What shipped:**

- `src/usetables.rs` — `UseTables {users, succs, preds}` with a full
  O(n) `derive()` (ground truth) and O(degree) incremental ops
  (`add_use`, `remove_use`, `move_use`, `set_succs`, `ensure_rows`).
- `Fabric` owns the tables; every sanctioned edit maintains them:
  `add_cell`/`insert_cell`/`place_cell` (add), `remove_cell` (remove —
  new), `retarget` (rewire — new), `set_kind` (edge edit on
  terminators — new), `set_operands` (bulk rewire, e.g. phi join-drop
  — new), `forget` (decay death), `add_region` (region op).
- Queries are table lookups: `uses_of` O(degree), `successors` O(1),
  `predecessors` O(degree) — all returning borrowed slices in the exact
  order the old scans produced (user-asc/slot-asc; succs dedup
  [then,else]; preds dedup ascending).
- `Fabric: PartialEq` is now manual and **ignores the tables**: a
  derived index is not fabric content (a fabric and its re-derived
  twin are equal). Replay bit-identity is unaffected by table state.
- Production edits converted to the mutators: constfold, dce, inline,
  replay (`apply_edit`), semmut's mutation operators (the two operand
  swaps → retargets; ret/phi rebinds → retarget; branch-target swap
  and join-drop-with-edge → `set_kind`/`set_operands`). Value-only
  const edits (const-off-by-one, stage-tamper) stay raw — provably
  table-neutral (they touch no operand, no terminator).
- **The desync policy, stated plainly:** `slab`/`regions` are public;
  the forgery tier (`fuzz::mutate`, verify test fixtures, the
  manager's deliberately-broken test passes) pokes them raw and can
  desync the tables — by design; those fabrics exist to be rejected.
  Audited: every reachable forgery fires a structural code (V01
  dangling operand, V03 missing terminator) before any table-backed
  query (V06/V16 preds). `rebuild_tables()` is the O(n) recovery path
  for raw tooling. One documented semantic deviation: operands whose
  id is ≥ slab length at placement (fresh-phi `u32::MAX`
  placeholders, forged forward refs) are not indexed; the old scan
  could only see them if the slab later grew past the id, which no
  sanctioned path does.

## 1. The derivability law (the replay law, applied to indexes)

A maintained index that cannot be re-derived is a lie. The law:
`UseTables::derive(f) == f.tables` at every point reachable through
the sanctioned vocabulary.

Enforced by `fuzz::tests::tables_derivable_bit_identical_10k_corpus`:
the **whole 10k corpus** (same seed law as the published 10,000/10,000
replay number, seed 0xFAB1C), checked on the generated fabric, on
**every pipeline stage**, and on **every replayed stage** — the
maintained tables after constfold/dce edits and after history-replay
edits must be bit-identical to a from-scratch derivation.
`corpus_run` gained a `tables_fail` counter (0 on the 10k run;
mutated fabrics are exempt — forgeries desync by design).

The unit tier pins each maintenance op separately
(`add_remove_retarget_keep_tables_derivable`): add / mid-region
insert / retarget / set_operands / set_kind on a terminator / remove /
remove-of-terminator, each asserted against `derive` AND against a
second, independent scan implementation kept in the test. The
sabotage test proves the red condition: a use-blind add and a
use-blind remove each desync (identity breaks).

Two maintenance bugs were caught by this law during the lane, both
fixed before landing: (1) forward control references — a `jump j`
placed before region `j` is registered — dropped the pred edge
(rows now grow on demand, capped against forged indices); (2) a
swapped-variable bug in `set_succs` inserted the *target* into its
own pred row. Both were invisible to `derive`-vs-`derive` comparisons
and caught only by the independent scan — which is why the scan twin
exists in the test.

## 2. Query cost, before/after (the 53→1443-cell curve)

Measured with `llvm-fabric utbench` (release, medians of 21 sweeps;
a sweep = one query per present cell for uses, per region for preds;
scan twins are the pre-R2 implementations copied verbatim into the
harness). Verify "before" from the pristine `a556e4f` build, same
machine, same session.

```
query cost, one sweep = one query per cell (uses) / per region (preds),
median of 21 sweeps, release build:

shape          cells   uses-scan-us/sweep   uses-table-us/sweep   preds-scan-us/sweep   preds-table-us/sweep
chain-50           53                 6.2                  0.1                  0.0                   0.0
chain-200         203                70.3                  0.1                  0.0                   0.0
chain-800         803              1534.1                  0.5                  0.0                   0.0
diamonds-10        93                12.8                  0.1                  9.8                   0.0
diamonds-40       363               194.0                  0.2                136.4                   0.0
diamonds-160     1443              3994.2                  0.9               3944.4                   0.0
dag-50             51                 5.3                  0.1                  0.0                   0.0
dag-200           203                86.1                  0.1                  0.1                   0.1
dag-800           803              1582.1                  0.5                  0.0                   0.0
```

Read: at diamonds-160 a full uses sweep drops **3994 µs → 0.9 µs**
(~4,400×; per-query 2.8 µs → 0.6 ns) and a full preds sweep
**3944 µs → <0.05 µs** (below the timer's per-sweep resolution at 481
regions). The scan twins are verified bit-identical to the tables on
a real shape (`ut_tests::scan_twins_agree_with_tables`) — the "before"
measures the same thing the "after" answers. Preds on chain/dag are
trivial for both (single region). Reproduce: `llvm-fabric utbench`.

## 3. Verify scaling re-measured (gate target ≤ 1.2)

Before = pristine `a556e4f` build, after = this branch, both via
`llvm-fabric bench` (release, medians of 21), same machine, same
session. Verify column:

```
shape          cells    verify-before-us   verify-after-us
chain-50          53                1.9                4.1
chain-200        203               17.7               23.1
chain-800        803              197.1              264.9
diamonds-10       93                7.9                4.2
diamonds-40      363               94.9               10.5
diamonds-160    1443             1457.4               60.2
dag-50            51                2.2                2.8
dag-200         203               21.8               28.3
dag-800         803              251.3              381.0
```

Fitted exponents (least squares on log-log unless stated):

```
fit                    before     after
two-point c50→d160*    2.01       0.81
LS all-9 (mixed)       1.83 R².98 1.27 R².75
LS diamonds-only       1.90 R²1.0 0.97 R².97
LS chain-only          1.71 R²1.0 1.53 R².99
LS dag-only            1.72 R²1.0 1.78 R²1.0
```

\* the method that produced the published O(n^1.96) (NEXT-PHASE §R2:
27.2× cells → 642.6× verify).

**The honest read.** The gate target ≤1.2 is met by the method that
produced the published number (two-point: 0.81) and by the family the
target was about (diamonds, the CFG/phi shape where pred queries
dominated: 1.90 → 0.97; absolute cost 1457 → 60 µs, 24×). The mixed
all-9 LS fit reads 1.27 with poor R² because single-region shapes
carry a DIFFERENT quadratic this lane did not touch: V12's
`index_in_region` does a linear `position()` per operand inside the
use-before-def loop — O(n²) on same-region chains/dags (chain-800 and
dag-800 verify actually got slightly slower: the tables add ~1 µs/100
cells of maintenance on placement, visible only where the quadratic
shrank). Booked as the next known verify quadratic; out of this
lane's scope (it is a region-list index, not a use/pred table). Small
shapes pay a small constant (~+2 µs at 53 cells) for table
maintenance at build time — the corpus-relevant regime (≤63 cells,
~2 µs verify) is unchanged in any way that matters (C10).

## 4. Judge cost — kills per microsecond (the gate's new criterion)

`llvm-fabric semmut --iters 2000 --seed 24269` (the gate's own
reproduction run). The tier table reproduces lane 3 bit-for-bit on
this branch — judged 1,203 · killed 76 (6.3%) · confirmed-wrong 113 ·
unjudgeable 1,002 · replay=76 — so the mutator conversions (retarget/
set_kind/set_operands) preserved determinism. Judge costs are this
run's wall clock:

```
judge             calls   kills   total-us   us/call   kills/us
ctrl               1127       0    78178.7     69.37     0.000
pipeline           1127       0    68677.1     60.94     0.000
roundtrip          1203       0    49603.1     41.23     0.000
prov               1127       0    26731.2     23.72     0.000
replay             1203      76    16589.5     13.79     0.005
conserve           1203       0     1634.4      1.36     0.000
verify             1203       0     1353.3      1.12     0.000
oracle(dataflow)   1400      37      277.3      0.20     0.133
```

The gate's question — which judge earns its time — answered with
numbers: **the three most expensive judges (ctrl, pipeline,
roundtrip: 172 µs/call combined) bought zero kills**; every kill in
the battery came from **replay** (76/76 on the tamper control, 0.005
kills/µs); and the **dataflow property oracle is ~26× more
kill-efficient per microsecond than replay** (0.133 vs 0.005) while
being the cheapest check on the board — but it is a wrongness
ORACLE (needs a ground-truth evaluator), not a tamper detector, and
it only reaches the 37 dataflow-decidable mutants of 1,127 input
mutants. Verify costs 1.12 µs/call and fired 0/1,127 — cheap and
blind on input semantics, exactly as GATE-W2 §3 said. Caveat: the
76 replay kills all live in the tamper control; kills/µs answers
"what does time buy TODAY," not what a future semantic oracle would
buy.

## 5. Suite counts

Baseline at `a556e4f`: **139 lib + 19 measurer (shape-audit) = 158,
green** (re-run at branch point before any edit). This branch:
**145 lib + 19 measurer = 164, green** (`cargo test`, full run).
The +6: four usetables unit tests (scan-parity on a diamond; the
add/retarget/set_operands/set_kind/remove derivability walk; the
sabotage battery; hole-row parity), `ut_tests::scan_twins_agree_with_
tables`, and `tables_derivable_bit_identical_10k_corpus` (10k
fabrics × every pipeline stage × every replayed stage; 9.2 s debug;
`QUILT_UT_ITERS` shrinks it).

Cross-checks that the tables changed no outcome:

- 10k corpus run (release): all invariants green, and the mutation
  rejection histogram is **bit-identical to master's published
  histogram** (V01 1196 · V03 615 · V04 613 · V16 175 · V17 27 …) —
  the corruption tier's verdicts did not move.
- semmut 2000/24269: tier table bit-identical (above).
- shape-audit corpus totals pinned by its 19 tests (255,446 cells ·
  15,333 phis) still green.
- Red/green: sabotaging `remove_use` (env-gated) turns both the unit
  derivability test and the 10k-corpus law red ("pipeline stage
  tables diverged from derivation") — the green is earned, not
  vacuous.

## 6. Honest limits (labeled)

- The verify exponent is a **bench-shape number the corpus does not
  exercise** (C10: corpus fabrics ≤ 63 cells, where verify costs ~2 µs
  either way). It measures the hand-built curve, nothing else.
- The judge-cost table's kills/µs divide kills that all come from the
  tamper control (input-level semantic mutants: 0 kills — lane 3's
  finding, unchanged here). The table says *where* time goes and what
  it buys **today**, not what a semantic oracle would buy.
- Query "after" numbers include the borrow-checker-free slice return,
  but exclude the cost of *maintaining* the tables during edits
  (maintenance rides the edit paths; its share is inside the pipeline
  numbers, not the query numbers).
- Wall-clock timings are one session's numbers (WSL2); medians of 21.
  Order-of-magnitude evidence, not a benchmark-suite claim.

## 7. Judgment, labeled as judgment

- The tables are worth building for R3 regardless of the measured
  query wins: every region-edit operation in R3's vocabulary needs
  preds/succs of the edited regions, and the join-drop surgery
  (semmut's `JoinDropWithEdge`, the gate's noted R3 prerequisite)
  already runs through `set_kind`/`set_operands` because of this lane.
- `remove_use` keeps rows sorted (scan-order parity) at O(row) instead
  of O(1) swap-remove. Rows are tiny (avg uses per def ≈ 1–2 on the
  corpus); the determinism is worth more than the nanoseconds.
  Judgment, not measured.
- `cell_mut` remains public and raw-kind-edits through it remain
  possible (the forgery tier needs them). A future lane could split
  "value edits" from "structure edits" in the type system; not
  attempted here — scope discipline.

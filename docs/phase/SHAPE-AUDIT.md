# SHAPE-AUDIT — what the corpus generator actually emits

*R1 lane 2, 2026-08-30. Measured, not read: every number below is the
`shape-audit` binary (in-tree, `experiments/llvm-fabric/src/bin/shape-audit.rs`)
run over the exact corpus behind the published claims — 10,000 fabrics,
seed base `0xFAB1C` — plus a stability run at seed base `0xD3CA5`
(the decay-curve seed). Both runs are in the audit log below; every
structural conclusion reproduces on both. House rules: measured numbers
or it didn't happen; judgment labeled as judgment; failures first-class;
undersell.*

**Why this document exists.** Every "10,000/10,000, 0 failures" claim in
EXPERIMENTS.md is a statement about *this generator's output
distribution*, and nothing else. The list of constructs the generator
cannot emit (§1) is the boundary of those claims.

Reproduction:

```
cd experiments/llvm-fabric
cargo run --release --bin shape-audit            # 10k, seed 0xFAB1C
cargo run --release --bin shape-audit --seed 0xD3CA5
cargo test --release                             # 121 passed, 0 failed
```

Cross-checks against published numbers (same seeds): phis **15,333**
and cells **255,446** at `0xFAB1C` — bit-for-bit the counts in
EXPERIMENTS §9.2; cells **255,198** at `0xD3CA5` — matches the
decay-curve "cells in" exactly. The audit sees the same corpus the
claims were made on.

---

## 1. The cannot-emit list (the deliverable)

Measured zero-for-10,000 on **both** seed bases. Each row bounds every
corpus-derived claim listed after it.

| # | Construct | Measured | What it bounds |
|---|---|---|---|
| C1 | **Call cells / call depth ≥ 1 / multi-fabric programs** | 0 call cells in 20,000 fabrics; max call depth 0 | All M5/inline and v1 `program` claims. The corpus never exercises `Call`, callee resolution, or program-level verify; those numbers come from hand fixtures (`inlineme.v1fabric`) only. EXPERIMENTS says this (§ "v1 scope, stated"); it is now measured. |
| C2 | **Non-i32 phis** | 15,333/15,333 phis are i32 | Phi joins of i1/i64/f64 are untested territory: no fold, no verify path, no decay behavior for a non-i32 mux exists in any corpus number. |
| C3 | **>1 phi per region** | max seen: 1, in 20,000 fabrics | Multi-phi join blocks (the normal shape of real SSA merges — one phi per merged value) never occur. |
| C4 | **Phi at spine head** | 0/15,333 at head; 100% sit immediately before the terminator | Spec conflict, first-class: ARCHITECTURE §2.1 says "phi cells sit only at a spine head." The generator cannot emit the spec's placement, and `verify` accepts the placement it does emit — the rule is enforced nowhere. Either the spec or the verifier is wrong; corpus-green proves nothing about it. |
| C5 | **Phis with partial predecessor coverage** | 0/15,333 (all phis cover exactly their region's preds) | Half-expected: V16 *rejects* partial phis, so they are ungeneratable **and** invalid — the only partial phis in testing arrive via mutation 9. Real post-transform CFGs routinely have them. |
| C6 | **A phi value feeding computation** | consumers: phi 5,866, ret 1,455; **arith/cmp/branch: 0** | The headline bias. No fold, DCE decision, or branch in the corpus has ever depended on a phi's value. Combined with C4 (phis emitted after all body cells, so nothing downstream can legally reference them), every loop-shaped fabric is **cyclic but computation-free**. "No real loops" (§9.4.3) is now a number: 0. |
| C7 | **Params outside the entry region** | 0 (1.505/fabric, all in entry; 77.4% i32) | Consistent with v0 scope, but it means param-related V-codes are exercised only in entry position. |
| C8 | **Nested/contained regions** | impossible in the IR: regions have no parent field; every fabric is a flat 1–6-region CFG | V3 ("containment acyclic forest") is vacuously true on everything the corpus can produce. Any region-*tree* semantics (R3's schedule, §1.4's post-order) is corpus-untested at depth > 1. |
| C9 | **Boundary-valued or negative-domain constants** | i64: min 0, max 999,973, negatives 0/27,613; i32 confined to [−500, 500) ∪ [0, 100) mints; f64: all multiples of 0.125 in [−12.5, 49), 0 exceptions in 27,607; no NaN/Inf (parser rejects) | The wraparound/overflow/saturation corners of the fold table are corpus-invisible. The property oracle's ±MAX/MIN/MAX−1 grid (R1 lane 1) covers precisely the corner the corpus cannot reach — the two orifacts are complements, not duplicates. |
| C10 | **>6 regions, >63 cells, >12 ctrl edges** | max 60 cells (0xFAB1C) / 63 (0xD3CA5); region count uniform 1–6 | All scaling claims (verify O(n^1.96), diamonds-160, chain-50) come from hand-built bench shapes. The corpus itself never exceeds toy size. |
| C11 | **A non-latest value as branch condition or arith operand head** | by construction (judgment from code, confirmed by dup-operand rate): cond = most recent i1; arith a = most recent value, b = second-most-recent or a | Distribution bias, not absence: operand *choice* is chain-shaped, so use/def fan-out is unnaturally low. 29.6% of ariths have `a == b`. |

Dropped after measurement (judgment, recorded so nobody re-litigates it
by reading the code): "branch with equal targets" — the generator's
distinct-target fixup only avoids the entry region, and measurement
caught it: **969/12,703 branches (7.6%) have `then == else`** at
0xFAB1C, 924 at 0xD3CA5. Reading the code said 0; the histogram
overruled. Measure first.

---

## 2. Measured histograms (10,000 fabrics, seed 0xFAB1C)

### 2.1 Region count per fabric — uniform by design, confirmed

| regions | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|
| % of fabrics | 16.65 | 16.62 | 16.63 | 16.71 | 16.72 | 16.67 |

### 2.2 Cells per fabric

mean **25.54** (claim: 25.52 ✓), min 2, max 60.

| bucket | 1–10 | 11–20 | 21–30 | 31–40 | >40 |
|---|---|---|---|---|---|
| % | 13.01 | 24.57 | 25.53 | 24.24 | 12.65 |

### 2.3 Ctrl edges per fabric (br = 2, jmp = 1, ret = 0)

mean 4.11; total 41,063 edges, of which 40,094 unique region→region.

| edges | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| % | 16.65 | 2.38 | 8.40 | 13.31 | 14.54 | 12.96 | 11.35 | 9.45 | 6.15 | 3.35 | 1.14 | 0.28 | 0.04 |

### 2.4 Phi count per fabric (cap 4 = 4+)

| phis | 0 | 1 | 2 | 3 | 4+ |
|---|---|---|---|---|---|
| % | 25.20 | 25.66 | 27.56 | 15.07 | 6.51 |

Total **15,333** (matches EXPERIMENTS §9.2 bit-for-bit). Join sizes:
1→8,341 (54.4%), 2→5,433, 3→1,374, 4→168, 5→17. **A majority of phis
are single-pred pass-throughs, not merges**; true merges (≥2 joins) are
0.70/fabric. Unconsumed phis: 9,515 (62.1%). Phi operand slots pointing
into the phi's own region: 5,849 of 24,086 (24.3%).

### 2.5 Call depth

**0 everywhere** — histogram is a single bar at zero (C1). There is no
call graph to histogram.

### 2.6 Type coverage

Value cells: i1 69,061 · i32 65,351 · i64 43,104 · f64 42,906.
Use-wires by source type: i32 86,775 · i1 53,011 · i64 37,251 · f64 36,992.
Fabrics containing ≥1 value of each type: i32 98.79% · i1 96.68% ·
i64 90.78% · f64 90.44%. Consts: i1 24,400 · i32 24,149 · i64 27,613 ·
f64 27,607; 18,671 distinct values. i1 consts: 12,609 true / 11,791 false.
Ranges: see C9. All four types are present — but see C9 for the value
*domain*.

### 2.7 Cyclic-region frequency — the surprising one

| metric | 0xFAB1C | 0xD3CA5 |
|---|---|---|
| fabrics with ≥1 ctrl cycle | **77.03%** | 76.40% |
| …cycle reachable from entry | 72.51% | 71.70% |
| regions on a cycle | 18,965 (1.90/fabric) | — |
| edges closing a cycle | 24,084 | — |
| jmp self-loops (r→r) | 10,295 in 6,477 fabrics (64.7%) | — |

The corpus is *structurally* loop-heavy — 3 in 4 fabrics contain a
cycle — yet (C6) not one carries loop-carried dataflow. The two facts
together are the sharpest statement of the generator's blind spot: it
builds loop *shapes* and cannot build loop *computation*.

### 2.8 Secondary shape facts (bounds for other claims)

- **Unreachable regions: 57.38% of fabrics contain ≥1** (11,617 total,
  1.16/fabric). Good news, recorded: R3's region-DCE will have real
  corpus material — the worry that the corpus "contains no unreachable
  regions" and would prove nothing (NEXT-PHASE §3 R3) is measured false.
- Dead-on-arrival, immediate definition (value cell with zero users):
  **90,993/220,422 = 41.3%**. The published 66.7% uses the decay
  classifier (transitive: no backward path to any terminator) — a
  different, legitimate definition; both are now stated so neither can
  be quoted as the other.
- Branch conditions: cmp 6,461 (50.9%) · arith(i1) 3,074 · const 3,050
  (24.0%) · param 118. **A quarter of all branches are on constant
  conditions** — const-branch folding (R3 pass 1) has corpus material.
- Arith mix ≈ uniform (add 14,353 / sub 14,298 / mul 14,160 / div
  14,058); cmp mix ≈ uniform; div-with-const-zero-RHS: 906 (defined
  wraparound, exercised). Cmp operand types: i32 16,083 · i1 5,276 ·
  f64 4,035 · i64 4,006.
- ret: with value 4,702 / void 1,962; value-from-phi 1,455.
- Params: 15,051 total (1.51/fabric), i32 77.4%, others ~7.5% each.

### 2.9 Stability run (0xD3CA5, 10,000 fabrics)

cells 255,198 / mean 25.52 (matches decay-curve input exactly) · phis
15,255, all i32, 0 at head, 0 partial, 62.2% unconsumed · cyclic 76.40%
· unreachable-region fabrics 56.47% · calls 0 · i64 negatives 0 · f64
non-eighths 0 · immediate-dead 41.4%. **Every conclusion in §1–§2
reproduces on the second seed base.**

---

## 3. What this does to the claims (undersold)

1. "10,000/10,000 valid, 0 failures" is true and now precisely bounded:
   it is a claim about flat, ≤6-region, ≤63-cell, call-free fabrics
   whose phis are i32-only, single-per-region, full-coverage,
   terminator-adjacent, and (except via phi→phi and phi→ret) unused.
2. The phi *machinery* (V16, prov through joins, ctrl closure) is
   structurally exercised on 15,333 instances — but the phi *semantics*
   (value selected on a live path, then computed on) has zero coverage.
   Any pass that reasons about phi values (R3's three, and any future
   loop pass) is green-on-vacuum until the generator grows C4/C6
   capability or hand fixtures carry the load.
3. The mutation battery and the fold table inherit the numeric domain
   (C9): nothing in the corpus probes boundary values; the property
   oracle (R1 lane 1) is the correct complement and should land.
4. Corpus-invisible constructs are still *verifier*-visible where unit
   tests exist (V18/V19 call checks, V14 join dupes, etc.) — this audit
   bounds the **corpus**, not the unit-test surface.

## 4. Recommended generator follow-ups (judgment, not measured)

Judgment, in priority order for R3's needs: (a) allow phis at spine
head and let later cells consume phi values — kills C4+C6, the two rows
that most blind R3; (b) non-i32 phis (C2) — one-line class of fix;
(c) boundary constants behind a flag (C9) — pairs with the property
oracle; (d) multi-fabric programs (C1) — bigger lift, belongs to the
M5 follow-on, not R1. None of these are scheduled by this document;
R3's gate (§6 of NEXT-PHASE) should decide whether (a) is a prerequisite.

— audited by the R1 lane-2 subagent. Raw dumps preserved verbatim:
`docs/phase/shape-audit-raw-0xFAB1C.txt`,
`docs/phase/shape-audit-raw-0xD3CA5.txt`; re-runnable in ~2 s each.

# REGION-SPIKE — the region-edit vocabulary on real fabrics

*R2 lane A, 2026-08-30. The R3 gate-decider (GATE-W2 §4 "region-edit
spike", NEXT-PHASE §6). A SPIKE, not production: the API is ugly on
purpose; what is real is that every edit below is **verify-legal on
bred fabrics** and every number was measured this session. House
rules: measured or it didn't happen; failures first-class; undersell.*

Worktree `quilt-llvm-wt-region`, branch `r2-region-spike` (base
`a556e4f` + the GA corpus spike `3874865` cherry-picked for the fabric
source). Reproduction:

```
cd experiments/llvm-fabric
cargo test                        # 162/162 (143 baseline + 19 spike)
cargo run --release --bin region-spike
```

## 0. Verdicts up front

| pass | core surgery on real fabric | verify-legal | semantics (property oracle) | replay bit-identity |
|---|---|---|---|---|
| const-branch fold | **done** — Br→Jmp + phi maintenance, 142 bred sites | **140/140** | preserved wherever decidable (0 changed, ever) | **140/140 identical** |
| region-DCE | **done** — 191 dead regions removed across 140 bred fabrics | **140/140** | preserved wherever decidable | **0/140 — inexpressible today** (the §6 answer) |
| CFG-graft inline | **done** — 34 multi-region callee programs | **34/34** | preserved (11 decidable, 0 changed) | **0/34 — inexpressible today** |

**R3 is a build job, not a redesign — with one named debt:** the diff
vocabulary must grow region-granular edit kinds (`RegionRemoved`,
`RegionAdded`/move, join-relabel) or region-stable ids, because
cell-level decomposition replays the fold perfectly but **cannot
express region compaction, cell moves, or join relabeling** (§4).
That is a vocabulary extension, the smallest kind of redesign — and it
is now *measured*, not hypothesized.

The #1 blocker ("phi-join maintenance when a predecessor edge
disappears", v0 §6.1 → v1 §8.4.1 → §9.5.5) has a working operator:
**95.4% verify-legal on seed-corpus arms, 99.3% on bred arms**, with
the residual refusal class diagnosed and named (§3.1) — down from
semmut's 49.2% on the same generator distribution.

## 1. Method

**Fabric source (the GA corpus).** Breeding exactly as the GA spike
(`ga::run_keep`, pop 200 × 50 gens, seed 0x6A1C0): 140/200 final
population verify-green — **the measured corpus is these 140 bred
fabrics**, plus the 200-fabric GA **seed** corpus (ga.rs gen-0 =
`fuzz::gen_fabric`, the GA's own seeding) for the legs where the task
names it. GA mutation pressure matters here and is stated wherever it
biases a number (§3.3).

**API surface (src/region.rs, spike-grade).** `region_add`,
`region_remove` (refuses reachable/referenced; compacts region ids and
remaps every reference), `drop_edge` (the semmut
join-drop-with-edge surgery **factored out of `semmut.rs` and made
semantics-preserving**: the kept arm is the one the condition selects,
not a random one), `join_phi` (the maintenance inverse), `region_graft`
(closed-region copy with id/region remaps), plus three passes built on
them: `const_branch_fold`, `region_dce`, `cfg_graft_inline`.
REUSE, not reimplementation: `drop_edge`'s Br→Jmp + join/operand strip
is semmut's operator; the fold's arithmetic is `semmut::eval_dataflow`
(Rust checked arithmetic); `region_dce` is dce.rs's booked deferral
("unreachable REGIONS are not removed"); `cfg_graft_inline` is
inline.rs's booked deferral ("CFG grafting needs the region-edit
vocabulary") and shares its skip-note-never-silent contract.

**The property oracle (`region::interp`).** A spike-grade concrete
interpreter: walks regions from entry, requires every branch cond on
the executed path to be dataflow-const, resolves phis through the
region the path entered from, resolves calls by const-substitution
into the callee. Arithmetic kernels are constfold's `eval_arith`/
`eval_cmp` — the same kernels the R1 fold-table oracle audits against
Rust. "Semantics preserved" = interp answer unchanged; fabrics it
cannot decide are counted **unjudgeable, never assumed** (§3.2 states
how thin decidability is on this corpus).

**Red/green (D1).** Each pass has a green fixture (edit → verify green
+ oracle preserved) and red sabotages that MUST fire (fold: wrong-arm
fold flips the oracle answer while verify stays green — the exact
silent-wrong-wire class; join-strip on the wrong arm fails V06;
inline: misbound phi join fails V06, misbound value retarget flips
the oracle; dce: removing a reachable region is refused, stale joins
fail verify). 19 spike tests; the suite's sabotage count is what makes
the oracle's silence meaningful.

## 2. Measured (release, this session)

### 2.1 Material (140 bred fabrics)

| construct | bred | GA seed corpus |
|---|---|---|
| const-conditioned branches (distinct arms) | 142 sites / 136 fabrics | 121 sites / 89 fabrics |
| unreachable regions | 191 regions / **140 fabrics (100%)** | 114/200 fabrics (57%, matches audit) |
| inline-eligible callees (verify + acyclic entry + uniform rets) | **0** | 34/200 |

### 2.2 The passes

| | attempted | verify green | oracle: preserved / changed / unjudgeable | replay identical |
|---|---|---|---|---|
| **A** const-branch fold (bred) | 140 | 140 | 4 / **0** / 136 | **140** |
| A on seed corpus | 89 (121 sites) | 89 | (20 decidable across A+B, all preserved) | 89 |
| **B** region-DCE (bred) | 140 (191 dead regions) | 140 | 4 / **0** / 136 | **0** |
| B on seed corpus | 114 | 110 (4 refuse, §3.1) | — | 0 |
| **C** CFG-graft inline | 34 programs (multi-region callees) | 34 | 11 / **0** / 23 | **0** |

Oracle on param-constified twins (consts substituted for params to
widen decidability): A 4/4 preserved, B 4/4, C 11/11. **Zero changed
answers anywhere, in any leg.**

Throughput ( bred fabrics, release):

| op | ops/sec |
|---|---|
| const_branch_fold (pass invocations) | ~3,900 |
| region_dce (pass invocations / region removals) | ~4,100 / ~5,600 |
| cfg_graft_inline (pass invocations) | ~730 |
| drop_edge (raw op, incl. verify) | ~760–1,700 |
| region_add | ~120k–167k |
| region_remove (raw, incl. refusals) | ~640k–1.4M |
| join_phi (guard path / legal path) | ~2.1M / ~5–7k |

(Numbers vary run to run by ±2×; these are single-run release numbers,
not benchmarks. The corpus is tiny (≤~90 cells), so per-op cost is
dominated by `verify` and the O(n) `predecessors`/`uses_of` scans —
the R2 use/pred tables are the known fix, which is why GATE-W2 called
them an R3 prerequisite.)

### 2.3 Phi-join maintenance (the #1 blocker, measured)

`drop_edge` on **every branch arm of every fabric**:

| corpus | arms | verify-legal | refused (diagnosed §3.1) | red |
|---|---|---|---|---|
| 140 bred fabrics | 564 | **560 (99.3%)** | 4 | 0 |
| 200 GA seed fabrics | 476 | **454 (95.4%)** | 22 | 0 |
| semmut join-drop-with-edge, same generator distribution (GATE-W2) | 250 | 123 (49.2%) | — | 127 invalid |

The improvement over semmut is the strengthened single-join strategy
(§3.1): collapse the phi to its operand when legality allows, else
materialize the operand as a const in entry, else refuse loudly.
Zero-join phis — semmut's silent V05 killer — are now either rewritten
or a named refusal; the strategy is exercised by dedicated red/green
tests including both fix paths.

## 3. Failures, first-class

**3.1 The residual refusal class (named, not hidden).** A phi whose
ONLY join is the dying edge, whose operand is a **non-const cell
defined in another region**: V12/V07 forbid rewiring users to it, no
const exists to materialize, so `drop_edge`/`region_dce` refuse
(`"cannot legally replace it — refusing"`). Measured: 4/564 bred arms,
22/476 seed arms, 4/114 seed-corpus dce fabrics. This class needs
either value duplication across regions (a copy-prop step) or
dead-region co-removal (fold→dce composition catches most of it — the
pass pipeline composes green in the bred test). It is the honest
boundary of edge-drop phi maintenance in this IR, and R3's pass design
should compose around it rather than pretend it away.

**3.2 Oracle decidability is thin on bred fabric: 4/140.** Bred
fabrics are loop-heavy (GA `mut_grow` adds back-edges; self-joining
phis appear) and interp honestly returns None for loop-carried phi
chains and param-dependent conds (params ARE the corpus's shape).
Twin-constification widens decidability only slightly on bred
material (the loops remain). The 0-changed record above is therefore
*strong where it applies* (every decidable fabric, both corpora, three
passes, plus 4/4 decidable drop-edge arms) and the sabotage battery is
what proves the oracle fires when wrong. An execution semantics for
loops is the M2 lane's question, not this spike's.

**3.3 GA pressure destroys inline-eligibility: 0/140 bred callees.**
`mut_grow`'s back-edges give every bred fabric entry predecessors; the
callee pool for pass C is therefore the GA **seed** corpus (34
eligible), which the task explicitly names as the fabric source.
Measured honestly: the GA as configured cannot supply CFG-inline
material; R3's generator prerequisite (NEXT-PHASE §10.4) should say
so.

**3.4 Replay bit-identity: the §6 answer, measured.** Fold replays
**bit-identically 140/140 + 89/89** — Br→Jmp and phi-strip decompose
cleanly into AddCell/RemoveCell/Retarget (built by replay-applying the
own edit stream, so identity is by construction and re-checked).
Region-DCE and CFG-inline replay **0/140 and 0/34**: region
compaction, cell moves (entry→continuation), and join relabels have no
Edit kind; the recorded cell-level edits alone cannot reproduce the
fabric. NEXT-PHASE §6 asked whether "bit-identical replay holds" for
region edits: **yes where the edit is cell-decomposable, no for
region-granular moves — the diff model survives as a base, and needs
region-granular kinds added, not a redesign of N4 itself.**

**3.5 A bug the seed-corpus leg caught mid-spike.** The first
single-join strategy kept one-join muxes with a stale join (V06) and
the collapse check treated phi users as V12 cells (they are V07
cells: the operand must live in the *join* region or entry) — 4 V07
reds on seed fabric. Both fixed (§2.3 shows 0 red); recorded because
the bred-only leg was blind to it (bred fabrics don't hit the case)
and because it is the house method working: the second corpus caught
what the first could not.

## 4. What this means for R3

1. **Proceed.** The three blocked passes' core surgery runs
   verify-legal on real material with semantics preserved everywhere
   decidable, at spike scope. GATE-W2's "build job rather than
   redesign" judgment survives contact, now with numbers.
2. **The diff-vocabulary debt is specific:** add region-granular edit
   kinds (RegionRemoved with id compaction or region-stable ids,
   MoveCell, join-relabel-as-edit) and replay/conservation/weft extend
   mechanically — the fold already proves the cell-decomposable half.
   Death certificates for removed regions ride the RemoveCell ledger
   (the spike carries ledger entries already); `verify_deaths` recomputation
   is R3 work, not spike work (none of the spike's removals tombstone).
3. **Compose fold→DCE** to consume §3.1's refusal class (the stranded
   arm usually becomes fully unreachable, which DCE then removes).
4. **The generator prerequisite grows a second clause:** the corpus
   can supply const-branch and unreachable-region material, but
   CFG-inline callees need either seed-corpus fabrics (as here) or a
   GA mutation that preserves acyclic entries.
5. **The property oracle is worth keeping:** `interp` + the sabotage
   battery caught every deliberate wrongness this spike planted
   (wrong arm, wrong join, wrong value). It should ride into R3 as the
   passes' red/green fixture base, with the loop-semantics gap stated
   wherever it is silent.

## 5. Demo fabrics (from the run, verbatim)

Fold (bred fabric, 1 site): entry `%35 = br %25, r3, r2` with
`%25 = const i1 true` → `jump r3`; the el/r2 join maintenance keeps
verify green. DCE (bred fabric, 1 region removed): a never-entered
region's cells leave with ledgered RemoveCells, ids compact, live
phis' dead joins stripped. Inline (seed-corpus callee, 3 regions):
diamond callee `pick(8,31)` grafts as three fresh regions + a
continuation region; the call's use retargets through a fresh return
phi; provenance then walks caller consts through the graft — the exact
boundary v1's inliner could not cross. Full texts in the binary's
output (`cargo run --release --bin region-spike`), deterministic.

## 6. Test inventory (19 new)

`region::tests` (17): interp ×2 (decidable diamond, param unjudgeable,
call resolution); drop_edge ×2 (verify+replay, zero-join collapse);
fold ×3 (green+replay, wrong-arm red, wrong-join-strip red); dce ×3
(green+semantics, reachable-refused red, stale-join red); inline ×4
(green multi-region+provenance, misbound-join red, wrong-value red,
noted-skip); join_phi ×1; region_remove refusals ×1; bred end-to-end
×1 (GA-bred fabric through fold→replay→dce, composed green).
`region::materialize_tests` (2): materialize green (verify+replay+
semantics), non-const refusal red.

*Verdicts: all three passes' core surgery is real and measured on GA
corpus material. R3 should proceed with the diff-vocabulary extension
named in §4.2 and the §3 refusal classes on the books.*

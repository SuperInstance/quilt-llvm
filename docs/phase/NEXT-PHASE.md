# NEXT-PHASE — the plan

*Strategic pass, 2026-08-30. Synthesized from the foreman's brief
(NEXT-PHASE-BRIEF.md), Opus 5's round-1 position, and two rival
critiques (round2-critique-glm.md, round2-critique-opencode.md).
House rules apply: measured numbers or it didn't happen; judgment
labeled as judgment; failures first-class; undersell.*

**Tree state at planning time:** `90d38b0`, `cargo test` re-run this
session → **121 passed, 0 failed** (D7). M3 (ledger pass manager) and
M4 (DCE-as-decay) landed mid-round-1; this plan is written against the
post-M4 tree, not the tree the brief described.

---

## 0. A note on the evidence base for this plan

Two rival critiques were commissioned. **Both were produced by
GLM-5.3** — the second file's own header reads
`Round-2 rival critique (opencode / GLM-5.3, 2026-08-30)`. Their
agreement is therefore *one model's judgment stated twice*, not two
independent judgments converging.

This is worth stating plainly because it is the same defect this phase
exists to fix, appearing in the planning process itself: an oracle that
looks like two checks but is one. Their consensus is weighted here as
**one strong critique**, which is why §7(b) still holds a position
against it. Their arguments are adopted on their merits, not on their
count.

---

## 1. Phase theme (final)

> **Audit the judges, then pay the debt.** This phase stops trusting
> its own numbers before it spends them — it audits the corpus
> generator, gives the fold table an independent oracle, and splits the
> verifier into tiers with a measured cost — and then spends the
> remainder of its budget on region-edit vocabulary, the debt the docs
> have now named #1 three times.

**Why now.** Round 1 proposed *semantics-first* on the strength of a
sabotage probe. Both rivals attacked the scoping and both were right;
the measurements below show the semantic hole is real but **narrow and
cheap**, not phase-sized. What is *not* cheap, and what neither the
round-1 theme nor the region-first alternative budgeted, is
OpenCode's finding (§7d): the corpus generator is the untested oracle
sitting underneath every number in EXPERIMENTS.md. That is the root
cause of both blind-spot incidents, it is upstream of both candidate
themes, and it costs days. Fix the judges first because they are cheap;
then spend the real budget on the real debt.

**What the theme is not.** It is not "build M2." M2 stays deferred, and
§5 makes that a written decision rather than a silent drift.

---

## 2. What round 1 got wrong (measured, not conceded politely)

Round 1 claimed the fleet's oracle set was blind to semantic corruption
and proposed a one-week self-differential probe. I re-ran the sabotage
battery against the current tree and then **priced the rivals' cheaper
alternative instead of arguing about it**.

Method: `git archive 90d38b0 experiments/llvm-fabric` into a scratch
pin (concurrent lanes make the live tree unsafe to measure in — §7c),
one-line type-correct corruptions of the fold table, `cargo test`.

| sabotage | existing 121-test suite | + property oracle (~55 lines) |
|---|---|---|
| i32 add → x*y+1 | FAILED (5 tests) | FAILED |
| i64 add → x*y+1 | **ok. 121 passed** | FAILED |
| i32 sub → y-x (swapped) | **ok. 121 passed** | FAILED |
| i32 mul → x+y | **ok. 121 passed** | FAILED |
| i32 div → x*y | FAILED (1 test) | FAILED |
| cmp i32 Lt → ≤ | **ok. 121 passed** | FAILED |
| cmp i64 Ge → > | **ok. 121 passed** | FAILED |
| inline: args bound reversed | FAILED (1 test) | FAILED |
| decay: liveness skips 1st operand | FAILED (6 tests) | FAILED |

**Readings, undersold:**

- **5 of 7 fold-table corruptions survive the full suite** including
  the 10,000-fabric corpus. Real hole, confirmed at the current hash.
- **A ~55-line property oracle kills 7 of 7.** It compares `eval_arith`
  / `eval_cmp` against Rust's own checked arithmetic over a 15×15
  operand grid (zeros, ±1, ±2, MAX, MIN, MAX−1, MIN+1). Suite goes
  121 → 123 tests, 0.22 s → 0.53 s. This is an **hours** task, not
  GLM's 1–2 days and not round 1's one week.
- **The transform-level sabotages were already caught** — inline
  arg-binding by exactly one test, decay liveness by six. Round 1
  implied the transforms were as exposed as the fold table. They are
  not. That framing was wrong.
- **The residual is not a wrong implementation — it is an absent one.**
  There is no execution semantics for phis, control flow, or pass
  composition anywhere in the crate, so there is nothing to sabotage.
  You cannot mutation-test code that does not exist. That is what M2
  would buy, and it is why M2 is a *later* investment (§6, T2), not
  this phase's theme.

Round 1's self-differential probe is **withdrawn** (§7a).

---

## 3. Sprint rounds

Ordered by dependency. Durations are judgment; exit criteria are
commands that pass or numbers that get published, per ARCHITECTURE §4.

### R1 — Price the judges (1 week, hard cap)

**Goal:** every oracle this fleet cites gets a measured statement of
what it can and cannot detect.

**Lanes:**
1. **Property oracle** for `eval_arith` / `eval_cmp` vs Rust checked
   arithmetic. Already spiked and measured (§2); this lands it in-tree
   with the grid widened to f64 and i1.
2. **Generator shape audit** (OpenCode §7d, elevated from a track to a
   sprint lane). Publish histograms over the 10k corpus: phi count,
   ctrl-edge count, call depth, region count per fabric, type coverage,
   cyclic-region frequency — each against the *spec surface* in
   ARCHITECTURE §2.1. The deliverable is the list of constructs the
   generator **cannot emit**, because that list bounds the validity of
   every "10,000/10,000, 0 failures" claim in EXPERIMENTS.md.
3. **Semantic mutation tier.** The battery today runs 4,430 mutants and
   every rejection is a structural V-code. Add mutants that are
   structurally valid and semantically wrong (flip a fold result, swap
   an arg binding, drop a phi join) and report the kill rate per tier.

**Exit criteria (measured):**
- Property tests in-tree; suite ≥ 123/123 green; the seven-sabotage
  battery reproduced in CI as a *documented* red/green fixture (D1).
- A published `SHAPE-AUDIT` table naming every spec construct the
  generator cannot produce. Baseline already known and damning: 66.7%
  of generated fabric is dead on arrival, mean 25.5 cells, no memory,
  no real loops, straight-line callees only (§9.2, §9.4.3).
- Semantic-mutant kill rate published as a number. Today's measured
  baseline: **5/7 fold corruptions survive**.

**Why capped at one week:** GLM's instinct was right and §2 proves it —
the work is hours. The cap exists so the lane cannot expand into the
interpreter it is explicitly not building.

### R2 — One verifier becomes two (1.5–2 weeks)

**Goal:** kill the largest measured cost in the pipeline, and produce
the tier-spread number that cascade R&D has been asserting without.

**Re-measured this session** (`cargo run --release --bin llvm-fabric -- bench`):

```
shape          cells   print-us   verify-us
chain-50          53        7.6        2.2
diamonds-160    1443      167.4     1413.7
```

27.2× the cells → **642.6× the verify time ≈ O(n^1.96)**. Verify is now
8.4× the cost of a print at diamonds-160. It is the dominant cost and
it is getting worse with size.

**Lanes:** maintained use/pred tables (§8.4.3, booked twice); split
`verify` into a cheap tier (structural, per-edit, incremental) and the
full walk; measure the spread.

**Exit criteria (measured):**
- Verify scaling re-measured across the 53 → 1443 cell curve and
  published; target is an exponent ≤ 1.2. Anything above 1.5 means the
  table rewrite did not work and says so.
- **Cheap-tier / full-walk cost ratio published as a single number.**
  This is the gate for T3 (§4) and it is the only reason cascade is
  still alive as a question.
- 121+ tests green; corpus unchanged in outcome.

### R3 — Region-edit vocabulary (3–4 weeks; the bulk of the phase)

**Goal:** the debt named #1 in v0 §6.1, again in v1 §8.4.1, and again
in §9.5.5. It blocks const-branch folding, region-DCE, and CFG-graft
inlining — three named passes.

**Gated on a 2–3 day spike, not started day one.** See §6; this is
where I hold a position against OpenCode.

**Lanes:** `RegionAdded` / `RegionRemoved` / `JoinDropped` in the diff
vocabulary; replay, conservation, weft-chain and death certificates
extended to region-granular edits; then the three unlocked passes, in
that order (const-branch fold → region-DCE → CFG-graft inline).

**Exit criteria (measured), per pass:**
- Red/green (D1): suite fails with the pass stubbed out.
- Every-prefix replay bit-identical (the standard `7378df3` hardened).
- Conservation green; region deaths carry certificates that
  `verify_deaths` recomputes, same contract as M4.
- Corpus green **on a generator that can emit the shapes** — this is
  why R1 lane 2 is a dependency, not a nicety. A region-DCE that is
  green on a corpus containing no unreachable regions has proven
  nothing.

### R4 — quilt-scratch: the external oracle (parallel; gated on humans)

**Goal:** the one lane where the oracle cannot be built, only
scheduled. Runs alongside R1–R3 because its blocker is calendar time,
not engineering time.

**Lanes:** kid-testing sessions; vibe-panel deploy (blocked, §5);
TA bridge prod cadence (blocked, §5); remaining story threads.

**Exit criteria (measured):** N kids × M minutes observed, written up
with failures first, one-new-tile-per-level rule tested against actual
confusion. A session that produces no recorded failure is reported as a
methodology failure, not a success.

**Undersold honestly:** if procurement (§5) does not start in week 1,
this round does not happen this phase. That is the single most likely
silent slip in the plan and both rivals flagged it independently.

---

## 4. R&D tracks and kill criteria

| # | Track | Funded? | Kill / revisit criterion |
|---|---|---|---|
| T1 | Semantic mutation battery | **Yes** (R1 lane 3) | Closes on success: if kill rate ≥ 95% after the property oracle lands, the track is *done*, not continued. No interpreter needed. |
| T2 | M2 — interpreter + gcc differential | **Gated, not funded** | Opens only if BOTH: T1 leaves a residual class no property oracle can reach, AND R3 ships CFG-graft inlining (composition bugs a fold-table oracle structurally cannot see). Otherwise M2 stays booked-not-scheduled and §5 records that as a decision. |
| T3 | Verification cascade (battens) | **Unfunded this phase** | Revisit only if R2 publishes a cheap/full tier ratio **≥ 5×**. Ratio < 5× kills it permanently — B.4's +7.5 pts over trivial does not survive a thinner spread. |
| T4 | GA-fuzz — mud-arena's genetic engine breeding toward verifier failures | **Yes**, small (pairs with R1 lane 2) | Kill if 3 generations produce no new V-code hit and no new semantic-mutant kill. Log every fitness number (CROSS-POLLINATION §4's own caveat). |
| T5 | MerkleMesh × Weft cross-impl root | **Yes**, small | Kill to "export-only, no shared root" if a bit-identical Rust↔TS canonicalization fixture suite is not green in 3 days. Canonicalization drift is the silent-corruption failure mode; an unverified shared root is worse than none. |
| T6 | tit-quilt tombstone shape → M4 decay certificates | **Yes**, small | Kill if the tombstone record cannot carry `killer/tick/witness` without weakening `verify_deaths`. Absorb the shape or drop it; do not fork the contract. |
| T7 | JEPA room-sense in quilt-scratch | **Unfunded**, speculative | Opens only after R4 produces one kid-observed failure that room-sense would plausibly fix. No kid data, no track. |

**Note on T3.** Both rivals said defer; I concede fully and go further —
the round-1 position already held that the cascade line was a hypothesis
dressed as a finding, and OpenCode's framing is sharper than mine was:
*you cannot route between two tiers when one tier exists.* R2 builds the
second tier because it is worth building on its own merits; whether a
router is ever warranted is next phase's question, decided by a number.

---

## 5. Casey decisions — what blocks what

**Decisions (resolvable by choosing):**

| Decision | Blocks | Cost of delay |
|---|---|---|
| vibe-panel Worker secrets | vibe-panel deploy → amplifier live loop → part of R4 | R4 runs on mock transport; the learning loop stays unvalidated against real play |
| DeepSeek key top-up | cheap bulk lanes | R1/R2 run on GLM + Claude only; judgment: schedule stretches, no lane dies |
| TA bridge prod cadence (cron home, event cadence) | ta-bridge cutover | The amplifier's event pipeline stays dev-only |
| **Ladder amendment: does M2 stay deferred?** | Nothing technically; everything doctrinally | See below |

**The ladder amendment is the one new decision this plan surfaces.**
ARCHITECTURE §4 states M2 is load-bearing and *"must precede any
transform."* The fleet has now shipped M3, M4, and M5 past it. GLM's
sharpest point (§7d of its critique) is that the oracle blindness was
therefore **self-inflicted plan deviation, not a discovery** — and that
is correct, and round 1 missed it. Either §4 is amended in writing to
reflect the order actually taken and the compensating controls (property
oracle, semantic mutants, death certificates), or the doctrine is being
violated silently, which D8 calls an incident. **This is Casey's call,
not the strategic lane's.** The plan assumes amendment; if the answer is
"no, M2 comes first," R3 is displaced and the phase re-plans.

**Long-lead procurement (NOT decisions — start now or lose the phase):**

- **Kid testing.** Guardian consent, participant recruitment, session
  scheduling, and a written observation protocol. Longest lead time in
  the phase and the **only external oracle quilt-scratch has**. Both
  rivals flagged it independently as under-filed. It is listed here as
  procurement, not as an R&D track, specifically so it cannot be
  quietly reprioritized into next phase. **Start week 1 regardless of
  where the code lanes are.**

---

## 6. The likeliest failure, and the cheapest experiment to find it early

**Most likely to fail: R3.** Not for lack of priority — it has been #1
three times and has not been built. The plausible reason it keeps
slipping is that it is genuinely hard in a specific way: **all three
enforcement laws are defined over cell edits, and a region is a coarser
entity.** N4 replay, conservation, and the death certificates each ask
"what happened to this cell," and a region removal has no obvious
answer. Does removing a region record one edit or N+1? Bit-identical
replay is exactly where that ambiguity breaks, and it breaks after
weeks of construction, not before.

**Cheapest experiment (2–3 days, before R3's real work starts):**

Build **one** region edit — `RegionRemoved` for a provably-unreachable
region — and drive it, and nothing else, through the machinery that
already exists: every-prefix replay, `conserve::check_pipeline`,
`check_weft` / `verify_chain`, and `verify_deaths`. Run it over the 10k
corpus.

- **If bit-identical replay holds and region deaths certify:** the diff
  model survives contact. R3 is a build job and proceeds on schedule.
- **If it does not:** R3 is a *redesign* of the diff model, not a
  feature, and the phase re-plans in **week 2 instead of week 5.**

This is the experiment that decides whether the bulk of the phase is
correctly scoped, and it costs less than 3% of the phase to run.

**This is where I hold a position against OpenCode**, which wrote
"start region vocabulary on day one regardless." I disagree: day-one
construction against an unvalidated diff model is exactly how a
redesign gets discovered in week five. Two days of gate first. Given
that both critiques came from one model (§0), this disagreement is with
one opinion, not a consensus.

---

## 7. Explicit verdicts on the three contested items

**(a) The self-differential probe — WITHDRAWN.** Both rivals attacked
it; OpenCode's form of the attack is decisive and better than the
version round 1 raised against itself: *if the minimal evaluator shares
any semantics with the pipeline, `eval(pre) == eval(post)` holds with
the same wrong arithmetic — the probe is blind to precisely the bug
class that motivates it.* Round 1 called the interpreter "not an
external oracle"; it did not follow that through to noticing the probe
was therefore worthless for its stated purpose. The measurements in §2
close the argument: the property oracle kills 7/7 for ~55 lines and
sub-second runtime. A week spent confirming a foregone conclusion is a
week not spent on R3.

**(b) Cascade funding — UNFUNDED, gate written.** Concede to both. Kill
criterion in §4 T3: revisit only on a published tier ratio ≥ 5×, killed
permanently below that. The precondition (O(n) verify, a real second
tier) is funded on its own merits as R2, which is GLM's point and it is
correct — the dependency was inverted.

**(c) D9 "one lane, one worktree" — REJECTED AS LAW, ADOPTED AS
PROTOCOL.** Both rivals rejected the legislation and both were right,
for the same reason stated two ways: **D1–D8 are checkable properties of
artifacts; D9 would be a property of operators.** A law you cannot check
in a diff is not the same kind of object as the other eight, and
OpenCode's point that every law past the load-bearing eight dilutes the
eight is correct. Adopted instead as a lane pre-flight check:
clean `git status`, own worktree, before a lane starts.

The underlying incidents are real and stay on the record: the
batten-spike vendored a pin at `2e5469e` because *"the live llvm-fabric
tree had uncommitted WIP at spike time and did not compile,"* and during
round 1 of this planning pass a concurrent lane's in-flight
`manager.rs` + modified `lib.rs` broke the tree mid-measurement. Both
measurement passes in this document were therefore run against
`git archive` pins, not the live tree. That is the practice; it does not
need to be a law to be followed.

---

## 8. Round structure at a glance

```
week  1    2    3    4    5    6    7
R1    ####                                  price the judges (capped)
gate       ##                               region-edit spike (2-3d, §6)
R2         #########                        verify tiers + O(n) tables
R3              ###################         region vocabulary + 3 passes
R4    ===================================   quilt-scratch (human-gated)
T4/T5/T6   ....  ....  ....                 small tracks, kill criteria §4
```

Judgment, not measurement: the week counts are estimates. The gate at
week 2 is the only hard scheduling commitment, because it is the one
that can re-plan everything downstream cheaply.

---

## 9. What this plan does not claim

- It does not claim the fleet's transforms are semantically correct. It
  claims 7 of 7 known fold corruptions are caught after R1, and that
  phi/control/composition semantics remain **unjudged because no
  execution semantics exists** (§2).
- It does not claim region vocabulary is achievable in 3–4 weeks. It
  claims a 2–3 day experiment will tell us cheaply whether that estimate
  is worth anything (§6).
- It does not claim cascade is dead. It claims it is unfunded pending
  one number (§4 T3).
- It does not claim quilt-scratch's engine works for kids. No kid has
  used it. That is the point of R4, and procurement has not started.
- The cross-repo status lines in the brief (quilt-scratch 89/89,
  ta-bridge 23/23, vibe-panel 23/23) are **carried as reported, not
  re-run** by this lane (D7). Only quilt-llvm's 121/121 was re-run here.

---

# 10. Addendum — R1 adjudication after lanes 1 and 2 (2026-08-30)

*Written after the fold-table oracle landed (`f70fb5b`, suite 125/125)
and after reviewing the lane-2 shape audit on branch `r1-shape-audit`
(`e086555`, 121/121 on that branch). Reviewed adversarially, per the
house rule that a lane's findings are red tests, not prose.*

## 10.1 What I verified myself

Not taken on trust. Re-run from a `git archive e086555` pin:

- **Reproduces bit-for-bit.** cells 255,446 · phis 15,333 · calls 0 ·
  equal-target branches 969 · dead-on-arrival 41.3%. The cell and phi
  counts match EXPERIMENTS §9.2 exactly — a genuine external anchor.
- **C4 confirmed independently of the audit binary.** Using only the
  library API (`index_in_region` / `region().cells`) over 2,000 fabrics:
  **0 of 3,078 phis sit at a spine head.** And `verify.rs` contains no
  phi-placement check at all — V03/V04 check *terminator* position only.
  ARCHITECTURE.md:268 mandates "phi cells sit only at a spine head."
  **The spec rule is enforced nowhere and the generator emits its
  opposite 100% of the time.** This is the audit's most valuable find.

## 10.2 Accepted findings

1. **C4 (spec conflict)** — verified above. Promoted to a Casey decision
   (§10.5).
2. **C1 (0 calls)** — every M5/inline corpus claim rests on hand
   fixtures. EXPERIMENTS said so in prose; it is now a measured zero.
3. **C6's datum** — phi consumers are phi (5,866) and ret (1,455);
   arith/cmp/branch **0**. No pass decision in the corpus has ever
   depended on a phi's value.
4. **C9 × R1 lane 1 are complements, and this is mutual validation.**
   Corpus constants are confined to small non-boundary domains; the
   oracle's grid is exactly ±MAX/MIN/MAX−1. Neither could have found the
   other's bugs. Independent support that lane 1 was worth landing.
5. **The 7.6% equal-target correction.** Reading the code said 0; the
   histogram said 969/12,703. Measurement overruled reading, and the
   lane published the correction against itself. That is the house
   method working.
6. **§2.8 corrects THIS plan.** NEXT-PHASE §3 R3 worried that region-DCE
   would be "green on a corpus containing no unreachable regions."
   Measured false: **57.38% of fabrics contain ≥1**. My concern was
   wrong; R3 has real material. Recorded rather than quietly dropped.

## 10.3 Attacks that stand

1. **The measurer is untested — 564 lines, 0 tests, suite unchanged at
   121.** The audit's thesis is that unverified oracles produce
   confident wrong numbers; the audit is itself an unverified oracle.
   Two counters have external anchors (§9.2); the *decision-driving*
   ones — spine-head position, phi-consumers-by-kind, cyclic frequency,
   unreachable-region rate — have none. I verified C4 by hand and it
   held, which raises confidence without discharging the problem.
   **Action: ~60 lines of tests over hand-built fabrics of known shape,
   before these numbers are cited again.** Cheapest item in the phase,
   and it gates everything the audit is about to change. See §10.4.
2. **C6's gloss overstates its own datum.** "every loop-shaped fabric is
   cyclic but computation-free" contradicts EXPERIMENTS §8, which
   documents a genuine loop-carried influence cycle (seed 1026845:
   `add → its own region's gate → branch cond → back to the add`) with
   no phi involved. The corpus **does** carry loop-carried influence —
   through control wires, which is exactly what §8's ctrl machinery was
   built and tested on. The correct, narrower claim: **no phi
   participates in loop-carried dataflow.** The number is right; the
   sentence is not, and R3 would under-credit the ctrl lane if it read
   the sentence.
3. **"100% sit immediately before the terminator"** — my independent run
   gives 3,012/3,078 = **97.9%**, not 100%. Minor, but stated as exact.
4. **Generator ≠ corpus.** The cannot-emit list bounds `gen_fabric`
   output. Mutation is a second shape-production path (4,430 mutants,
   1,367 staying valid), and mutants reach `verify` — so
   verifier-derived claims (the V-code histogram) are bounded *less*
   tightly than pipeline-derived ones, which only ever see generator
   output. C5 notices this for one row; the general principle should be
   stated once.
5. **Two seeds is stability, not independence.** C1–C8 are properties of
   the generator's *code*; no second seed base can falsify them. Framed
   honestly as a stability run, but §2.9's "every conclusion reproduces"
   invites over-reading.

## 10.4 What changes in the plan

- **R1 gains lane 4: test the shape-audit binary** (~60 lines, hand-built
  fabrics of known shape). Blocking: SHAPE-AUDIT's numbers may not
  re-scope R3 until its measurer is pinned. §10.3(1).
- **R3's gate (§6) gains a material check per pass**, now measurable
  instead of assumed:
  - const-branch folding — **has material** (24.0% of branches are
    const-conditioned);
  - region-DCE — **has material** (57.38% of fabrics have an
    unreachable region);
  - CFG-graft inlining — **has none** (0 calls, C1);
  - any pass reasoning about phi *values* — **has none** (C6).
- **The generator follow-up SHAPE-AUDIT §4(a) is promoted from optional
  to an R3 prerequisite.** All three R3 passes reason about phi values;
  C4 + C6 mean that capability is untested by construction. Either the
  generator grows it or R3's exit criteria must name the hand fixtures
  carrying the load, explicitly.
- **§3 R3's unreachable-region worry is withdrawn** as measured false
  (§10.2.6).
- **Merge note:** `r1-shape-audit` predates the oracle and reports
  121/121; master is 125/125. The branch touches different files and
  should merge clean, but its suite line must be re-run post-merge or it
  will read as a regression.

## 10.5 New Casey decision

**C4: the spec says phi-at-spine-head; the verifier does not enforce it;
the generator violates it 100% of the time.** Three ways out — amend
ARCHITECTURE §2.1 to match the emitted placement, add a V-code and fix
the generator, or declare the rule advisory. This is the same class as
the M2 ladder amendment (§5): a written doctrine the implementation has
silently diverged from. Not the strategic lane's call. Until it is
decided, no corpus number should be cited as evidence about phi
placement in either direction.

## 10.6 R1 status

| lane | state | suite |
|---|---|---|
| 1 — fold-table oracle | **landed** (`f70fb5b`) | 125/125 on master |
| 2 — generator shape audit | done on branch, **unmerged**, measurer untested | 121/121 on branch |
| 3 — semantic mutation tier | branch `r1-sem-mutants` exists; not reviewed here | — |
| 4 — test the measurer | **new**, blocking (§10.4) | — |

R1's one-week cap still holds. Lane 4 is hours, not days.

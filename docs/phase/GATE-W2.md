# GATE-W2 — the week-2 gate verdict

*Adjudicated 2026-08-30 against R1 lanes 1–3 as docked. Every number
below was re-run in this session from `git archive` pins, not taken
from the lanes' reports. House rules: measured or it didn't happen;
judgment labeled; undersell.*

**Status: lane 4 landed (`cb8d201`) and all three branches merged
(`4e3251b`, `78f3c02`, `9850cf2`) while this was being written. Master
is **139/139 green**, re-run this session (D7). Verdict committed.**

---

## 1. What I verified myself

| claim | source | my re-run | verdict |
|---|---|---|---|
| S1: 1,127 input-level semantic mutants, 0 killed | `ec4ca13` | `semmut --iters 2000 --seed 24269` → judged 1,203, killed 76, tier 6.3%; control 76/76 by replay; confirmed-wrong 113; unjudgeable 1,002 | **reproduces exactly** |
| shape audit histograms | `e086555` | cells 255,446 · phis 15,333 · calls 0 · equal-target 969 | **reproduces bit-for-bit** |
| C4 (0 phis at spine head) | `e086555` | verified via library API only, 0/3,078 | **independently confirmed** |
| tombstone lane, 133/133 | `d02a1dc` | branch reports 121→133; does **not** contain the oracle | confirmed, see §4 |
| tombstone raises verify cost | *my hypothesis* | bench diamonds-160: **1416.7 µs** vs master 1413.7 µs | **measured false — withdrawn** |
| the four lanes will conflict | *my hypothesis* | all three branches merge clean; combined suite **139/139** | **measured false — withdrawn** |
| lane 4 closes the untested-measurer attack | `cb8d201` | 19 tests: one per detector on hand-built fabrics of known shape, two cross-checks to EXPERIMENTS §9.2, plus **14 detector mutations that each turn the suite red** | **attack closed** |

Two of my own concerns died on contact with measurement. Recorded
rather than dropped.

Lane 4 deserves a specific note: it did not just add tests, it applied
the *sabotage-battery method to the measurer itself* — 14 mutations of
the detectors, each required to turn the suite red. That is the right
generalization of R1 lane 1's technique, and it closes §10.3(1) of
NEXT-PHASE properly rather than nominally.

---

## 2. Are R1's exit criteria met?

| # | Exit criterion (NEXT-PHASE §3 R1) | Verdict |
|---|---|---|
| 1 | Property tests in-tree, suite ≥123, sabotage battery as a documented red/green fixture | **MET** — `f70fb5b`, 125/125. Caveat booked: no `.github/workflows` exists; `cargo test` is the gate. |
| 2 | Published table of what the generator cannot emit | **MET** — C1–C11, reproduced bit-for-bit; measurer now pinned by lane 4 (19 tests, 14 detector mutations). The condition I attached in NEXT-PHASE §10.4 is discharged. |
| 3 | Semantic-mutant kill rate published as a number | **MET** — two tiers, both reproduced. |

**R1 is met, 3/3, with no open items.** All four lanes are on master at
139/139. Do not reopen it.

**T1 closes.** 19/20 = 95.0% at J2 meets the written ≥95% criterion.
Per §4 T1 the track is *done, not continued* — with the lane's own
honest qualifier carried forward: the 95% is concentrated in the fold
table, so it measures that battery's fold density, not transform
safety. §5.1 is the counterexample inside the same run.

---

## 3. The finding that changes the gate

Lane 3's sharpest number is not the 95%. It is this pair:

- **input-level semantic mutants: 0 killed / 1,127**
- **output tamper control: 76 killed / 76**, all by replay bit-identity

The fleet's judges audit *the pipeline's own claims* perfectly and
audit *the semantics of what goes in* not at all. That is a sharper
statement than round 1's "the oracle set is self-referential": the
self-reference is precisely the boundary. Everything downstream of the
pipeline's assertions is checked; nothing upstream is.

**This guts the cascade rationale for R2.** R2's tier split existed for
two reasons — cost, and building a second tier so T3 could be judged.
But a "cheap tier" and a "full walk" are *both structural*, and the
structural judge set scores 0/1,127 on semantic mutants. Splitting a
blind judge yields two blind tiers. Routing between them can trade cost
for cost; it cannot trade accuracy for cost, because there is no
accuracy difference to trade.

**And R1's own shape audit undercuts R2's cost rationale.** C10: the
corpus never exceeds **63 cells**. The O(n^1.96) curve and the
"dominant cost" reading come entirely from hand-built bench shapes
(chain-800, diamonds-160). At the sizes the corpus actually produces,
verify costs ~2 µs. R2 was about to optimize a cost nothing currently
pays.

---

## 4. Gate call

### R2 — **GO, with scope adjusted.** Not the R2 that was written.

**KEEP — maintained use/pred tables.** Re-justified. The cost argument
is weak (§3), but the *structural* argument is strong and independent:
`predecessors()` and `uses_of()` are linear scans (fabric.rs:178;
EXPERIMENTS §4c), and R3's region-edit vocabulary will hammer pred/succ
queries on every region add, remove, and join drop. **The tables are an
R3 prerequisite, not a performance win.** That is the honest reason to
build them, and it survives §3 intact.

**DEMOTE — the cheap/full verify tier split.** Its only consumer was
T3, and §3 shows both tiers would be equally blind. Keep the O(n)
scaling re-measurement, which is nearly free once the tables exist, but
it is a byproduct now, not a gate deliverable.

**Amended exit criteria:**
- pred/succ and use tables maintained incrementally; every existing
  invariant green (139/139 baseline post-merge, not 121).
- Verify scaling re-measured across the 53 → 1443 curve; target
  exponent ≤ 1.2. Reported honestly as a bench-shape number that the
  corpus does not exercise (C10).
- **New:** per-judge **kill-rate-per-microsecond**, measured. Lane 3
  showed the only judge that fires is replay (76/76) while the most
  expensive judge, verify, fired 0/1,127. R2 should not tier verify by
  cost without knowing which judges earn their time. This is cheap and
  it is the question §3 actually raises.
- **Dropped:** "publish the cheap/full tier ratio as T3's gate."

### T3 (verification cascade) — gains a second condition, and it is currently unsatisfiable

The written kill criterion was "revisit only on a tier cost ratio ≥5×."
That is now **necessary but not sufficient**. Added: *the tiers must
differ in what they can detect.* No such difference exists to build
today (§3). I am not claiming T3 is killed — I have not measured the
ratio, and I will not claim a number I don't have. I am claiming **R2
no longer owes T3 a gate deliverable**, and that on current evidence T3
does not revive this phase regardless of the ratio.

### The region-edit spike — **GO in parallel, and it just got cheaper**

Unchanged as the decision-maker for R3 (NEXT-PHASE §6). One update
that materially de-risks it:

**Lane 3 accidentally built R3's hardest prerequisite.** `semmut`'s
`join-drop-with-edge` operator performs exactly the surgery the region
vocabulary needs — Br→Jmp on one arm plus join/operand removal in the
dropped arm's phis — and it produces a **verify-legal** fabric in
123/250 attempts. Phi-join maintenance when a predecessor edge
disappears is the thing v0 §6.1 deferred and every later doc has called
the #1 blocker. There is now working code for it in `src/semmut.rs`.

The spike should reuse it rather than reimplement it. Judgment, not
measured: this moves the spike from ~3 days toward ~2, and it raises my
confidence that R3 is a build job rather than a redesign.

### Merge — done, and my risk assessment was wrong

I dry-ran the merges expecting conflicts on `verify.rs` between the
tombstone retrofit and R2's table work, and predicted 137 tests. Both
predictions were wrong: all three merged clean at **139/139**
(125 + 2 + 12), and the fleet then landed them on master in that order
(`4e3251b`, `78f3c02`, `9850cf2`) while this verdict was being written.
No ordering constraint existed. Recorded because I raised the risk.

---

## 5. What does not change

- **M2 stays gated.** T2 condition 1 ("a residual class no property
  oracle can reach") is now demonstrated **twice** — `inline-noncomm-swap`
  and the 427 unjudgeable control mutants. Condition 2 (R3 ships
  CFG-graft inlining) remains unsatisfied. My own written gate says
  M2 stays booked-not-scheduled, and I am honoring it rather than
  relitigating it on fresh evidence I find persuasive.
- **The phase theme.** "Audit the judges, then pay the debt" is what the
  last week did, and the judges came back worse than assumed on input
  semantics and better than assumed on tamper detection. Unchanged.
- **Kid testing.** Still long-lead procurement, still unstarted as far
  as this lane knows, still the most likely silent slip in the phase.
  Nothing in R1 touched it.

## 6. One chore, not a sprint

`inline-noncomm-swap` survives every judge the fleet owns (J1 and J2).
The fix is one red test that inlines a callee containing a `sub`. Hours.
It should ride along with lane 4 rather than wait for R3, because the
inliner is *shipped* code with exactly one happy-path test behind it —
and lane 3's judgment, which I share, is that the same hole likely
exists for every transform detail exercised only by happy-path
fixtures.

---

*Verdicts: R1 met (3/3, lane 4 in flight). T1 closes. R2 go with the
tier split demoted and the tables re-justified as an R3 prerequisite.
T3 gains an unsatisfiable second condition. Region-edit spike go, and
cheaper than planned. M2 stays gated on its own written criteria.*

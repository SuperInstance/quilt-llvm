# NEXT-PHASE-R4 — the fourth round, conceived

*Strategy lane, 2026-08-31, against master `bb566bb` (R3 fully
merged). Suite re-run by this lane before writing: **226 green —
207 lib + 19 shape-audit, 0 failed** (D7). House rules: measured
numbers or it didn't happen; judgment labeled as judgment; failures
first-class; undersell in summary, overdeliver in body.*

---

## Summary (the undersell)

R4's spine is **the external differential**: lower verify-green
fabrics to real LLVM IR, execute them with a real toolchain
(`clang`/`lli` or equivalent), and compare against `region::interp`
across a 10k-scale corpus. This is the cheapest honest form of M2 —
the debt NEXT-PHASE T2 kept gated — and the gate's two written
conditions are now both satisfied by R3's shipped state (§2). The
named debts (replay completeness across all edit kinds, verify
pass-rate spread, name collisions, dead-region material, Hebbian
witness, verdict aggregation) run as priced secondary lanes, none
blocking the spine.

The kill condition is registered before any work (§5). If it fires,
R4 re-plans around its own wreckage in week 2, not week 6.

---

## 1. R3's honest state — PROVEN / PRICED / UNKNOWN

No laundering. Tapestry doctrine: an obligation is not a result.

### PROVEN (measured, reproducible, in-tree on `bb566bb`)

1. **The region-edit vocabulary exists and carries its weight.**
   RegionAdded / RegionRemoved / MoveCell / RelabelJoin are
   sanctioned, table-maintaining, and every region op records and
   applies real edits (lane 1, `65cf06b` lineage, merged).
2. **The DCE replay gap is closed**: 0/140 → 155/155 bit-identical
   on the lane-1 corpus, with a red-condition test proving the gap
   was the edit kind's to close. Inline replay closed as a
   consequence (34/34).
3. **The GA breeds real callees**: 0/140 → 140/200 on the default
   seed (163, 155 on the stability seeds), entry-acyclicity
   preserved by construction with a loud, zero-counting guard, and
   the bred corpus now runs the full pipeline + conservation battery
   green (140/140, 163/163, 155/155) — a battery that had never been
   run on bred fabric before and caught a real dangling-operand bug
   the first time it was (lane 2 §4).
4. **Three passes are graduated library citizens** with one shared
   verdict vocabulary (`passes::verdict::judge`) and a registered
   `PIPELINE_V2`; every new stage replays bit-identically, per pass
   and end-to-end (lane 3).
5. **The property oracle catches all 7 known fold corruptions**
   (R1, in-tree since `f70fb5b`), and the shape-audit measurer is
   pinned by 19 bin tests.

### PRICED (obligations on the record, not paid)

- **dce/inline replay completeness across ALL edit kinds** — lane 1
  closed the *region-granular* gap; the claim "every op records and
  applies real edits" is proven for the ops touched, and the corpus
  law now runs region-DCE on every generated fabric, but a
  systematic per-Edit-kind × per-op replay matrix has not been
  published as a table. Priced in R4-L2.
- **Verify pass-rate spread by seed: 71.0 / 78.0 / 80.0%** (lane 2
  §5.3). The stricter operators cost bred yield; named, unretuned.
  Priced in R4-L4 as *measurement first* — a menu-weight retune is
  forbidden until the spread is explained.
- **`ga_sink`/`ga{N}` name collisions** — cosmetic today (verify
  does not key on names), but a namespace debt that becomes real the
  moment any tool keys on names. Priced in R4-L3 (hours).
- **Dead-region material collapsed 691 → 58** — the entry-acyclicity
  fix's ratchet twin. The fix's price, honestly booked by lane 2.
  Priced in R4-L4 (a v0 variant targeting phi-free non-entry
  regions — one menu entry).
- **Hebbian witness = M4's v2 debt** — decay certificates carry
  `killer/tick` but the *witness* (what the decay was answering to)
  is deferred. Priced in R4-L5.
- **`bin/region-spike.rs` still runs its private `PassCounts`** —
  two verdict vocabularies in the tree. Priced in R4-L5 (aggregator
  `PassVerdict::merge` + rewire, lane 3's own named next commit).
- **`PIPELINE_V2` order is a scout's claim** — no differential
  measurement of alternative orders. *The spine prices this one
  for free* (§3): once an external outcome measure exists, pipeline
  order becomes measurable instead of argued.

### UNKNOWN (no oracle exists, stated as such)

- **Execution semantics for phi values, control flow, and pass
  composition.** The property oracle bounds arithmetic and
  comparison tables; `region::interp` bounds single-fabric answers
  *against itself*; nothing external has ever executed a fabric.
  NEXT-PHASE §2's verdict stands unamended: *"unjudged because no
  execution semantics exists."* This is the hole R4's spine is
  aimed at, and it is now the *only* structural hole left — R3
  closed the vocabulary and pass-layer versions of it.
- Whether the 71–80% verify spread is operator strictness, V16
  phi-join pressure, or corpus-shape interaction. Hypotheses exist;
  no decomposition published.

### Carried as reported, not re-run (D7, per NEXT-PHASE §9)

Cross-repo status lines (quilt-scratch, ta-bridge, vibe-panel) are
out of this lane's scope. quilt-llvm's 226/226 was re-run here.

---

## 2. The spine: the external differential (M2, paid in its cheapest honest form)

### 2.1 Why this spine, and why the alternatives are not it

**The case in one sentence:** every oracle in the tree agrees with
the tree — the property oracle checks Rust arithmetic against Rust
arithmetic's own semantics, interp checks the pipeline against
interp, verify checks shapes against the spec — and after R3 closed
the vocabulary debts, the only remaining structural unknown
(§1 UNKNOWN) is precisely the class *no self-referential oracle can
see*: phi placement, control flow, and pass composition as they
affect *what a program computes*. One real compiler execution is
worth more epistemically than any number of internal agreements,
because it is the first judge that does not share a codebase with
the accused.

**Against the four candidates from the brief:**

- *Self-differential evaluator probe at 10k scale* — **withdrawn
  doctrine, not a candidate.** NEXT-PHASE §7(a) killed it with the
  decisive argument: a minimal evaluator sharing any semantics with
  the pipeline holds `eval(pre) == eval(post)` through the same
  wrong arithmetic. Growing the withdrawn probe to 10k would be
  laundering a refuted idea by scale. What survives from it — the
  property oracle — already landed in R1 and does its job in
  sub-second runtime.
- *Pipeline vs gcc -O1* — **this is the spine**, with one amendment:
  the comparison is *outcome-level* first (does the optimized
  program compute the same answers), *codegen-level* only if the
  outcomes already agree. We are not claiming to beat gcc at codegen;
  we are claiming our passes preserve what a fabric computes, and we
  propose to ask someone else's compiler whether that's true.
- *Hebbian witness debt* — real but narrow; a secondary lane. A debt
  this size cannot carry a round, and paying it first would leave
  the composition hole unknown for another full round.
- *GA-corpus → pass-fitness closed loop* — **rejected as a spine on
  incest grounds**: a closed loop that breeds corpus to flatter
  passes, judged only by the passes' own verdicts, optimizes the
  feedback signal, not the program. It becomes *safe and useful
  exactly when the spine lands* — an external outcome measure is
  what turns "pass fitness" from self-grading into measurement. So
  the loop is sequenced *behind* the differential (§4, R5 hook),
  not beside it as a rival spine.

### 2.2 T2's gate is satisfied — say so in writing

NEXT-PHASE §4 T2 gated M2 on BOTH: (a) T1 leaves a residual class no
property oracle can reach — **satisfied**: phi/control/composition
semantics have no oracle (§1 UNKNOWN; the oracle's own scope note
says arithmetic and comparison tables); (b) R3 ships CFG-graft
inlining — **satisfied**: `passes::cfg_inline` is a registered,
replay-identical library citizen. The gate opens; this plan walks
through it, and this section is the written record that we noticed.

What we build is deliberately *not* full M2 (an in-tree interpreter
with gcc differential was the old framing — building our own
interpreter re-installs the self-referential problem at higher
cost). We build a **lowering + external execution harness**:

1. **Lower** a verify-green fabric to LLVM IR text (`.ll`): cells →
   SSA values (the fabric is already SSA-shaped; phis → LLVM phis,
   regions → basic blocks, ctrl edges → terminators, `Ret` → `ret`,
   calls → the function assembly the inline lane already models).
   The lowering is a *printer variant*, not a semantics — that is
   the whole point. Any semantic the fabric has that resists
   lowering is a finding, not a bug in the harness.
2. **Execute** both legs: `region::interp` (in-tree) and the `.ll`
   via a real toolchain — `lli`, or `clang` + run, whichever the
   host provides (spike determines; both are free).
3. **Compare** answers per fabric: agree / disagree /
   interp-undecidable (budget) / lli-error / **unlowerable** — the
   last two are first-class outcomes, not failures to hide. The
   unlowerable list is the lowering's own shape-audit: it bounds
   what the fabric language can say, the same way SHAPE-AUDIT
   bounded what the generator can emit.
4. **Differential tiers, measured separately:**
   - T-0: raw fabrics, no passes — does the lowering itself preserve
     semantics? (If T-0 diverges, everything above it is noise; this
     is the harness's red/green floor.)
   - T-1: fabric + `PIPELINE_V1`; T-2: fabric + `PIPELINE_V2`.
     The T-1/T-2 agreement rate is the first *external* measurement
     of what R3's passes actually do.
   - T-3: sabotage tier — inject the 7 fold corruptions and 2–3
     composition saboteurs (a pass-order swap, a phi-join mis-strip)
     and measure the differential's kill rate. **The spine's value
     is proven by T-3, not by T-1/T-2 passing.** A differential that
     passes everything and kills nothing is a tautology in toolchain
     clothing, and §5's kill condition handles that case.

### 2.3 Scale and honesty

10k corpus as the default run (matching the fuzz corpus law), with
the GA bred corpus as a second population (140–163 fabrics, richer
control shape, weaker guarantees). Both reported separately; no
pooling. Numbers published as a table: per tier, per population —
agree / disagree / undecidable / lli-error / unlowerable, with the
disagreements itemized (each one is either a fabric bug found, a
lowering bug found, or an interp bug found — all three are wins,
and the doc must say which).

---

## 3. Secondary lanes (priced, none blocking the spine)

| lane | debt | scope | size | gate |
|---|---|---|---|---|
| R4-L2 | replay completeness matrix | publish the per-Edit-kind × per-op replay table (all five passes, corpus law extended to inline + fold); red-condition test per kind | 2–3 d | table published; any hole becomes a priced debt or a fix, named |
| R4-L3 | name collisions | unique-ify `ga_sink`/`ga{N}` names at mint; one test minting colliding sources | hours | collision test red-before/green-after |
| R4-L4 | GA material accounting | (a) decompose the 71–80% verify spread by rejection cause; (b) the phi-free-non-entry v0 variant to regrow dead-region material, *measured before/after* | 2 d | both numbers published; no menu retune without the decomposition in hand |
| R4-L5 | witness + verdict unification | Hebbian witness shape (what the decay answered to) without weakening `verify_deaths`; `PassVerdict::merge` + rewire region-spike bin onto the library verdict | 2–3 d | `verify_deaths` contract unchanged (T6's kill criterion applies); one verdict vocabulary in the tree |

L4 is sequenced deliberately *after* the spine lands wherever
possible: once the differential exists, "regrow dead-region
material" can be judged by whether region-DCE's differential tier
improves — otherwise we're tuning the generator against its own
verdicts again.

---

## 4. Lane structure, models, verification gates

**Lanes and repos.** One repo (quilt-llvm), one worktree per lane
(pre-flight: clean `git status`, own worktree — D9-as-protocol).
Four parallel lanes:

- **S (spine)** — the differential harness: lowering printer,
  executor, comparator, tier runner, sabotage battery. The only
  lane that may add a `bin/` (one: `diff-harness`) and a module
  (`lowering.rs` + tests). **Model: z.ai main (GLM-5.3)** — this is
  high-judgment, doctrine-heavy work where the lane must *not*
  launder disagreements; GLM-5.3 has carried every strategy lane so
  far and the lowering's honesty requirements (unlowerable as
  first-class) are judgment-shaped.
- **L2 (replay matrix)** — mechanical, corpus-law-shaped. **Model:
  z.ai runner tier (GLM-5-turbo)** with GLM-5.3 review at gate; the
  work is enumeration and table-building, exactly runner-shaped.
- **L3+L4 (names + GA accounting)** — small, independent.
  **Model: KimiCode (K3)** — spatial/bookkeeping strength, small
  daily allowance fits two small lanes. KimiCode has no doctrine
  context; its lane briefs must be fully self-contained, and its
  outputs reviewed by the main lane before merge.
- **L5 (witness + verdict unification)** — contract-sensitive
  (touches `verify_deaths` and the verdict vocabulary).
  **Model: z.ai main (GLM-5.3)** — the one secondary lane that can
  break a doctrine if done carelessly.

**Local Liquid (LFM2.5-2.6B)**: not assigned lane work — its
agentic tool-calling at 2.6B is not trustworthy enough for
contract-touching Rust. Offered instead as a *cross-check reader*:
run the finished spine doc past it once and log where it agrees —
a cheap model-disagreement data point, explicitly weighted as
curiosity, not evidence.

**Claude Code (Sonnet 5)**: held in reserve for adversarial review
of the spine's T-3 results — a second model's eyes on the sabotage
kill-rate table before it's cited in EXPERIMENTS.md. One session,
not a lane.

**Verification gates (all lanes):**

- G0 pre-flight: clean tree, own worktree, base hash recorded.
- G1 per-lane: `cargo test` green including the lane's new tests;
  red-condition tests required wherever a lane claims to close a
  gap (lane 1's standard).
- G2 spine-specific: T-0 must be green on ≥ 99% of the first 1k
  corpus *before* T-1/T-2 runs are believed (a diverging floor
  poisons every tier above it); T-3 kill-rate published before any
  T-1/T-2 number is cited as evidence of pass correctness.
- G3 merge: master stays green at every merge; lane records written
  failures-first before merge (the R3 house style — verdicts up
  front, debts named, reproduction commands).

**Merge/adjudication order:** L3 (hours, trivial) → L2 (independent
files, mechanical) → L4 (touches `ga.rs`, after L3's names land to
avoid churn) → L5 (touches `passes/`+`region.rs`, rebase-checked
against the spine if S touched `region.rs` for lowering hooks) →
**S last** — the spine merges only with T-3 published, because a
differential without its kill-rate proof is an unverified oracle,
which is the exact thing this codebase keeps discovering in itself.
Main lane (GLM-5.3) adjudicates each merge against the lane record,
re-running the record's own reproduction commands (D7) — the R1
addendum's method, now standing practice.

---

## 5. Kill condition — registered before work starts

**FIELD-CARRY-R4-SPINE** (obligation-priced, like FIELD-CARRY-0):

> The spine is falsified — and R4 re-plans — if, after the 3-day
> spike (§6), EITHER:
>
> 1. **Faithfulness fails at the floor**: T-0 (raw fabrics, no
>    passes) disagrees or errors on > 1% of the first 1k corpus
>    *for reasons the lowering cannot fix without changing fabric
>    semantics* — i.e., the fabric language and LLVM IR disagree
>    about what a program *is*, not how to print it. (A lowering
>    printer bug is a fix, not a kill; a semantics mismatch — e.g.
>    undecidable interp meets defined lli behavior on the same
>    fabric — is a kill, logged as a spec finding.)
> 2. **The differential is a tautology**: T-3's kill rate on the
>    composition saboteurs is 0 — the external judge agrees with a
>    sabotaged pipeline as often as a clean one. Then the
>    differential adds nothing the property oracle doesn't already
>    give, the M2 debt is *refuted as valuable* rather than paid,
>    and that refutation is itself the round's result, published as
>    such.
>
> Cost of the kill: 3 days spike + up to 1 week of harness work.
> What the kill buys either way: the first external statement about
> what these fabrics compute. Branch survives as a lane record;
> secondary lanes L2–L5 are unaffected (they gate on nothing from
> the spine).

Judgment, labeled as judgment: condition 2 firing is *unlikely*
(the composition saboteurs break things no arithmetic oracle sees,
and an executed program is hard to fool about its own answer), but
condition 1 is genuinely open — the fabric's undecidable-interp
corners meeting LLVM's defined-everything semantics is a real
spec-level collision candidate, and nobody has looked.

---

## 6. The likeliest failure, and the cheapest experiment

**Most likely to fail: the lowering's treatment of undecidable
corners.** `region::interp` has a budget and undecidable outcomes;
LLVM has neither. Every fabric whose interp says "undecidable"
cannot be compared — fine — but fabrics near the budget boundary
will *flip* between legs for reasons that are neither a fabric bug
nor a lowering bug. If that class is large, the differential's
agreement rate becomes a statement about budget tuning, not
semantics, and the headline number launders itself.

**Cheapest experiment (3 days, before any tier runs):** lower the
first 1k verify-green fuzz fabrics, run T-0 only, and publish the
five-way outcome table (§2.2 item 3). Three outcomes:
- ≥ 99% agree with itemized, explainable disagreements → proceed;
  the spike's table becomes the harness's red/green floor (G2).
- Divergences cluster in undecidable-boundary fabrics → tighten the
  comparison to decidable-only, *say so in every headline*, and
  proceed with the narrower claim.
- Semantic mismatch (kill condition 1) → R4 re-plans around the
  spec finding; secondary lanes proceed; the finding is the
  deliverable.

This is the §6 discipline from NEXT-PHASE applied to itself: the
gate that saved R3 from discovering the region-model ambiguity in
week five now guards R4 from discovering the IR-semantics
ambiguity in week five. Same 3% of the round, same shape of
ambiguity, one round later in the stack. That pattern — each round's
spike gate probing the next layer's foundation — is becoming the
house cadence; note it, keep it.

---

## 7. What this plan does not claim

- It does not claim the passes are semantically correct. It claims
  no one external has ever checked, and R4 builds the checker.
- It does not claim the differential will find bugs. It claims the
  T-3 sabotage tier will *price* whether the differential can — and
  that the kill condition fires honestly if it can't.
- It does not claim the 71–80% spread is a problem. It claims it is
  unexplained, and forbids retuning until decomposed.
- It does not schedule M2-full (an in-tree interpreter). If the
  differential refutes itself (kill 2), M2's remaining value
  collapses with it and the ladder amendment (§5 of NEXT-PHASE,
  still Casey's call) records the outcome.
- Week counts in §4 are judgment. The §6 spike is the only hard
  scheduling commitment, same as R3's was.

---

*R4 conception lane, 2026-08-31. The judges have audited themselves
for three rounds; round four buys a judge from outside the family.*

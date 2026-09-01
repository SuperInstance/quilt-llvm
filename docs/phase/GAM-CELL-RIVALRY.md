# GAM-CELL-RIVALRY — cells as native ground for endless improving rivalry

*Strategy lane, 2026-08-31, against `r4-conception` `29fce25`
(master `bb566bb` + R4 conception). This is a CONCEPTION doc: nothing
in it has been run. It answers Casey's 2026-08-31 directive ("think
about the GAM abilities innate to cell design. without shared
context, cells can truly compete and iterate off one another in
endless improving rivalry") and pays the R5 hook that
NEXT-PHASE-R4.md §2.1 registered when it rejected the GA→pass-fitness
closed loop on incest grounds. House rules: measured numbers or it
didn't happen (there are none here — every number in this doc is
either a registered threshold or an estimate labeled as such);
judgment labeled as judgment; kill conditions registered before any
work; undersell in summary, overdeliver in body.*

Worktree: `quilt-llvm-wt-rivalry`, branch `gam-cell-rivalry` (base
`29fce25`). Reproduction: none yet — §7 is the first experiment.
Adversarially reviewed before commit by two lanes (claude Sonnet 5;
opencode lane) — verbatim critiques and dispositions in §9; the
review changed the design in twelve places, largest ones: the
demand economy is anchored to exogenous validated demand (kills
back-scratch collusion structurally), BDI thresholds are priced by
a control-run null rather than asserted, and one shared ring with
lineage-as-ancestry-label replaces the ambiguous two-lineage
framing.

---

## Summary (the undersell)

R4 rejected the GA closed loop as a spine because a loop that breeds
corpus to flatter passes, judged only by the passes' own verdicts,
optimizes the feedback signal, not the program. This doc argues that
**cell design is native ground for a different loop that does not
have that defect by construction**: a population of grid cells, each
hosting a program (a verify-green fabric), competing for *local*
resources — ring bandwidth, tick budget, survival of the slot —
under an information structure in which no population scoreboard
exists at all. Selection IS a fitness function — local, implicit,
and opponent-relative (a red queen, not a rank) — and that is
precisely the class of selection that resists the rank-collapse
incest R4 rejected; the doc's claim is about that class, stated so
in §0 after the opencode lane's correct objection to looser
wording. Selection is demand-decay through the existing M4
machinery (use-count aging → tombstone → slot freed); inheritance is
the existing GA operators plus rejection sampling; the referee is
the R2 conservation battery. Nothing in the loop computes a
population aggregate — the scoreboard is an econometric, computed
offline by the experimenter and never fed back, the way an economy
does not see its own GDP.

On top of that arena sits the **model-arena** (Casey's second
directive): lineages bred by different z.ai models — GLM-5.3,
GLM-5.2, and the old-timers (glm-4.7-flash, GLM-4.6 via opencode) —
each seeing ONLY its own lineage's local behavior and resource
outcomes. The models are stud farms, not referees. Old-timers are
mutation-distribution wideners: models sharing the newest training
priors propose convergent, fashionable mutations; older priors
surface unfashionable-but-viable moves that a judge would score down
and a resource arena lets live.

The formal claim is §1 with its kill conditions registered in §2
before any mechanism. The honest residues are named, not hidden:
the arena's own physics (interp budget, resource model) is a shared
context cells can co-evolve against — the SIM-PHYSICS-GAP debt —
and "endless" is demonstrated as non-collision with halt conditions
over measured horizons, never proven forever. Rivalry's products are
candidates, and R4's external differential (lli/clang) is the
downstream oracle that checks what they actually compute (§5). Rivalry
does not build that oracle; it sequences behind it.

## 0. The debt this answers

NEXT-PHASE-R4.md §2.1, quoting: the GA-corpus → pass-fitness closed
loop "is rejected as a spine on incest grounds: a closed loop that
breeds corpus to flatter passes, judged only by the passes' own
verdicts, optimizes the feedback signal, not the program. It becomes
safe and useful exactly when the spine lands... the loop is sequenced
behind the differential (§4, R5 hook)."

That rejection is correct and this doc accepts it in full. What it
answers is the other half of the sentence nobody has written yet:
the incest defect is not caused by *evolution*, it is caused by a
*shared judge*. Remove the shared judge — not by hiding it, by
making it nonexistent — and the loop stops being able to flatter
anything, because there is no signal to flatter. The quilt cell's
locality is precisely the ingredient that makes the judge's
nonexistence constructible:

- **A classic GA organism sees the scoreboard.** Fitness is a global
  function of the whole population's world; selection is a global
  tournament. Every organism competes on the SAME scale, so
  selection pressure is rank-collapsing: premature convergence is
  the textbook failure (Goldberg & Richardson's fitness-sharing
  work is the classic patch; it patches by re-injecting what the
  arena lacks — locality).
- **A quilt cell cannot see the scoreboard.** Its program observes
  neighbors' actions and its own resource outcomes. The population
  aggregate is not hidden from it; the harness never computes one
  inside the arena's causality. Competition is neighbor-vs-neighbor
  over ring slots, ticks, and budget — *relative, local, and
  dynamic*: when a neighbor's strategy changes, your fitness changes
  with zero change to your program. That red-queen property is the
  anti-convergence force classic GAs must simulate with sharing
  functions; here it is structural.
- **Selection is a fitness function, and the doc says so.** (Folded
  from review — the looser slogan "no fitness anywhere" was false.)
  Demand-decay + richest-replicates IS selection, implicit and local.
  The claim is not that selection is absent; it is that the
  selecting quantity (neighborhood demand for your output, metered
  by budget) is *endogenous, relative, and never aggregated* — there
  is no global scale for the population to collapse onto. The
economy is anchored externally: demand credit exists ONLY for
  satisfying harness-injected requests (§3.2), so the quantity being
  selected on is grounded outside the cells' mutual opinion of each
  other — mutual back-scratching cannot be rewarded because
  neighbor-consumption alone pays nothing (the opencode lane's
  sharpest catch, closed in §3.2/§6).
- **Conservation closes the cheat path.** Rivalry over resources is
  only honest if resources cannot be minted. The R2 battery
  (`conserve::check`, `conserve::population_audit`) plus a per-tick
  budget ledger (§6, priced) make spam unpayable: a cell cannot win
  by growing without limit because growth itself consumes budget and
  unledgered growth fails the audit. Natural selection, not
  optimization: there is no target to converge to.

## 0.5 Prior-art preflight (recalled literature, NOT re-verified this session — search quota exhausted; flag stands until re-run)

Per the workspace preflight rule, the idea was checked against known
art. From recall, labeled as recall:

- **Cellular / diffusion GAs** — Manderick & Spiessens (1989),
  Gorges-Schleuter's ASPARAGOS (1992), Sarma & De Jong's analysis of
  local selection in spatially structured GAs (1996/97). Local
  selection on a lattice demonstrably slows premature convergence.
  This is the closest prior art to §3 and its lesson (neighborhood
  radius controls selection pressure) is adopted wholesale: the ring
  is the radius-1 limit.
- **Digital organisms** — Ray's Tierra (1991), Ofria & Adami's
  Avida: self-replicating programs competing for CPU time and memory
  without a global fitness function. Closest in spirit (resource
  rivalry, no judge) but substrate-flat: no conservation law, no
  audit battery, no mechanical anti-cheat; parasitism there is
  tolerated, here it is *auditable*.
- **Ecosystem models** — Holland's Echo (mid-90s): agents competing
  for resources with no explicit objective. Same spirit, no
  compiler substrate, no lineage protocol over LLM breeders.
- **Open-endedness** — Lehman & Stanley's novelty search (2011),
  Brant & Stanley's Minimal Criterion Coevolution (2017), Wang et
  al.'s POET (2019): the "endless improvement without a target"
  literature. MCC is the sharpest neighbor (coevolution with no
  fitness function, only a minimal criterion). Delta: MCC's minimal
  criterion is still an external test applied globally; here the
  criterion is *demand from neighbors* — endogenous, local, and
  resource-metered — and the anti-cheat is mechanical (conservation
  proofs), not incentive-hoped.

**The honest delta of GAM-CELL-RIVALRY over all four:** none of them
run on an IR with conservation laws, a per-tick audited ledger
(Weft), tombstoned death, and rejection-sampled inheritance as
*existing, tested machinery* (R2/R3, 226-green suite). The novelty
claimed is narrow: the composition — resource-local selection on a
compiler IR whose safety rails are already proven, with LLM
lineages as the mutation layer. Each ingredient exists; the
composition and its measured kill conditions are the work.

## 1. The formal claim

**Claim (measurable form):** Under fixed per-tick resource pressure
and a demand pattern containing ≥ 2 distinct spatial niches, a
population of cells with ring-local information (§0) will
simultaneously (a) *differentiate*, (b) *specialize productively*,
and (c) *keep moving* — sustained over the measured horizon with
zero conservation violations. "Endless" is claimed ONLY as
non-collision with the halt conditions over the measured horizon,
with the mechanism argument (§3: no global signal exists to converge
to) as the reason to expect continuation, never as proof.

Three measured quantities plus one null, all computed OFFLINE by the
experimenter (the arena never sees them — this is load-bearing, §0):

- **The null (priced first, by the control run).** The opencode
  lane's catch, with its arithmetic corrected by measurement this
  lane: two *independent random* allocation vectors on the
  4-simplex have expected normalized L1 ≈ **0.43** (200k-sample
  Monte Carlo, seed 42; the review's back-of-envelope said ~0.35 —
  right direction, wrong constant) — a naive BDI floor of 0.20
  sits well BELOW the noise floor and could never fire, and naive
  differentiation above 0.20 proves nothing (pure drift already
  "achieves" it). Therefore: **the control run (blind-GA, §7)
  publishes the permuted-allocation null distribution FIRST**, and
  every threshold below is re-registered against it before the
  headline run is interpreted — collapse floor = control's 5th
  percentile BDI (the pairwise-distance 5th percentile under pure
  drift measures ≈ 0.15, so the naive 0.20 prior was also too HIGH
  to detect real collapse — both tails were miscalibrated, which is
  the whole argument for pricing floors empirically); freeze floor
  = control's bottom-5% drift band. Smoothing and drift windows are
  fixed before any run: EWMA 50 ticks, drift window 100 ticks,
  freeze window 300 ticks (the review's window-interaction catch:
  pre-registration is the whole answer).
- **BDI — Behavioral Differentiation Index (dispersion).** Cell i's
  behavior in tick t is its budget-allocation vector over the action
  simplex {serve, relay, compute, claim}, EWMA-smoothed over 50
  ticks. BDI(t) = mean over cell pairs of normalized L1 distance
  between smoothed vectors (normalized by simplex diameter 2).
  BDI = 0 is monoculture; the noise null is ≈ 0.43 (above). **The
  BDI claim is therefore two-tailed**: collapse = BDI below the
  control's 5th percentile (cells more alike than blind drift makes
  them — real convergence); differentiation = BDI *with structure*
  (see SPX) — dispersion alone is gameable by one outlier cell and
  measures spread, not division of labor (review catch, folded).
- **SPX — specialization index (division of labor).** Per cell: does
  its dominant action match its own neighborhood's dominant
  injected-demand type? SPX = fraction of live cells matched, plus
  the counter-case (anti-matched cells are data, not failures).
  **Specialized** = SPX above the control's matched-fraction band
  with ≥ 2 distinct dominant actions coexisting. Differentiation
  without SPX is dispersion; SPX without BDI is the imposed demand
  pattern echoing — the claim needs both, which is the review's
  circularity catch made mechanical.
- **η — niche efficiency.** Per cell: *validated* work delivered
  (exogenous requests satisfied, §3.2) per budget consumed, EWMA'd.
  **Specializing productively** = the population efficiency frontier
  (max η over live cells) has positive slope over each trailing
  250-tick window, and no cell reaches the frontier by conservation
  violation (violations halt the run — §6 — so "zero violations" is
  a precondition, not an achievement).
- **Δ — behavioral drift.** Mean |Δ(smoothed behavior vector)| per
  100-tick window. **Still moving** = Δ above the control's freeze
  band at every measurement point after tick 250. (Operationalized:
  frozen = Δ within the control's bottom-5% band for 300 consecutive
  ticks — the review's "vague freeze" catch, closed by
  pre-registration.)

**Improvement = drift toward niche specialization** is the
composite: η rising per cell *within* the niche its behavior is
nearest (each cell's η measured against its own neighborhood's
injected-demand profile), with SPX/BDI holding. A cell hoarding
budget with η→0 is differentiating but not improving; a cell
converging to the population mean is efficient-ish but not
differentiating. The claim needs both at once, per cell, without
the referee ever being asked.

**Lineage Separation (LS)** — the model-arena addendum (§4):
between-lineage mean behavioral distance minus within-lineage mean
distance, same normalization as BDI. LS > 0 = the two breeder
lineages occupy distinguishable behavioral regions. LS is the
number that answers "do different models breed different cell
lineages," measured, not asserted.

## 2. Kill conditions — registered before any work

**RIVALRY-COLLAPSE** (the §1 claim falsified):

> The rivalry mechanism is refuted for this configuration if, in the
> §7 spike (8 cells, 2 lineages, 1000 ticks, binding pool, ≥2-niche
> injected demand), ANY of:
>
> 1. BDI below the control run's 5th-percentile collapse floor at
>    any measurement point after tick 500 with no recovery by the
>    next point (monoculture collapse — BELOW blind-drift noise,
>    not merely "low"), OR
> 2. one lineage occupies ≥ 7/8 slots for ≥ 200 consecutive ticks
>    (lineage capture — note this can happen with BDI healthy if the
>    winner's own cells stay behaviorally distinct; capture is still
>    a kill because it ends *between-lineage* rivalry), OR
> 3. behavioral drift Δ sits inside the control's freeze band for
>    ≥ 300 consecutive ticks while differentiation holds
>    (diverse-but-frozen: the "endless" half dead — differentiation
>    without improvement), OR
> 4. effective live population < 6/8 for ≥ 200 consecutive ticks
>    (cascade collapse: coupled starvation or persistent child
>    rejection thinning the ring below a population — the review's
>    missing kill; replication cannot sustain what death drains).
>
> Cost of the kill: the spike (§7), ≈ 1 week. What it buys: the
> first measured statement of whether locality alone sustains
> rivalry in this substrate, either way. If it fires, the mechanism
> re-plans — the likeliest suspects, in order: ring radius too small
> (radius-1 = maximal inbreeding pressure; the cGA literature's own
> warning), decay threshold too slow/fast (death timing on a
> 2-consumer ring is high-variance — coupled non-readers die
> together, per review), pool not binding or over-binding. Each
> suspect is a one-knob re-run, priced at spike cost; two
> consecutive kills on different knobs = the claim is wrong, not
> the knob, and the doc says so.

**ARENA-NULL** (the model layer refuted as valuable — asymmetric by
scale, folded from the underpowered-review catch):

> At SPIKE scale (1 seed, ~10–20 model events per lineage): the
> spike can only fire this condition on STRONG signal — blind-GA
> control matching or beating model-bred arms on BDI/SPX/LS with
> comparable rejection rates. Absence of a detectable effect at
> n=1 seed is recorded as *inconclusive*, NOT null — the review is
> right that ~10 events per breeder is a case study, not a test.
> Null ACCEPTANCE requires the priced follow-on: ≥ 5 seeds × both
> arms, blind-vs-bred split published per seed, and the
> pre-registered band. If the follow-on shows parity, the
> model-arena is empty at this scale and THAT REFUTATION IS THE
> RESULT, published as such. (It does not kill the mechanism; it
> kills the second directive's premise at this scale. Re-test at
> larger population is a priced follow-on, not a default.)

**HALT (not a kill — an engine bug):** any `conserve::check`,
`conserve::population_audit`, Weft-chain, or budget-ledger failure.
The referee is physics; when physics fails the run stops and the
failure is a bug hunt. Cells never get credit for breaking the
arena, and the arena never gets to launder a violation as evolution.

## 3. The mechanism

### 3.1 The arena physics (what exists, what is built)

| layer | status | what it is |
|---|---|---|
| organism slot | build (small) | a grid cell on a ring (radius 1, wrap), hosting one program + a budget account |
| genome | **exists** | a verify-green `Fabric` (quilt-llvm `fabric.rs`); the program IS a fabric |
| execution | **exists** | `region::interp` under a per-tick step budget = the tick's compute slice; budget exhaustion is an undecidable outcome, first-class (as R4 §2.2 established) |
| observation | build (small) | the ONLY inputs a program may see: neighbors' published action counters (last k ticks) + own budget/ledger state. Enforced by the harness — programs take exactly these inputs, bound as entry-region `Param`s via the existing `interp_call` param-binding mechanism (region.rs; verified in-tree this lane) |
| actions | build (small) | per tick each cell allocates its budget over {serve reads, relay traffic, compute (self), claim (reserve)} — a 4-simplex choice emitted by the program. Claims are PER-TICK, use-it-or-lose-it: no banking, no accumulation (closes the kleptocracy path the review named — reserves deny neighbors budget this tick but cannot compound) |
| demand | build (small, folded from review) | **exogenous by construction**: the harness injects request streams at cells (the demand pattern = the experimental treatment); demand credit exists ONLY for satisfying an injected request (serving it directly or relaying to a cell that does). Neighbor-to-neighbor consumption alone pays ZERO credit and prevents ZERO decay — mutual back-scratching is structurally unpaid |
| decay | **exists** | use-count aging (`decay.rs`) re-based on validated demand: no validated service for D ticks = dead; tombstoned FORGET with witness, `verify_deaths` contract unchanged |
| selection | build (thin) | a slot freed by death is contested by neighbors: richest-by-η neighbor replicates into it |
| inheritance | **exists** | `ga.rs` operators (`mut_grow`, `mut_add_phi`, `mut_consume_phi`, `mut_add_call`, `mut_boundary_const`, `mut_operand_shuffle`) + `crossover` (region graft); rejection sampling (verify-green or die, rejections counted — R3 lane-2's guard pattern) |
| referee | **exists** | R2 battery per tick: `verify` + `conserve::check` + `conserve::population_audit` + Weft chain continuity |
| budget ledger | build (small) | pool mints exactly the fixed per-tick budget, sized so the pool BINDS by construction (total injected request value per tick strictly exceeds total servable capacity — scarcity is not a hope, it is an invariant of pool size); every claim is a ledgered per-tick row; a `population_audit`-twin audits rows (§6). Hours-days, priced |
| breeder interface | build (§4) | the model-arena contract |

Everything marked *exists* is R2/R3-shipped and 226-green. The build
list is deliberately thin: the arena is a HOST around proven
machinery, not a new engine. Spike scope (§7) is quilt-llvm only
(new `bin/rivalry-spike`); quilt-rust grid integration (quilt-wire
transport, field-edge ledgers) is a priced follow-on lane, not
spike scope — the spike's ring is in-process.

### 3.2 One tick, mechanically

1. Pool mints the fixed per-tick budget (binding by construction,
   §3.1); per-cell shares ledgered, per-tick rows only.
2. The harness injects requests at cells (the demand pattern — the
   treatment, imposed; see §8's non-claim on emergence).
3. Each cell's program runs under interp within its slice, on
   (neighbor counters, own ledger state, visible requests) → an
   allocation of the slice over the action simplex.
4. Actions settle: requests served (validated credit to the server,
   split server/relays when relayed, subject to ring bandwidth —
   fixed bytes/tick/direction, rivalrous by construction); claims
   reserve per-tick and expire at tick end; unconsumed slices
   return to the pool, ledgered.
5. Validated demand is recorded: whose service satisfied which
   injected request. Unvalidated consumption (a neighbor reading a
   neighbor) is observable behavior but pays nothing and stays no
   death.
6. Decay classification runs (M4, re-based on validated demand):
   dead cells are tombstoned; freed slots enter contention.
7. Replication (on freed slots only): the freed slot's richest
   neighbor by trailing η is the parent; with a live second
   neighbor, `crossover` (region graft) — **crossover crosses
   lineages freely**: one shared ring, lineage is an ancestry LABEL
   (§4.1), not a fence, and genetic competition mediated by the
   arena IS the rivalry; then ONE mutation — menu draw, breeder
   proposal (§4), or both with stated probability — through
   rejection sampling; the child enters the slot only verify-green
   (rejections counted, per source). A rejected child leaves the
   slot empty this tick — persistent rejection thins the ring and
   kill condition 4 reads it.
8. The R2 battery + budget audit run over the whole tick's diff.
   Any failure: HALT (§2). The tick's Weft entry lands, hash-chained.

No step computes, references, or stores a population aggregate.
BDI/η/Δ/LS exist only in the offline measurement pass over recorded
Wefts — the economy does not see its own GDP.

### 3.3 What selection IS, precisely

Selection is the conjunction of three local pressures, none of which
is a population score:

- **Starvation**: no VALIDATED demand for D ticks → tombstone (M4
  machinery, re-based per §3.2 step 5).
- **Slot pressure**: your death is your neighbor's reproduction
  opportunity — improvement propagates only through freed slots,
  i.e., only through *another cell's failure to matter*. Rivalry is
  literal: your lineage grows iff a neighbor lineage starves.
- **Bandwidth pressure**: relay is rivalrous; a strategy that floods
  the ring degrades its own neighbors' service — and validated
  demand is the only currency, so flooding is self-starving without
  any anti-spam rule. Conservation is the backstop that makes even
  clever flooding unpayable at the ledger level.

**Selection has two gates and only one of them sees behavior**
(folded from review): the verify gate (rejection sampling) is
behavior-blind — it selects for edit-robustness, and a lineage
under persistent high rejection ratchets toward verify-passing
no-op structure rather than better behavior. This is why per-source
rejection rates are a first-class published number (§7) and why
rejection-rate PARITY is a precondition for any breeder-vs-breeder
comparison (§4.2): a stud whose children die at the gate never gets
to run the race. It is also why exploration pressure lives in the
mutation supply (menu + breeders + the demand phase flip, §7), not
in selection — selection here is exploit-only (richest neighbor
wins the slot), and the review's sharpest tension says exactly
that: selection for local fit is not the same force as
differentiation, and the two can pull apart — an exploit-only
selector on an N=8 ring can lock in. The design's answer is that
drift is supposed to come from mutation and changed demand, not
from selection indulging weak variants — and if that answer is
wrong, kill conditions 1/3/4 are where it shows. Named, not hidden.

There is no generation boundary, no epoch, no termination predicate.
"Generation" is an econometric of the Weft (mean slot turnover),
not a mechanism. Improvement is behavioral drift under this
pressure — measured, not targeted.

### 3.4 The incest answer, restated mechanically

R4's rejection named the failure: judge shares a codebase with the
accused → the loop optimizes the signal. Here the judge's
nonexistence is enforced in three places: (1) the harness passes
programs only local inputs; (2) selection reads only demand and
budget — quantities that exist in neighbor ledgers, not in any
aggregate; (3) the measurement layer is causally disconnected
(offline over sealed Wefts). A cell cannot flatter the scoreboard
because nothing in its causal universe *is* the scoreboard. What
remains shared is the physics itself — interp's budget semantics
and the resource model — the named residue below.

**SIM-PHYSICS-GAP (priced debt, named now):** cells co-evolving
against arena physics will find interp-budget quirks and exploit
them; "efficiency" can drift toward "simulator exploitation."
Mitigations: keep the physics minimal and FROZEN during a run
(fewer quirks, and drift is attributable); price a per-run quirk
review; and — the reason this debt does not kill the concept — R4's
external differential audits products outside the loop's causality
(§5). The gap is real, named, and bounded by an oracle that does
not share the arena's code path.

## 4. The model-arena — stud farms, not referees

### 4.1 The lineage protocol

- **One ring, lineage as label** (the review's structural catch,
  decided): both lineages share the 8-cell ring; a lineage is an
  ANCESTRY LABEL that determines breeder access, not a fence.
  Crossover crosses lineages freely (§3.2 step 7) — a lineage's
  genes spread by winning slots, which is the rivalry working as
  designed, and lineage occupancy over time is the measured
  between-lineage scoreboard (offline, like every other metric).
  What NEVER crosses lineages: breeder context. Breeder A is never
  shown breeder B's proposals, genomes, or metrics; the only
  information that crosses is whatever the arena itself transmits —
  a descendant carrying B-grafted material in its fabric. That is
  genetic leakage through reproduction, nature's own channel,
  permitted BY DESIGN and symmetrical; the no-shared-context
  principle bars global aggregates and cross-lineage ARTIFACTS, not
  the existence of competitors (the review's "indirect leakage"
  point is accepted and classified: outcome-mediated inference is
  the rivalry channel itself, not a violation of it).
- **Assignment:** each lineage is bound to one breeder model for the
  run. Spike (§7): lineage A = GLM-5.3 (z.ai main tier), lineage B =
  glm-4.7-flash (runner tier). The full arena adds GLM-5.2 and the
  old-timer lane GLM-4.6 via `opencode run --auto`.
- **When a breeder acts:** ONLY at replication events in its own
  lineage (§3.2 step 7). Between events the breeder is silent —
  models never supervise ticks, never rate cells, never see the
  arena live.
- **What a breeder sees (the contract, enforced by the harness):**
  its lineage's descendant programs (fabric text), per-cell trailing
  local outcomes (actions, budget consumed/delivered, validated
  demand hits, decay class), the freed slot's neighborhood demand
  profile, the mutation menu and verify constraints (fixed
  boilerplate). NOTHING ELSE — in particular no cross-lineage data,
  which the harness enforces by construction: the input does not
  contain it. This is §0's no-shared-context principle applied one
  level up: the breeders are as blind to each other as the cells
  are.
- **What a breeder returns:** ONE mutation — a menu edit (which
  operator, where) or a mutation-menu weight change for that child.
  Machine-checked, applied through the same rejection gate as blind
  mutations. Rejections counted per breeder and published: a stud
  whose every proposal dies at verify pays for its lineage's decay
  like any other bad parent. No model output is ever executed
  outside the fabric's own semantics.
- **Blind share:** a fixed fraction of replications in every lineage
  use menu draws only (spike: 50%, labeled judgment, first-run
  default; one re-run at 30/70 priced if the split is load-bearing).
  Blind draws always use the FROZEN default menu weights; a
  breeder's menu-weight edits apply only to its own bred children
  (the review's control-contamination catch, closed). This is the
  control *inside* each lineage — if breeder proposals never beat
  their own blind share's outcomes, ARENA-NULL has its per-lineage
  evidence.
- **The treatment confound, named** (review catch): the treatment is
  model + harness — a breeder that cannot emit protocol-valid
  proposals degrades to its blind arm for FORMAT reasons, not
  prior reasons, and that difference alone can produce the arena's
  outcome. Therefore rejection-rate parity is a PRECONDITION for
  interpreting any breeder-vs-breeder comparison (§4.2), and a
  breeder with persistent high rejection is reported as
  "protocol-limited", its entropy/survival numbers quarantined from
  the old-timer hypothesis test.
- **Cost bound:** replications per run are bounded by deaths ≤ slots
  × (ticks / D). Spike numbers: 8 × (1000/200) = 40 events upper
  bound, ~20 expected — ≤ 40 model calls per lineage per run, small
  prompts, z.ai Max pricing. The arena is cheap by construction:
  scarcity bounds API spend the same way it bounds cells.

### 4.2 What the old-timers are FOR

Judgment, labeled as judgment, then measured: the hypothesis
(Casey's) is that older models lack the newest training priors, so
their proposals are more organic — wider entropy over strategy
space, immune to current fashion. The mechanism reason this matters
HERE and not in a judged arena: a shared-prior consensus (every
current model "knows" what good code looks like) acts as an
implicit global judge even when no explicit one exists — five
breeders with identical priors are five instances of one stud. An
old-timer's unfashionable proposal — a strategy a current model
would never surface because its prior scores it down — survives in
a resource arena iff it serves demand. The judge that would have
killed it does not exist (§0); the ledger that would have caught it
cheating does (§6).

So old-timers are **mutation-distribution wideners**: they raise the
search distribution's entropy exactly where newer models have
collapsed it. Measurable predictions, all published per breeder:
(1) proposal diversity (distinct operators/edit sites per breeder,
  entropy over menu choices), (2) rejection rates (unfashionable ≠
  broken: the prediction is higher entropy at SIMILAR rejection
  rate, not more verify failures — and rejection-rate parity is the
  precondition, §4.1), (3) niche occupancy distance —
  does the 4.7-flash/4.6 lineage occupy behavioral regions the 5.3
  lineage never visits (LS decomposition per lineage pair), and
  (4) survival: old-timer-lineage cells' η vs new-model-lineage
  cells' η. If (1) holds but (3)/(4) are null, the widener widens
  into regions the arena doesn't reward — also a result, priced as
  such. None of this is assumed; all of it is in the spike's table.

## 5. Connection to R4 — the downstream oracle

R4's spine (lower fabrics to LLVM IR, execute with `lli`/`clang`,
compare against `region::interp` across tiers) is the ONLY execution
oracle this project will have that does not share a codebase with
the accused. Rivalry consumes it and never duplicates it:

- **Rivalry outputs are candidates.** Frontier fabrics (η-leading,
  specialized, weird) are exported into the R4 differential corpus
  as a third population beside fuzz and GA-bred, reported
  separately, no pooling (R4 §2.3's law).
- **The differential does not drive selection.** Feeding lli
  outcomes back into the arena would re-install a global judge and
  destroy the locality the whole design rests on. The oracle audits
  products post hoc; it never touches the loop's causality. **And
  the review's price of that is accepted in full, stated plainly:**
  because the oracle is post hoc, no kill condition consumes it, and
  a physics-exploit "success" could be banked before anyone looks.
  Therefore every claim this doc's spike can make is prefixed
  **"behavioral, within arena physics"** — and the differential
  audit of frontier candidates (lowering them, executing them,
  publishing the quirk audit) is a REQUIRED follow-on lane before
  any "improvement" claim is allowed to leave this document. The
  mitigation is not proof; it is a bounded, sequenced debt with a
  named owner (the R4 spine).
- **Sequencing:** rivalry is safe to RUN before the spine lands
  (nothing in §3 needs lli), but rivalry's "improvement" claims are
  only HONEST after it lands — without the external differential,
  η-improvement is uninterpretable against SIM-PHYSICS-GAP (§3.4).
  This is exactly R4 §2.1's sequencing argument, restated for this
  loop: the loop runs, but its products await a judge from outside
  the family before anyone believes them.

## 6. Safety rails

Hard invariants, every tick, zero tolerance (failure = HALT, §2):
`verify` on every live genome; `conserve::check` +
`conserve::population_audit` on every diff (the R2 battery — no
minting, no vanishing, lifecycle-coupled edits); Weft chain
continuity (`verify_chain`); `verify_deaths` contract untouched
(T6's kill criterion, inherited); tombstone for every death; budget
ledger audited by the population-audit twin (rows conserved: pool
mints, claims ledger, per-tick expiry — nothing accumulates,
nothing else); **DEMAND-VALIDATION** — replication credit and decay
prevention exist only for harness-injected requests satisfied
(§3.2), which makes the back-scratch equilibrium structurally
unpaid rather than merely unfashionable (the review's missed-kill,
closed); **live-population floor** — effective population < 6/8 for
200+ ticks is kill condition 4, so rejection-thinning cannot
silently hollow the ring. Monoculture detection is the
measurement layer's job, not the arena's: BDI floor, lineage-share
ceiling, and niche-cluster count (behavior vectors clustered; one
cluster holding ≥ 7/8 cells for 200+ ticks = monoculture
regardless of lineage labels). No anti-stagnation engineering: if
rivalry freezes it must freeze honestly and the kill condition
reads it — propping up drift with forced mutation would launder
the metric.

## 7. The spike — smallest experiment (this week)

**Setup.** 8-cell shared ring (radius 1, wrap), 2 lineages × 4
cells (GLM-5.3-bred vs glm-4.7-flash-bred), 1000 ticks, BINDING
pool (§3.1), D=200 decay threshold, ≥ 2 spatially-varied injected
demand niches (e.g., cells 0–3 serve-heavy, 4–7 relay-heavy, with a
phase flip at tick 500 to test re-specialization under changed
demand — the imposed-treatment honest framing is §8's first
non-claim). Separate control run, run FIRST: 8 cells, same demand,
both "lineages" blind-GA (frozen default menu weights only) — this
prices the BDI null distribution (permuted allocations), the
collapse floor (5th percentile), and the freeze band (bottom-5%
drift), which §1/§2 re-register against before the headline run is
interpreted; it is also ARENA-NULL's comparator. New
`bin/rivalry-spike` in quilt-llvm (reuses `ga.rs` operators,
`decay.rs` classification, the R2 battery, `region::interp`); no
quilt-rust integration in scope.

**Measurements, published as one table at ticks 250/500/750/1000:**
BDI, SPX, LS, per-cell η slopes, drift Δ, live-population count,
per-breeder proposal entropy / rejection rate / survival (with the
protocol-limited quarantine rule, §4.1), deaths and replications
count, blind vs bred outcome split. All numbers from the offline
pass over sealed Wefts. Kill conditions RIVALRY-COLLAPSE and
ARENA-NULL are live from tick 1 (§2); HALT on any rail failure.
Windows pre-registered: EWMA 50, drift 100, freeze 300 (§1).

**Exit.** Any of: kill fires (published as the result); table shows
differentiation with structure (BDI above collapse floor + SPX
above the control's matched band) + frontier η rising + Δ above
freeze band (the claim holds at 1000 ticks — undersold exactly this
far: "held at the measured horizon, behavioral, within arena
physics"); or the fuzzy middle (some conditions hold) → the table
IS the result and the knob suspects (§2) get one re-run each,
priced. **Scale honesty (folded from review): one seed, two
breeders, ~10–20 model events per lineage — the spike is a
directional case study, not a powered test; the ≥5-seed follow-on
(§2 ARENA-NULL) is where conclusions live.**

**Estimates (labeled):** ~20 replications/run (≤ 40 hard bound) →
≤ 80 model calls total across both runs; wall-time dominated by
interp, sub-minutes per 100 ticks based on ga-corpus's 0.24 s /
10k-gen-eval release timings; the build list of §3.1 is 2–4 days.
The whole spike is priced at ≈ 1 week including the doc.

## 8. What this plan does not claim

- It does not claim niches EMERGE. The demand pattern is imposed —
  the wind tunnel is the experiment; what is measured is the
  population's differentiated, productive, moving RESPONSE to
  structured scarcity, not the origin of structure (review catch,
  conceded).
- It does not claim endlessness. It claims a mechanism with no
  structural convergence target, a measured non-collapse horizon,
  and registered kill conditions that fire honestly.
- It does not claim older models ARE more organic. It claims the
  hypothesis is now measurable (four published predictions, §4.2)
  and that the arena is the fair test a judge never was — and the
  spike alone cannot settle it (one seed, case-study scale, §7).
- It does not claim model-bred beats blind mutation. The blind
  share and control run exist precisely because ARENA-NULL is an
  acceptable result.
- It does not claim rivalry validates programs, and it does not
  bank physics-exploit successes: every spike claim is prefixed
  "behavioral, within arena physics" until the R4 differential
  audits the frontier candidates (§5). The R4 differential
  validates; rivalry only generates candidates worth validating.
- It does not schedule quilt-rust integration, multi-ring topologies,
  radius > 1, or the full four-breeder arena. Those are priced
  follow-ons, gated on the spike's table.

## 9. Adversarial review log

Two adversarial reviews attacked this design before commit; both
are archived verbatim (`gam-rivalry-critique-claude.md`,
`gam-rivalry-critique-opencode.md`) and folded the same day.
Disposition:

- **Folded (design changed):** demand economy re-anchored to
  EXOGENOUS validated demand — closes mutual back-scratching
  (opencode d; claude 2); one shared ring, lineage = ancestry label
  (opencode c); binding pool + per-tick claims, no banking —
  closes kleptocracy (opencode b); BDI null calibration via control
  run, 0.20 demoted to prior guess (both); SPX added because
  dispersion ≠ division of labor (opencode a); freeze
  operationalized, windows pre-registered (claude 7, opencode d);
  kill condition 4 (population shrink / cascade) added (opencode
  d); rejection-rate parity as precondition + protocol-limited
  quarantine (opencode c); frozen default weights for blind draws
  (opencode c); power honesty — spike directional, null acceptance
  needs ≥5 seeds (claude 6, opencode c); two-gate selection and the
  exploit-only tension named (claude 1, opencode b); post-hoc
  oracle concession — "behavioral, within arena physics" prefix,
  differential audit REQUIRED before claims escape (opencode d).
- **Answered, design unchanged (with reasons):** "indirect leakage"
  through slot transitions (claude 5) — classified as the rivalry
  channel itself: outcome-mediated inference is what nature's
  organisms have; barring it would require hiding the arena from
  its own participants, which is neither possible nor desirable.
  "Redefine selection as niche-fill" (claude's #1 suggestion) —
  rejected: niche-fill is still a global judgment of what a niche
  is; demand-decay keeps the criterion endogenous. "Wire the oracle
  into the loop" (opencode d) — rejected: re-installs a global
  judge; the concession paragraph in §5 is the accepted price.
- **Note on attribution:** the opencode lane's session log header
  read `build · glm-5.3` rather than GLM-4.6; the review is archived
  as the opencode-lane voice without a confident sub-model
  attribution. The content stands on its own.

*R5 conception lane, 2026-08-31. R4 bought a judge from outside the
family; R5 removes the judge from the family album entirely — and
lets the ledger, which cannot be flattered, keep score of nothing
but itself.*

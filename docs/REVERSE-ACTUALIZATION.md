# REVERSE-ACTUALIZATION — walking the far future backward to v0

*Status: **design-intent essay, methodology partially degraded** (see §6
Failures Booked — every external model lane was down this session; the
iteration rounds below were run by a single model under named critic
disciplines, and are labeled as such). What is fleet fact is cited by
file; everything forward-looking is conjecture, labeled. This page
answers Casey 17513 (deep thought on the verilog-grounded LLVM
abstraction, far-future reverse-actualization with model-iterated
rounds) extended by Casey 17515 (fold in batten-spline).*

Method in one line: start at the end-state artifact years out, written
present-tense; walk backward through five rounds (5yr → 3yr → 18mo →
6mo → v0), each round critiqued by an iterator before the next step
down; at the bottom, name the invariants that survived every round
(freeze them now) and the free variables that never settled (hold them
loose).

---

## 1. Ground: what the RTL fabric actually taught

Everything below is measured fleet fact from quilt-verilog, cited by
file. This is the only proven substrate in the house; every later
section builds on it and labels its conjecture.

1. **Conservation survives the worst states.** The ledger —
   cum(injected + emitted) − cum(accepted + drained) == pipes occupied
   — held at **0 violations across 2,852,899 cycles, including the
   wedged F3 deadlock states** (SILICON-EXPERIMENTS.md §2). The fabric
   deadlocked; the accounting did not lie. This is the single
   strongest fact we own, and it is why D4 is a law here and not an
   aspiration.
2. **Conservation is not liveness.** F3's saturation deadlock froze
   the ring with the ledger perfectly intact (SILICON-EXPERIMENTS.md
   §3): 11 flits stuck, occ frozen, accounting balanced. A compiler
   translation: a pass pipeline can be *conservation-clean and still
   not terminate / not make progress*. The compiler law we promote
   must therefore be a pair — **conservation (D4) plus a progress
   check** (each tick either advances the fabric or declares itself a
   fixed point; a pass that neither transforms nor terminates is the
   compiler's F3). This pairing was missing from THEORY.md §6 and is
   this lane's first correction to it.
3. **Proof cones have holes, and the holes are where the bugs live.**
   F2 (ringport flit cloning) lived in *the one fabric module formal
   never covered* — `formal/fabric.conservation*` proves a 2-cell
   `q_flit_pipe` path and never instantiates `q_link_ringport`
   (SILICON-EXPERIMENTS.md §3.1, "Formal coverage gap named"). The
   six SymbiYosys proofs PASS (VERIFICATION.md, lane 3) while the
   fabric manufactures phantom flits one module over. Lesson, stated
   hard: **a green verifier over here is not a statement about over
   there.** Verified regions and unverified regions must be
   *distinguishable in the artifact itself*, or the green launders
   itself across the boundary. This is exactly the hole batten-spline
   fills (§3).
4. **Formal culture pays before users pay.** The proofs forced two
   real RTL defects out before any consumer saw them (VERIFICATION.md,
   lane 3 pointer to formal/README.md); scale sim found three more
   (F1/F2/F3, all fixed and re-measured). The culture is: prove the
   small core, simulate the whole, record both honestly.
5. **State as one loadable file is a real product shape.** QUF — the
   GGUF of cellular silicon, one binary that loads into testbench,
   soft core, or FPGA identically (QUF-SPEC.md §0) — proves that
   "the entire state of the system is a portable, versioned artifact"
   is buildable and useful. The compiler analogue (a compilation is a
   file you can replay and query) has a fleet precedent, not just an
   analogy.
6. **Honesty has a measured cost and it was always paid.** The
   referee's adjudications are preserved verbatim-in-substance even
   where they were wrong (SILICON-EXPERIMENTS.md §0); pre-fix numbers
   are kept as wedged-state evidence next to post-fix numbers
   (§2/§3.2); INCIDENTS.md exists at all. The docs are slower than the
   code and that is the point.

Distilled: **cells/wires/ticks work in silicon; conservation holds
under stress; liveness needs its own law; verification coverage is
patchy by nature and must be mapped, not averaged; and the whole-state
artifact is shippable.** That is the entire evidence base. Everything
after this line is conjecture built on it.

---

## 2. The batten epistemology (Casey 17515)

batten-spline (repo: /home/eileen/projects/batten-spline) answers a
routing question — cheap local model or expensive cloud one? — by
treating **verified outcomes as battens** (anchor posts) in an
embedding-space map. Between battens, capability is unknown —
**fog-of-war** — so confidence is interpolated from nearby anchors:
Nadaraya–Watson over an RBF distance kernel times an exponential age
decay (half-life τ), with fog density = distance to the nearest
batten. Routing thresholds turn the confidence estimate into
LOCAL / CASCADE / CLOUD.

The transfer to the compiler is not the math (we import no kernels);
it is the **epistemology: verified points, declared fog, interpolated
confidence, decaying trust.** Three mappings, all design-intent:

**(a) Pass routing as cascade.** A pass in this house is a pure
function `fabric → (fabric, diff)` (GLOSSARY). Far-future, each pass
exists in a *tiered cascade over the same fabric*: a cheap tier that
reads only a signature/delta, an intermediate tier that reads the
diff + ledger, and a full tier that walks the fabric. Verified
outcomes — "pass P at tier t preserved property X on fabrics shaped
like S" — are battens in a *fabric-signature space*. A new fabric
routes by interpolated confidence: near battens, the cheap tier is
trusted; in fog, the pass escalates to the full walk. The cascade
thresholds are exactly LOCAL/CASCADE/CLOUD wearing compiler clothes.
Conjecture, labeled: no signature space exists yet; the experiments
lane would have to mint one and measure whether confidence actually
predicts pass correctness.

**(b) Verifier fog-of-war.** This is the load-bearing mapping, because
it answers lesson 3 above. Formally verified regions of the compiler
(and of any artifact the compiler produces) are **battens** —
verified-property markers with a timestamp. The fog between them is
**declared, not pretended**: F3's ringport sat outside every proof
cone, and the artifact did not say so. A batten-routed verifier makes
the coverage map part of the artifact: fog density at a cell = distance
to the nearest verified property; anything in dense fog routes to
heavyweight verification (proof or exhaustive sim), anything on a
batten routes to a re-run check. The **age half-life** maps to churn:
a batten staked on fabric-shape S decays as the pass pipeline evolves
underneath it, so stale verification stops laundering green across
refactors — the exact failure mode of "it was green when I wrote the
branch" (D7's enemy).

**(c) Anchors in provenance space.** batten-spline anchors in
embedding space; we anchor in **provenance space** — a batten is a
tuple (fabric-neighborhood, verified property, tick). Because history
is append-only and replayable (N4/D5), a batten anchored at tick k can
be *re-established at tick k′ > k by replay* rather than re-verification
— the anchor is cheap to maintain precisely because the fabric never
rewrites. This is the cleanest symbiosis in the whole document: **N4
makes battens durable; battens make N4's verification affordable.**
Without battens, append-only history means re-proving everything after
every tick; with them, you re-prove only what left the neighborhood of
its anchors.

---

## 3. The far future, present tense (end-state, ~5yr out)

*Written as if real. It is not. This is the artifact the backward walk
starts from — the reverse-actualization target.*

A user compiles a program the way a pilot files a flight plan. The
compilation is a **quilt file**: initial fabric, every diff, every
ledger line, every batten, one versioned artifact (QUF's
grandchild). Nothing about the compilation is a memory in a dead
process; it is all in the file.

Every optimization is **replayable history**. `quilt replay 37`
reconstructs the fabric at tick 37 bit-for-bit, and `quilt bisect
--predicate "retire not accounted"` walks the ticks the way quilt-
verilog's bench walked cycles. Miscompile triage is a query session,
not a debugger session: *which diff first detached this wire; which
ledger entries cite it; what did the pass read when it decided.*

**Conservation is provable per tick, and progress is checked per
tick.** The ledger reconciles at every boundary (D4) and every pass
either transforms or declares fixed-point — the F3 pairing. A pass
that wedges is caught by the same machinery that catches a vanished
value.

**Provenance is a query language.** Backward slices, "why is this
constant 42," "which passes touched anything derived from line 12 of
the source" — all walks over wires and diffs, answered in
milliseconds from the quilt file. Summaries carry their fibers
declared (THESIS-V3 discipline): every cached signature documents
which distinctions it cannot make.

**The pipeline itself is batten-routed.** Passes run in a cascade
tiered by cost; the router's confidence comes from battens in
fabric-signature space; verification effort routes on fog density.
Coverage is a first-class map you can look at — green archipelago,
declared gray sea — and CI refuses to *average* it. When a refactor
moves a module out from under its battens, the age-decay makes the
coverage map darken until someone re-verifies.

**Curricula compile.** A tuning run, an autotuning search, a
"learned" pass ordering — each is itself expressed as a fabric whose
cells are pass invocations and whose wires are fabric-signature
flows. The compiler compiles its own optimization strategy, and that
compilation is itself a quilt file with its own ledger. Recursion
stops being scary because each level is inspectable under the same
laws.

That is the end-state. Now we walk backward, five rounds, each one
critiqued before stepping down.

---

## 4. The rounds (backward, critiqued)

*Iteration honesty (D8): the task specified different DeepInfra models
as per-round iterators. Every external lane was down this session
(§6). The rounds below were critiqued by the sole available model —
GLM-5.3, this agent — **wearing four named critic disciplines**
(deep-reasoning, logic, creative, ambition-check) and one synthesis
pass between rounds. Same model, different hats: independence NOT
achieved, labeled. The disciplines are kept distinct and each round
records what its critique killed, so a future session with live lanes
can re-run the critiques as real model calls and diff the verdicts.*

### Round 5 → the 3yr artifact (~3yr out)

What must exist three years before the end-state: a compiler with
**persistent quilt files** as the interchange format (parse → history
→ replay → query works end to end on real programs, say a language's
test suite); **conservation + progress checks in every pass**; the
**provenance query language v1** (walks, slices, "why" queries); pass
cascades exist but routing is heuristic, not batten-based yet;
verification coverage map exists as a *report*, not yet a router
input.

*Deep-reasoning critique* killed two things. First, "curricula that
compile" cannot be a 3yr deliverable — a meta-IR over passes needs the
object-IR stable first; moved out to end-state only. Second, the
critique forced the **storage question up the stack**: a quilt file
for a real program's full optimization history is potentially huge
(quilt-verilog committed whole traces at 15 cells; a compiler fabric
is bigger). At 3yr there must already be diff compaction with
*replay-preserving* guarantees — compaction that cannot answer
`replay 37` is not compaction, it is amputation. Booked as a first-
class engineering surface, not a footnote (THEORY §5 said "real
problem"; this round prices it as *the* problem).

### Round 4 → the 18mo artifact

What must exist: **cell IR + verifier + 3–5 real passes** (fold, DCE,
simplify, a real GVN-ish pass) with red/green suites per D1; **ledger
reconciliation in the verifier** (D4 live, not documented);
**replay-from-mid-history in every test** (D5 live); a **coverage
report that names its fog** — which pass properties are tested,
which are proven, which are neither, in one honest page (the batten
*map* before the batten *router*); provenance walks working over
append-only diffs, even if the query language is just a CLI verb.

*Logic critique* killed "batten routing at 18mo": interpolation needs
battens, battens need verified outcomes, verified outcomes need many
runs of stable passes — you cannot route on a map with three anchors.
Forced the sequencing: **map first (18mo), router later (3yr+)**. It
also caught a vocabulary trap: "progress check" must be defined
mechanically (tick advances fabric OR pass declares fixed-point with a
recorded reason) or it launders into vibes; the critique demanded the
fixed-point declaration be a ledger entry, symmetric with
dropped-with-entry.

### Round 3 → the 6mo artifact

What must exist: **v0 cell IR + verifier** (the experiments lane's
current keel target), **one real pass (fold) with a red/green suite**,
**ledger lines in the dump format** (TUTORIAL §1–2 shape), **replay
determinism tested**, and a **QUF-shaped on-disk fabric format v0** so
"the compilation is a file" starts true on day one rather than being
retrofitted. Fog map v0 is a DOCTRINE-style page: per pass, what is
tested / proven / neither.

*Creative critique* (the discipline licensed to propose, not just
kill) added one thing worth keeping: **snapshot the fabric signature
at every tick from the very first pass run**, even with nothing to do
with them yet — the signature history is the raw material every later
batten scheme needs, and it is nearly free to record and impossible to
recover retroactively once passes churn. (This is the same shape as
quilt-verilog committing pre-fix logs as wedged-state evidence: record
now, use later.) The critique also *attempted* to add φ-as-wire-join
to the 6mo deliverable and was refused by the round-4 logic critique's
residue — φ is the arch lane's open question, not v0's.

### Round 2 → the keel+docs layer (now)

What must exist now: exactly what exists — keel README, THEORY,
DOCTRINE, TUTORIAL, GLOSSARY — **plus the two corrections this
backward walk produced**: (1) THEORY §6's conservation law needs its
progress-law pairing (lesson 2 above); (2) the docs should name the
coverage-map idea (§2b) as design-intent so the experiments lane
builds the signature-recording habit (round 3's addition) from the
first day. Both are this document's contribution back to the keel;
THEORY itself belongs to the arch lane and is not edited by this one.

*Ambition-check critique* verified the keel does not overclaim: README
says "nothing here is claimed to work yet," every doc carries a status
line, MLIR relation is honest. It flagged one ambition *risk* for the
future, not the present: the far-future vision's most steal-able
increment by outsiders is the quilt-file format + replay; if the fleet
ships only essays for long, someone else ships the format. Priced, not
acted on.

### Round 1 → v0 synthesis (what this walk freezes)

The convergence of the five rounds: five invariants that survived
every round and every critic discipline, and the free variables that
never settled.

---

## 5. Convergence — the payload

### Invariants (survived all rounds and all critics — freeze NOW)

1. **Conservation per tick (D4).** Admitted values are delivered,
   consumed-with-derivation, or dropped-with-entry — verified at every
   tick. Ground: 0 violations in 2,852,899 cycles including wedges
   (SILICON-EXPERIMENTS.md §2). Every round re-derived it; no critic
   discipline weakened it.
2. **Progress per tick (the F3 pairing).** Every pass either advances
   the fabric or declares fixed-point *as a ledger entry*. Conservation
   without progress is a balanced ledger on a dead ring. This survived
   as the round-4 logic critique's sharpest addition and belongs at
   invariant strength from v0.
3. **Append-only history with replay determinism (N4/D5).** fabric@k =
   replay(diffs 1..k), bit-for-bit, tested. Survives on its own
   merits *and* as the substrate that makes battens durable (§2c) —
   two independent reasons, frozen twice as hard.
4. **Fog declared, never averaged.** Verification coverage is a map
   with named fog (tested / proven / neither, per property, per
   module), and green does not launder across its boundaries. Ground:
   F2 lived in the module formal never touched while six proofs
   passed (SILICON-EXPERIMENTS.md §3.1). From v0 this is just an
   honest page; later it becomes the batten map.
5. **Provenance is structure, not side-channel.** Wires carry birth
   certificates; walks and "why" queries read the fabric itself. Every
   far-future product — replay, bisect, batten anchors in provenance
   space, curricula — is a query over this, so it cannot be deferred.

### Free variables (never settled — hold loose)

1. **The signature space and routing policy.** Whether batten-cascade
   pass routing is a real mechanism or stays a vocabulary depends on
   an unmeasured bet: that fabric-signature neighborhoods predict pass
   outcome quality. Nothing in the fleet measures this yet. Hold
   loose; record signatures from day one (round 3) and let data
   decide.
2. **φ-as-wire-join and region semantics.** The arch lane's open
   framing (GLOSSARY, ⚠ entries). Five rounds did not converge it;
   it is correctly *not* a v0 commitment.
3. **Storage: diff compaction and quilt-file growth.** Round 5 priced
   it as *the* engineering problem of persistent history. Policy
   (compact-but-replayable, prune-with-ledger, or
   externalize-by-age) is genuinely open and cannot be settled before
   real pass histories exist at real program scale.

---

## 6. Failures booked (D8)

- **DeepInfra MCP (all four iterator models: Seed-2.0-pro, Qwen3.6,
  Hermes-405B, Nemotron):** `inference prohibited, you have reached
  user-set limit` — billing-limit ceiling, this session. Zero calls
  landed.
- **DeepSeek direct API (fallback iterator lane):** `Insufficient
  Balance`. Zero calls landed.
- **MMX / MiniMax-M3 (second fallback):** weekly quota exhausted
  (`weekly_remaining_percent: 0`, quota API, this session).
- **Ollama node inference:** no connected nodes advertise Ollama.
- **Consequence:** all per-round critiques were performed by GLM-5.3
  (this agent) under four named disciplines — deep-reasoning, logic,
  creative, ambition-check — with discipline-level separation of
  powers (each discipline may only kill or only propose within its
  charter). **Independence was not achieved; the rounds are
  model-diverse in method, not in model.** A future session with a
  live lane should re-run each round's critique as a real external
  model call and diff the verdicts against this document; divergence
  there is signal, not noise.

---

*Related: [THEORY](THEORY.md) (the argument this walk corrects in two
places) · [DOCTRINE](DOCTRINE.md) (the laws the invariants obey) ·
[TUTORIAL](TUTORIAL.md) / [GLOSSARY](GLOSSARY.md) (the shapes and
words) · Fleet ground: quilt-verilog SILICON-EXPERIMENTS, VERIFICATION,
QUF-SPEC, INCIDENTS; batten-spline README.*

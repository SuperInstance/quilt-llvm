# THEORY — why a compiler IR should be a fabric

*An essay, not a spec. The spec-shaped work belongs to the arch lane's
ARCHITECTURE.md and the experiments lane's EXPERIMENTS.md; this page is
the argument underneath both. Status of everything here:
**design-intent** — quilt-llvm has a keel and this documentation, no
code. Every claim is either about something that already exists
elsewhere in the fleet (cited) or a statement of what we intend to
build (labeled). Nothing has been run.*

---

## 1. The claim, stated small

LLVM's real lesson was not "SSA everywhere," though that's the part
everyone quotes. It was that a compiler becomes a **library of
reusable transformations** the moment its intermediate representation
is explicit, typed, and inspectable by every pass on equal terms.
Passes stop being pipeline stages and start being swappable parts.

quilt's lesson — from quilt-verilog in silicon, the quilt engine on
Durable Objects, quilt-scratch — is that state lives in **cells**,
change travels on **wires**, and every transformation step is recorded
so it can be inspected and replayed. Nobody trusts a fabric that
rewrites itself silently.

quilt-llvm is the bet that these are the same lesson. An IR where the
program is a fabric of cells and wires, where every pass appends a
diff instead of mutating in place, where provenance is a walkable
first-class structure — is an IR where passes are library parts *and*
the whole compilation is inspectable after the fact. That's the whole
thesis. The rest of this page takes it apart and says what is fleet
fact versus what we intend.

## 2. Cells as values

SSA already says: every value is defined exactly once, and every use
points at its def. What it doesn't say is what a *value* is once
defined — in LLVM it's a node in a mutable graph that the next pass
may edit, recycle, or delete. The value's past is not part of the
value.

A **cell** inverts that. A cell is a value with:

- an **identity** that survives transformation of its content,
- **content** (opcode, type, operands — the usual IR payload),
- a **history** — the append-only record of everything that has ever
  happened to it.

quilt-verilog cells are learned state machines (dials, edges, a tick
schedule — see its
[README](https://github.com/SuperInstance/quilt-verilog/blob/master/README.md));
quilt-llvm cells are program values. Different payloads, same
discipline: the unit of state is small, named, inspectable, and never
edited in place. The RTL fabric proved the discipline scales to
silicon and a million-cycle simulation with zero ledger violations;
we intend to prove it survives contact with optimizer passes.

## 3. Wires as uses

In a conventional IR a use is a pointer from a later instruction to an
earlier one. Rewiring is a memory write — invisible unless you were
watching the memory.

In the fabric a **wire** is a first-class object: it has an identity, a
source cell, a destination cell plus operand slot, and its own history.
Rewiring — a pass replacing an operand — is not a write to a use-list;
it is a *recorded event*: old wire detached, new wire attached, both
appended to history. The def-use chain is the wire fanout; the
use-def chain is a walk against the wires.

The payoff is not aesthetic. When every rewiring is an event:

- a pass's diff **is** its report — you review what it touched, not
  what it claims it touched;
- a miscompile investigation starts from "what rewired this operand
  and when," which is a query, not a debugger session;
- determinism is structural — replay the events, get the same fabric —
  rather than hoped for out of pass ordering.

**φ as a wire-join** (this framing comes from the arch lane, in
flight): the value arriving in a block from multiple predecessors is
one cell receiving several wires, one per predecessor edge. The join
is explicit wiring rather than a special opcode. Whether this survives
implementation intact is the arch and experiments lanes' question;
here it stands as the clearest example of the general move — *syntax
becomes wiring, wiring becomes inspectable structure.*

## 4. Regions as blocks

A **region** is the fabric word for a block: a container of cells with
one entry, an order-preserving tick within it, and wires out to other
regions. Control flow is wires between regions. Nesting is
containment — a region cell holds child cells — not textual scoping,
which is what lets structured control flow (loops, branches) and flat
CFGs be the same object at different zoom levels. MLIR's regions are
the incumbent proof this shape works; we are not claiming novelty on
this axis, only continuity.

## 5. N4 as pass semantics

The quilt engine's fourth law (the keel's numbering, see the
[README](../README.md)): **history appends, never rewrites.** quilt's
cell stores work this way; quilt-verilog's ledger checks assume it.

Promoted to a compiler, N4 becomes pass semantics:

- a pass does not edit the fabric. It **appends a diff** — cells
  added, cells retired, wires attached and detached, ledger entries.
- the fabric at pass *k* is the replay of diffs 1..*k* over the
  initial fabric. Nothing more.
- "run a pass backwards" is not implemented: it is *unreplayed* — stop
  at *k*−1. Bisection of a miscompile is walking the history.

The consequences we care about, stated as intentions: pass replay from
any tick (determinism for free), IR bisection as a first-class
operation, and a compiler whose entire behavior after the fact is a
data structure you can query. The consequence that will cost us:
append-only means storage engineering (diff compaction, history
pruning policy) becomes a real problem, not a footnote. quilt-verilog
paid the same tax in trace storage; its answer (commit the trace, cite
the cycle count) is our floor.

## 6. Conservation as IR semantics

In quilt-verilog's scale runs, the conservation ledger —
cumulative in minus cumulative out equals what's still inside — held
at **0 violations across 2,852,899 cycles, including the wedged
states** of the F3 deadlock (its
[SILICON-EXPERIMENTS.md](https://github.com/SuperInstance/quilt-verilog/blob/master/docs/SILICON-EXPERIMENTS.md),
§2–§3). The fabric deadlocked; the accounting did not lie.

F3's promoted lesson, stated as an IR law: **a value admitted into a
transform is either delivered or explicitly dropped-with-ledger-entry
— it never silently vanishes.** In compiler terms:

- every input value to a pass is delivered to the output fabric,
  consumed into another value with a recorded derivation, or dropped
  with a ledger entry naming the reason (dead code, folded-away,
  subsumed);
- the verifier is an accountant: it reconciles the ledger at every
  tick, and a silent disappearance is a verification failure, not an
  optimization;
- DCE stops being a pass you trust and becomes a pass that files
  paperwork.

This is design-intent promoted from measured fleet fact. The fleet
fact: conservation accounting survives the worst states a fabric gets
into. The intent: it survives optimizer passes. The failure mode it
exists to catch is the silent one — the value that evaporates between
two ticks with no record, which in a conventional compiler surfaces
much later as a miscompile bug report from a stranger.

## 7. Provenance as a first-class, queryable structure

Standard compilers treat provenance as metadata: debug locations,
remarks, `-print-after-all` dumps. All side-channel. The fabric makes
it **structure**:

- "where did this constant come from" is a **provenance walk** —
  follow wires and history entries backwards until you hit the parse,
  or the pass that minted it;
- a backward slice is that walk with a predicate;
- "what did pass k do and why" is a query over one diff plus the
  ledger entries citing it.

What changes when provenance is queryable structure rather than debug
info is the *cost of asking*. The questions were always askable; in a
conventional compiler they cost a rebuild under a debugger. In the
fabric they cost a walk. We intend to demonstrate the difference with
concrete queries over a real miscompile-shaped example once the
experiments lane exists — until then this section is a promise, and
DOCTRINE ([DOCTRINE.md](DOCTRINE.md)) exists so promises get priced.

### 7.1 The reader-delta connection (restrained)

The zeroclaw dissertation's v3 thesis
([THESIS-V3](https://github.com/SuperInstance/zeroclaw-dissertation/blob/master/research/dissertation/THESIS-V3.md))
spends its sharpest pages proving that **a reader of a summary inherits
the summary's fiber** — the v2 room-reader died on the Switch Test
because a first-moment observable provably cannot distinguish clouds
that the room's own dynamics can produce. The reader wasn't broken;
the *summary it read* was a lossy projection, and the loss was
measurable.

The transfer to a compiler is one discipline, not a method: **a
pass-writer reads the IR the way the doctor reads the nurse's delta.**
The doctor trusts the delta for triage but knows exactly which
questions the delta cannot answer, and walks to the bedside for
those. A pass in this project reads the delta (the diff, the ledger,
a signature over cells) for speed — and when it summarizes, the
summary's fiber — the distinctions it provably cannot make — is part
of the pass's documentation, or the pass reads the full fabric. No
claim that compilation is medicine, no latent-space machinery
imported; just the rule that a summary is used with its blind spots
declared. THESIS-V3's fiber theorems are about rooms on S⁶; our
fibers will be about IR hashes and pass signatures, and they will be
much smaller claims, measured and written down.

## 8. Honest relations

- **quilt-verilog** is the direct ancestor: cell/wire discipline at
  RTL, the conservation ledger that held through F3, the formal
  culture (its proofs found two real defects before users did), and
  the documentation precedent this repo copies — docs as first-class
  artifacts, failures recorded first-class
  ([INCIDENTS.md](https://github.com/SuperInstance/quilt-verilog/blob/master/docs/INCIDENTS.md)).
  What does not transfer: its cells learn; ours are inert program
  values. Its opcodes (bind/link/effect/view/tick) are not our IR
  operations; the transferable part is *operations as recorded data*.
- **zeroclaw THESIS-V3** contributes the fiber discipline (§7.1) and
  one structural echo we note without leaning on: its adjoint
  inference runs *backward as more forward ops on the same schedule*
  (λ_t = Aᵀλ_{t+1}); our history replay runs backward as
  *unapplied* forward diffs. Both make "going backwards" a matter of
  running the same recorded machinery in reverse order. That is an
  analogy, labeled as one.
- **MLIR** is the incumbent and we will not launder vocabulary
  against it. MLIR dialects, regions, and operations already deliver
  structured, inspectable, transformable IR at industrial scale, with
  backends and a community we do not have. What MLIR leaves to
  tooling and convention — pass-history persistence, conservation
  accounting, provenance as queryable graph — is exactly the axis
  this project bets on. If that bet is wrong, the honest outcome is
  "a small MLIR dialect with a good ledger," and the docs will say
  so.

## 9. What this is not

Not a claim that LLVM or MLIR got it wrong. Not a research result —
nothing has been measured here yet, and the first measured numbers
belong to the experiments lane. It is a design commitment: an IR
whose unit is the inspectable cell, whose change unit is the recorded
wire, whose pass semantics is append-only history, whose invariant is
a conservation ledger, and whose product is a compilation you can
interrogate after it's over. The lanes that follow will either make
it real or fold it with a documented cause — both outcomes are
first-class.

---

*Related: [DOCTRINE.md](DOCTRINE.md) — the enforceable laws. ·
[TUTORIAL.md](TUTORIAL.md) — the design-intent walkthrough. ·
[GLOSSARY.md](GLOSSARY.md) — the term map. · Keel: [README](../README.md).*

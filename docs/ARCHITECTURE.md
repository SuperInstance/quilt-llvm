# QUILT-LLVM Architecture — Cells as IR

**Repo:** quilt-llvm · **Doc:** docs/ARCHITECTURE.md · **Date:** 2026-08-30
**Status:** design + plan. Nothing below is claimed to run. Every measured or
quoted claim carries a citation; everything else is labeled **design-intent**.

**Provenance.** This document was synthesized by the orchestrator (GLM-5.3)
from two CLI specialist lanes run as persistent tmux sessions, per doctrine:

- **Claude Code** (Sonnet 5), tmux session `llvm-arch-claude`, **3 rounds**
  (same session, `-c` continuation): IR theory → machinery + honesty →
  milestones + five sharpest points. Cited as `[C-rN]`.
- **KimiCode** (K3), tmux session `llvm-arch-kimi`, **2 rounds** (same
  session, `-r` continuation): spatial/structural pass → textual format +
  v0 sanity. Cited as `[K-rN]`.

CLI failures, booked honestly: the first `claude -p` launch died on tmux
quoting ("Input must be provided either through stdin or as a prompt
argument") — relaunched via stdin redirect, then clean. The first `kimi -p`
run died silently after 128 bytes (exit 0, no error text) — a plain retry
produced the full ~990-word pass, and `kimi -r` session-resume worked for
round 2 despite our prior note that "plain `-p` is the only working form."
No lane content was lost. Lane transcripts: `/tmp/llvm-arch/*.out` (ephemeral;
the quoted passages below are the durable record).

Source texts the lanes read: this repo's keel `README.md` (cells, N4, F3),
`quilt-verilog/docs/QUF-SPEC.md` (cell state as a file; hostile-input
reject-or-skip discipline, rules R1–R18), and
`zeroclaw-dissertation/research/dissertation/THESIS-V3.md` (conserved-manifold
substrate; conservation as *design target, not proven invariant*; backward
pass = adjoint = more forward ops on the same schedule).

---

## §1. The theory — cells as IR

### 1.0 The one-sentence version

A program is a **fabric** of **cells** (instruction cells, block cells,
function cells); values travel on **wires**; a pass is one scheduled
**tick** that fires matching cells, **appends** a diff to history (never
rewrites), and ledgers every drop. The IR is the fabric; the compiler is a
schedule over it.

### 1.1 Formal-ish definitions

These are definitions of the v0 object model — precise enough to implement
and verify, deliberately not a formal semantics (that is priced work, §3).

- **Fabric** `F = (C, W, R, ⊏, H)` — a finite set of cells `C`, wires `W`,
  a containment (region) relation `R ⊆ C × C` forming a forest, a per-region
  spine order `⊏` on children, and an append-only history `H` of diffs.
- **Cell** — the atomic unit of state and audibility. An instruction cell
  has: an opcode, typed input wire references, output wire definitions
  (usually one), and a stable id assigned at birth. A **region cell**
  (block, function) additionally owns a *spine* (ordered list of child cell
  ids) and *ports* on its boundary. As Claude put it: "The unit of
  compilation is the cell — a datum with a configurable address, input
  wires, and verifiable provenance. Not the instruction, not the function:
  the cell." `[C-r3]`
- **Wire** — a directed, typed edge from a defining cell (or a region
  in-port) to a using cell (or an out-port). "Every use of a value is a
  directed edge in the graph. A value's provenance is not implicit in
  naming (as in SSA); it is a path of wires you can walk backward to the
  source and forward to all sinks." `[C-r3]` Ctrl-wires are the subtype
  that carry *which block fires next* `[K-r1]`; val-wires carry data.
- **Region** — a cell that contains cells. Uniform nesting: function ⊃
  block ⊃ instruction; later loop/if regions reuse the same shape. The
  load-bearing structural idea is Kimi's: **ports**. "Each in-port of a
  block is a splice point where an exterior wire becomes an interior wire…
  a phi cell sits at the splice and *multiplexes by control provenance*.
  In hardware terms it's a mux whose select lines are the incoming
  terminator wires." `[K-r1]`
- **Tick / pass** — see §1.4.

**The SSA correspondence.** value = cell output; use = wire; block = region
cell; function = region hierarchy; phi = wire-join at a region boundary
(control-clocked mux). Claude's honesty about where this is exact and where
it strains `[C-r1]`, condensed:

1. **Where it is exact:** straight-line code and simple control flow — the
   mapping is faithful.
2. **Dominance.** SSA *computes* dominance (frontier algorithm, post-hoc);
   the fabric *reifies* it as topology — "dominance is not computed; it is
   structurally auditable — you walk the wires and see where they merge."
   But this is weaker than it sounds: the verifier must still *check* the
   topology (rule V4 below), so dominance is **checked-structural, not
   free**. Claude's round-3 phrasing ("violations are structurally
   impossible") is aspirational; the honest v0 statement is that dominance
   violations are *locally detectable at the wire*, not that they cannot
   occur. Orchestrator's tempering, not the lane's.
3. **Predecessor convenience.** SSA lists block predecessors syntactically;
   the fabric infers them from ctrl-wires. "The trade is auditability vs.
   convenience" `[C-r1]` — one way to encode a fact (wires only), at the
   price of reverse-edge walks.
4. **Phi operand order.** SSA's i-th phi operand pairs with the i-th
   predecessor by syntax; a fabric wire-join has no inherent order, so each
   incoming wire must *carry its source port label* explicitly — "a
   verification obligation that SSA doesn't bear" `[C-r1]`.

### 1.2 φ-nodes as wire-joins — the exact story

The merge picture, from the spatial lane (reproduced because the whole IR
hinges on it) `[K-r1]`:

```
  ┌─BLOCK entry───────────────┐
  │ [I0: %c = icmp sgt %a,%b] │◄── %a, %b arrive on param wires
  │ [T0: br %c, then, else]   │──┐  terminator cell, 2 ctrl wires
  └───────────────────────────┘  │
                    %c ──────────┼───────────────┐ (cond wire)
                          ┌──────┴───┐   ┌───────┴──┐
              ┌─BLOCK then▼────┐  ┌─BLOCK else▼────┐
              │ [T1: br merge] │  │ [T2: br merge] │
              └───────┬────────┘  └───────┬────────┘
            %a on val-wire │              │ %b on val-wire
                    ┌──────▼──────────────▼──────┐
                    │  BLOCK merge               │
                    │  [P0: %r = phi(%a@then,    │◄── join cell:
                    │             %b@else)]      │    mux clocked by ctrl
                    │  [T3: ret %r]              │
                    └────────────────────────────┘
```

Three consequences the lanes drew, which we adopt as theory:

- **"SSA is a routing property, not a naming convention."** Two defs of
  "the same variable" are two wires; the join is where they are admitted
  into one interior name. The verifier checks wires, never name strings.
  `[K-r1]`
- **Wires crossing boundaries are explicit and countable** — every
  cross-region wire passes exactly one in-port and one out-port, which is
  what makes the conservation law *checkable per port* (§1.5). `[K-r1]`
- **Dominance becomes spatial**: "cell X dominates block B iff every
  ctrl-path to B's in-ports passes X's region." `[K-r1]` (design-intent
  until the verifier enforces it; see V4.)

Where it breaks, honestly `[C-r1]`: a join whose incoming wires lose their
source-port labels becomes ambiguous (V11); a wire from an unreachable
region is structurally legal (SSA's syntax would have forbidden it) and
must be caught by the verifier (V4-reachability); a wire from a
non-dominating source into a join is undefined semantics, not a type error
— reject at verify time, never interpret.

### 1.3 N4 as pass semantics — the pass is a pure function

**Definition (pass).** A pass is a pure function

```
pass : (Fabric, Config) → (Fabric', Diff)
with Fabric' = Fabric ⊕ Diff
```

The pass never mutates in place; it returns the new fabric *and the proof
of what changed*. The diff is the minimal edit log (cells added/dead-marked,
wires added/rewired, ledger drops), strictly ordered, non-overlapping
(monotone sequence numbers — V8). This is the N4 law ("history appends,
never rewrites" — keel README) promoted from a storage rule to **pass
semantics**.

What replay buys, from the strategy lane `[C-r1]`, condensed:

1. **Auditability** — verify the diff once, trust it forever; every delta
   is a checkable list, not a re-run of the pass's logic.
2. **Backward exploration** — `Fabric₀ = Fabric₁ ⊖ Diff₁`; intermediate
   states are never discarded, so runs rewind. (Diff inversion is unique
   given both states — the diff *is* the pairing.)
3. **Incremental verification** — verify diffs in isolation, then check
   domains/codomains compose; no re-analysis of combined effects.
4. **Parallelism/slicing** — passes touching disjoint cells have
   independent diffs; slices merge without recompute.
5. **Determinism-via-log** — "what did pass X do to cell Y?" is a ledger
   grep, not an instrumented re-run.

And the contrast that motivates it: "In a traditional compiler (e.g. LLVM's
legacy pipeline), a pass mutates the IR in place. The old IR is lost." `[C-r1]`

### 1.4 The tick — a pass walk as a scheduled sweep

The compiler analogue of quilt-verilog's fabric-wide, starvation-free tick
(keel: decay + fire + fanout, per QUF's `ticks` section), from the spatial
lane `[K-r1]`:

```
tick N (pass "const-fold"), phase order: post-order over region tree
  for each region R in schedule:
    decay(R)  — age every wire's use-count by 1 tick; counts hitting 0
                become DCE-eligible (marked, not removed)
    fire(R)   — visit each cell once, in spine order; a cell fires iff its
                neighborhood matches the pass rule; firing APPENDS a diff
    ledger(R) — every fired diff appends to history; every marked-dead cell
                gets {drop: id, reason} — never silent
```

The correspondences: **fire = pattern rule** (never mutates, appends);
**decay = use-count aging** (wire use-counts as Hebbian buckets — DCE
becomes "a threshold read in a scheduled sweep phase," emergent from the
same tick machinery, not a special pass); **cannot starve = the schedule
is total** (every region visited exactly once per tick; no fixpoint loops
inside a tick — convergence is *repeating the tick* with a no-diff
steady-state detector, "so a non-converging pass shows up as a schedule
that never idles — visible, meterable, killable"); **phases = parallelism
discipline** (regions with no crossing live wires may fire in the same
phase). `[K-r1]`

The one-sentence version, quoted whole: **"a block is a spine of cells
with boundary ports, a phi is a control-clocked splice at the boundary,
and a pass is one starvation-free tick — decay the use-counts, fire the
matching cells, append the diffs, ledger the drops."** `[K-r1]`

Design-intent caveat, ours: the tick framing is a *metaphor with teeth* —
the teeth are the schedule totality and the no-fixpoint-inside-a-tick
rule, both checkable. v0 implements passes as deterministic ordered walks
(the "tick schedule" is an array), not as any concurrent machinery.

### 1.5 Conservation (F3) as IR semantics

**Law (conservation, F3 promoted).** Every value admitted into a
transformation is either present in the output fabric or has an explicit
drop entry in the diff's ledger. No value silently vanishes. (Keel README;
verifier rule V5.)

This is the differentiator, and we state its status precisely: within
quilt-llvm, conservation is **enforced-by-construction of the pipeline**
(every pass emits diffs; the verifier rejects a diff with unaccounted
values — machine-checkable, V5/V7). It is *not* a proven theorem about all
possible passes, the way THESIS-V3's balanced-write condition (1ᵀH = 0,
mass-neutral moves, not mints) is an obligation with a stated gap on the
silicon fabric. We inherit that thesis's honesty clause verbatim in
spirit: **conservation here is a design target enforced by the verifier,
not a proven invariant of the metal.** What the ledger gives that LLVM
does not: "a pass that drops a value without a ledger entry is rejected
outright" `[C-r3]` — enforcement moves from pass-author discipline to the
verifier.

The adjoint flavor from THESIS-V3 maps cleanly and is adopted as
design-intent for later: the ledger makes *backward* queries (replay,
rewind, per-cell history) into *more forward ops over appended diffs* —
"backward as more forward ops on the same schedule."

---

## §2. v0 — the concrete plan

**v0 is a proof of concept, not a production compiler** `[C-r2]`. It proves:
(a) the cell IR is parseable, printable, verifiable; (b) passes append
replayable diffs under a ledger; (c) differential testing against a
reference compiler catches bugs. Everything else is deliberately out
(§3).

### 2.1 The minimal cell IR

Instruction set (fixed; no traits/interfaces meta-system in v0):

- **const** — `c9 = const.i32 5 -> %w9`
- **arith** — `add`, `sub`, `mul` (i32, wraparound defined — *defined*
  wraparound is what keeps differential testing sound; no UB anywhere)
- **compare** — `icmp.sgt` etc., producing an i1 wire
- **branch** — `br %cond, b_then, b_else` / `br b_target`; terminator
  cells, the only source of ctrl-wires
- **phi-as-wire-join** — `c5 = phi %w3 [ %w0 @ p.a, %w1 @ p.b ]`: the join
  cell at a region's in-ports, muxing by ctrl provenance (§1.2)
- **call stubs** — `call @f(args)` as a cell with a wire to a function
  cell; v0 semantics = interpreter dispatch, no inlining until M5
- **ret** — terminator of a function region

Structural rules: a block's spine ends in exactly one terminator; phi
cells sit only at a spine head; ids are monotone and never reused;
every wire is defined at exactly one line and referenced by id elsewhere.

### 2.2 Textual format (`.qlf`)

Human-readable, diffable, and — the load-bearing design law — **"the text
is a serialization of the cell graph, not a program listing"** `[K-r2]`.
Four rules make `diff` between pipeline stages meaningful `[K-r2]`:

1. **Print order is canonical, not source order** — blocks in computed
   reverse post-order of the ctrl graph (deterministic tiebreak: block-id
   order); spine cells in spine order. "Two fabrics with identical
   structure print byte-identically regardless of which pass built them."
2. **Wires print at definition, reference by id** — a wire is born on
   exactly one line; sinks move during transforms, defs don't; "defs
   don't (until the def itself is folded, which *is* the diff you want
   to see)."
3. **Ids are forever** — "Const-fold never renames c4 to c1; a new
   constant cell gets the next id from `alloc`. Diffing two stages shows
   only appended cells, appended wires, and `~` dead-marks — a rewrite
   that renumbers the world is un-reviewable, so the format forbids it."
4. **One entity per line, no whitespace freedom** — the emitter is the
   only writer; no hand-authored pretty-printing drift.

Ledger lines appended per tick (N4): `@` added, `~` dead-marked, `!`
rewired, plus the conservation ledger line — `admit`, `drop reason=…`,
`ok` `[K-r2]`. A before/after `git diff` between pipeline stages is then a
real pass-cost metric (design-intent until M3 demonstrates it).

### 2.3 Verifier rule list (V1–V11)

Modeled on QUF-SPEC's reject-or-skip discipline: every rule rejects with a
machine-readable code; check order is fixed; no partial acceptance — "a
hostile or corrupted fabric is rejected in whole before any state change"
`[C-r1]`. From the strategy lane, with the spatial lane's check-order
discipline folded in `[C-r1, K-r2]`:

| rule | checks | failure |
|---|---|---|
| V1 | wire cell-ids in `[0, cell_count)` | reject |
| V2 | wire slots/dial indices in range | reject |
| V3 | region containment acyclic (forest) | reject |
| V4 | dominance respected per wire: def dominates use, or same-region spine order, or labeled join input from a real predecessor; plus ctrl-reachability (no wires from unreachable regions) | reject |
| V5 | **conservation**: every diff-admitted value present in output or ledger-dropped | reject + missing value id |
| V6 | single def per value except labeled joins (no multiply-/un-sourced values) | reject |
| V7 | diff validity: claimed old state matches, claimed new state matches | reject |
| V8 | history monotone: strictly increasing tick numbers; chained diffs agree at the seams | reject |
| V9 | value quantization/range fits declared format; overflow saturates *and is marked* | reject |
| V10 | type consistency along every wire (or explicit conversion cell) | reject |
| V11 | every join input carries a valid source-port label naming a real predecessor | reject |

Cheap mode (between passes): V1–V3 + diff-local V5. Full mode (pipeline
end, CI): all V1–V11 including dominance recomputation `[C-r2]`.

### 2.4 Pass manager sketch

An ordered pipeline with attestation `[C-r2]`, where each tick appends one
ledger entry:

```
PassManager { fabric: Fabric, ledger: Vec<LedgerEntry> }
LedgerEntry { pass_name, hash_before, hash_after, diff: Vec<DiffOp>,
              verifier_mode, violations }
DiffOp { kind: CellAdded|CellDeadMarked|WireAdded|WireRewired|Dropped,
         cell_id, wire_id, old, new, reason }

for pass P in pipeline:
    (fabric', diff) = P.run(fabric)          # pure; N4
    verify_cheap(fabric', diff) or reject
    ledger.push(entry{ hash(fabric), hash(fabric'), diff })
    fabric = fabric'
verify_full(fabric)
```

Pass instrumentation becomes **ledger queries**, not instrumentation:
per-pass stats = count diff kinds by `pass_name`; "what was cell Y after
pass N" = apply `ledger[0..N]` to the initial fabric; and a pass-commute
analysis — "if pass_A's diffs and pass_B's diffs touch disjoint cells,
they're order-independent" `[C-r2]`. "This is free introspection. In LLVM,
you instrument passes to emit this; here, it falls out of the ledger
structure." `[C-r2]`

### 2.5 Test culture

**Red/green per pass.** Every pass ships a fixture pair: a fabric-before,
an expected diff + expected fabric-after hash. RED: run the pipeline
*without* the pass, assert the ledger contains no entry for it and the
output hash differs. GREEN: run with it, assert the ledger entry, the
expected diff ops, and the final hash, then `verify_full`. "The red test
ensures the pass is actually doing something; the green test ensures it
does the right thing." `[C-r2]` The verifier's reason codes double as the
mutant battery: each V-rule needs a golden mutant that trips exactly it
(and only it, per check order) — the QUF mutant-corpus method
(QUF-SPEC §5a) transplanted to IR verification.

**Differential testing vs a reference (gcc/clang at -O0).** The sound
program class, from the pitfalls list `[C-r2]`: i32 arith with *defined*
wraparound, comparisons, bounded branches, bounded loops, fixed-index
arrays, non-recursive calls (or bounded depth); **no** floats (fabric is
integer-only), **no** UB (no signed overflow — wraparound is defined in
the IR and avoided in the C reference), **no** uninitialized reads (fabric
zero-inits dials), **no** pointer arithmetic/aliasing, **no** I/O or
side effects, single-threaded. Harness: N ≥ 50 programs × exhaustive
small-input grids (e.g. a,b ∈ [−100,100]), assert interpreter output ==
reference output on every point. Zero mismatches is the bar, and any
mismatch is a first-class incident, not a flake.

---

## §3. The honesty section

What LLVM and MLIR do better, and why we are not pretending otherwise.
The strategy lane's inventory, condensed and adopted `[C-r2]`; the
estimates are the lane's judgment calls, recorded as such.

- **Instruction selection / backend.** LLVM's SelectionDAG is a
  constraint-satisfaction engine over latency, pressure, addressing modes
  — "thousands of engineer-hours across 25 years," still heuristic. A v0
  cell-to-machine lowering would be naive 1-to-1, "2–5x slower than LLVM
  for typical integer code." Per-architecture tuning to close that:
  ~3–5 years. **v0 emits no machine code at all.**
- **Register allocation.** NP-hard in general; LLVM's graph coloring +
  live-range splitting + rematerialization vs a greedy spilling allocator:
  "correct; slow." ~2–3 years per architecture to do well. **Not in v0.**
- **Vectorization/SIMD, alias/escape analysis, PGO, IPO.** Dependence
  analysis, TBAA, memory SSA — each a multi-year lane of its own; v0
  assumes all memory aliases (there is barely memory in v0) and optimizes
  nothing interprocedurally except the one inliner.
- **MLIR's trait/interface system.** "MLIR's genius is that it separates
  the IR structure from the operation semantics… extensibility without
  forking. The cell fabric is more monolithic: cell types are fixed in
  the IR spec." Designing a meta-system: 6–12 months to get right.
  **v0 extends the opcode set by hand and says so.**
- **MLIR's nested regions.** Production region semantics (lambdas,
  scoping, capture, invariant maintenance) are beyond our block/function
  regions: "keep regions simple." **v0 regions are blocks and functions,
  nothing fancier.**
- **Cranelift/WASM as comparators.** "Cranelift is a mid-level IR designed
  for fast compilation, not fast code… you can ship a compiler that
  produces reasonable code without the sophistication of LLVM, as long as
  you're honest about the trade-off." v0 adopts that philosophy *minus*
  the fast-compilation claim — we make no performance claims of any kind.
- **What will take years even on our own thesis:** making the ledger
  cheap enough that a 100-pass pipeline doesn't drown in diff volume;
  incremental RPO at scale (the spatial lane's flagged first break —
  §2.2/§4, M4 note); any equivalence proof between the interpreter and
  any backend (quilt-verilog carries the same gap between its Python lane
  and RTL — "a model, not a miter," VERIFICATION.md).

**What v0 deliberately is NOT:** not a production compiler; not a backend;
not multi-target; not UB-tolerant (it forbids UB by construction instead);
not a traits meta-system; not debug-info-capable; not IPA beyond one
inliner; not dynamically scheduled. "v0 is a proof of concept… Once v0
proves this, v1 can add sophistication. Trying to match LLVM's code
quality or MLIR's extensibility in v0 is a death march." `[C-r2]`

**LLVM parity is a direction, not a claim** (keel). Repeated here so no
section of this doc can be read as a challenge to 25 years of
infrastructure. The only thing v0 claims is the ledger: append-only pass
history, machine-checked conservation, structural audit — and only once
M1–M6 are green.

---

## §4. Milestone ladder M1–M6

Exit criteria are commands that pass, not prose that persuades. Structure
per the strategy lane's critique `[C-r3]`: phi-coalesce lowering is *out*
of v0 (the interpreter executes joins directly); DCE is the load-bearing
milestone because "the cell-as-IR thesis stands or falls on F3 — M1
validates the *structure* of conservation; [M4] validates the
*enforcement* in practice. If [M4] passes, you've proven the thesis
works. Without it, conservation is just a specification." `[C-r3]`

- **M1 — Cell IR v0 + textual format + verifier.**
  `verify golden.qlf == OK && for each of ≥20 golden mutants: reject with
  the exact expected V-code` — golden + mutants checked in; round-trip
  `parse → print → parse` byte-identical.
- **M2 — Interpreter + differential harness.**
  ≥50 programs from the sound class × exhaustive small-input grids vs
  `gcc -O0`: zero mismatches. (Load-bearing: this is the ground truth
  every later pass is judged against; it must precede any transform.)
- **M3 — Pass manager + ledger + const-fold.**
  Red/green fixture passes; every pipeline run leaves one ledger entry per
  pass with matching `hash_before/hash_after`; replaying the ledger from
  the initial fabric reconstructs the final fabric bit-exactly.
- **M4 — DCE as decay sweep (conservation enforced).** *(load-bearing)*
  Red/green passes; for every dead-marked cell the ledger carries a drop
  entry; `verify_full` green post-pass — conservation checked, not
  assumed.
- **M5 — Inlining on fabrics.**
  Red/green passes; full differential suite still zero-mismatch post-inline
  (soundness of the one interprocedural transform, witnessed).
- **M6 — Replay/audit tooling.**
  For every step k: `reconstruct(ledger[0..k])` == ground-truth fabric k,
  byte-exact; commute query correctly predicts identical outcome for a
  deliberate order-swap of two disjoint-touch passes and *flags* a pair
  that does not commute.

Post-v0 (booked, not scheduled): phi-coalesce/lowering lane, a second
backend-shaped target, incremental scheduling at 10k cells, diff-volume
budgeting. The spatial lane's warning is the first entry in that ledger:
"the first thing that breaks at scale is recomputing the canonical walk,
so keep ticks region-local." `[K-r2]`

---

## §5. Attribution summary

| Content | Lane, round |
|---|---|
| SSA↔cell correspondence + strain points; φ as wire-join breaks; V1–V11; replay benefits | Claude Code (Sonnet 5), r1 `[C-r1]` |
| Pass manager/ledger structures; ledger queries; differential program class + pitfalls; honesty inventory; v0 skip-list | Claude Code (Sonnet 5), r2 `[C-r2]` |
| Milestone critique; five sharpest points; M4-is-load-bearing argument | Claude Code (Sonnet 5), r3 `[C-r3]` |
| Fabric layout ASCII; region ports; φ as control-clocked mux; pass-as-tick (decay/fire/ledger, starvation-free schedule) | KimiCode (K3), r1 `[K-r1]` |
| `.qlf` textual format + diffability laws; v0 fourth-pass argument; verifier check order; RPO-at-scale warning | KimiCode (K3), r2 `[K-r2]` |
| Conservation honesty clause (design target, not proven invariant); backward-as-forward-ops framing | THESIS-V3 (read, not generated) |
| Reject-or-skip mutant discipline; reason codes; check order | QUF-SPEC §5a (read, not generated) |
| Temperings, synthesis, structure, all framing above | GLM-5.3 (orchestrator, this document) |

Orchestrator's own five sharpest points (the doc's thesis in five lines,
crediting the producing lane for each):

1. **The cell is the value; the wire is the use; the block is a region
   cell.** The correspondence is faithful for sequential code and weakens
   exactly where dominance is the structural protagonist. *(Claude r1/r3)*
2. **A φ-node is a control-clocked mux at a region boundary** — SSA
   becomes a routing property, not a naming convention; every join input
   must carry its source port, a verification obligation SSA's syntax
   never bore. *(Kimi r1, Claude r1)*
3. **A pass is a pure function `fabric → (fabric, diff)`** — history-append
   (N4) makes the pipeline replayable, rewindable, slice-parallel, and
   greppable; in LLVM "the old IR is lost," here the old IR is *always*
   the current fabric minus an append. *(Claude r1)*
4. **A pass walk is one starvation-free tick** — decay the use-counts,
   fire the matching cells, append the diffs, ledger the drops; DCE
   becomes a threshold read on aged use-counts, emergent from the same
   machinery as everything else. *(Kimi r1)*
5. **Conservation is enforced, not aspirational** — admitted values are
   delivered or ledger-dropped, never silently lost; the verifier rejects
   a leaking diff outright, moving soundness discipline from pass authors
   to the machine. *(Claude r3; keel F3; honesty clause inherited from
   THESIS-V3)*

— synthesized and written by GLM-5.3 (orchestrator lane), 2026-08-30.

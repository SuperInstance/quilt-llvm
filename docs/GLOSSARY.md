# GLOSSARY — fleet words and compiler words, mapped

*Two vocabularies already exist in this house: the quilt fleet's
(cells, wires, ticks, ledgers) and compilers' (values, uses, blocks,
passes, passes' accounting). This page maps them exactly and says
where they do **not** line up — no hand-waving, no quietly borrowing
the strong word. Status: terminology is design-intent like everything
else here ([DOCTRINE](DOCTRINE.md) D8); entries marked ⚠ mark real
collisions to watch.*

## The map

| fleet word | compiler word | exact relation |
|---|---|---|
| **fabric** | IR module / program unit | The whole cell-wire graph at a given tick, plus its history. Where an LLVM `Module` is the current state, a fabric is the state *and* the ledger *and* every diff so far. |
| **cell** | value / instruction (SSA node) | A defined-once value with payload (opcode, type), identity that survives content changes, and append-only history. An LLVM `Value` minus mutation, plus a past. Not to be confused with quilt-verilog's cells, which are *learned state machines* — same discipline (small, named, inspectable, never edited in place), different payload. |
| **wire** | use (def–use edge) | A first-class object: identity, source cell, destination cell + operand slot, own history. An LLVM `Use` is a pointer; a wire is a pointer *with a birth certificate*. Detaching/attaching a wire is a recorded event. |
| **fanout** | use list / def–use chain | All wires out of a cell. "Zero fanout" = dead, and is directly observable, not derived. |
| **region** | basic block / (MLIR) region | Nestable container of cells: one entry, in-region order, wires out to other regions. Nesting is containment, not syntax. Maps to basic block in v0; the MLIR region is the honest name for the nested general form. |
| **tick** | (no exact incumbent) | One pass application = one tick = one appended diff. ⚠ Pun alert: quilt-verilog's *fabric-wide tick* is a clocked sweep of the whole fabric (time passes, decays run). Here a tick is a discrete pass boundary — same "time passes, fabric changes" sense, different mechanism. We keep the word for continuity and eat the collision. |
| **pass** | pass | Same intent (swappable transform), different contract: a pure function `fabric → (fabric, diff)` — per the arch lane, design-intent. Incumbent passes mutate; fabric passes append. |
| **diff** | (no incumbent) | The appended record of one tick: cells minted/retired, wires attached/detached, ledger entries. The diff is the pass's report — reviewed, testable (D1), replayable (D5). |
| **ledger** | def–use accounting / verification bookkeeping | The conservation record: every admitted value is **delivered** or **dropped-with-entry**. The verifier reconciles it each tick. Incumbents have nothing standing; closest relative is a debug checker in a testing pass. |
| **dropped-with-entry** | "optimized away" | The fabric version must name the reason and the tick. "Optimized away" without a record is exactly the silent vanishing D4 forbids. |
| **retired** | deleted / erased | ⚠ Not deleted: a retired cell is out of the live fabric but present in history; `replay 0..k` resurrects it. Incumbent deletion is final; retirement is reversible by construction. |
| **provenance walk** | backward slice / use–def traversal | Following wires and diffs backwards from a value to its origins (see [TUTORIAL §4](TUTORIAL.md)). Incumbent equivalent exists as an analysis you write; here it's a query over structure the IR already maintains. |
| **history replay** | pass replay / IR bisection | `fabric@k = replay(diffs 1..k)`. Determinism made structural (N4) rather than emergent from pass ordering. |
| **conservation law** | (no incumbent) | Fleet inheritance: quilt-verilog's ledger held 0-violations through 2.85M sim cycles including deadlocks; promoted here as D4. |
| **fiber** | observable blind spot | From zeroclaw THESIS-V3: the set of states a summary cannot distinguish. Used here only for our own summaries (IR hashes, pass signatures): if a pass reads a summary, the summary's fiber gets documented. Not imported beyond that discipline. |

## Term rules worth writing down

- **"fold" means constant folding.** The fleet also uses *fold* for a
  project ending (zeroclaw v2 folded; a lane folds). Inside quilt-llvm
  docs, `fold` is the constant-folding pass, full stop; diff
  application is **replay**; a project dying is "folds, with a
  documented cause." ⚠ collision acknowledged, resolved by rule, not
  by pretending the word has one meaning.
- **N4** — the quilt engine's law number, per the keel: *history
  appends, never rewrites.* Cited as `N4`; we don't renumber it per
  repo.
- **F3** — quilt-verilog's saturation deadlock incident; the ledger
  held through it. "F3's lesson" in our docs = the conservation law's
  origin story ([THEORY §6](THEORY.md)). We don't create F-numbers
  here; incidents get their own doc if we earn any.
- **φ as wire-join** — arch-lane framing (design-intent): the
  multi-predecessor value is one cell, several wires, explicit join
  structure instead of a φ opcode. Kept as the arch lane's term until
  the experiments lane either implements it or retires the framing.
- **Vocabulary laundering is banned** (D8): we do not call something
  "verified," "proven," or "MLIR-class" because it would be nice.
  Verified means a re-run command in this repo; everything else is
  design-intent with its status line on it.

---

*The argument for these shapes: [THEORY](THEORY.md). The walkthrough
that uses them: [TUTORIAL](TUTORIAL.md). The laws that bind them:
[DOCTRINE](DOCTRINE.md).*

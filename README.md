# quilt-llvm — a compiler fabric

**Work has begun. Nothing here is claimed to work yet.** This keel exists so
the lanes that follow build on a named idea, not a blank repo.

## The keel

quilt-llvm is a compiler infrastructure where the unit of compilation is the
same as the unit of quilt: **the inspectable cell**. LLVM's lesson was that a
compiler is a library of transformations over an explicit, typed,
 SSA-like intermediate representation. quilt's lesson (quilt-verilog,
quilt-scratch, the quilt engine on Durable Objects) is that state lives in
cells, change travels on wires, and every step of every transformation is
inspectable and re-runnable.

So:

- **IR as fabric.** A program is a fabric of cells (instruction cells, block
  cells, type cells). Every cell inspectable; every value's provenance is a
  wire you can walk.
- **Passes as swappable transformations.** A pass reads a fabric, writes a
  new fabric, and *records the diff* — history appends, never rewrites (N4
  law). Run a pass backwards if you want; the history is the trail.
- **Conservation law.** A value admitted into a transform is either delivered
  or explicitly dropped-with-ledger-entry — never silently vanishes. (F3's
  lesson, promoted to a compiler law.)
- **LLVM parity is a direction, not a claim.** We start with a cell IR, a
  verifier, and a handful of real passes (constant folding, DCE, inlining on
  fabrics). We will say exactly what works and what doesn't. No vocabulary
  laundering — if a piece is a toy, the docs say toy.

## Doctrine

Measured numbers, failures first-class, undersell/overdeliver. Every claimed
hash reachable in git log. Every pass ships with tests that go red without
the pass and green with it. Archive-by-rename; nothing destroyed.

## Status

- [x] Keel (this file)
- [ ] Scout report: what exists in the fleet to reuse
- [ ] Architecture doc (dissertation-grade: cells as IR, the theory)
- [ ] Cell IR v0 + verifier
- [ ] First real pass + red/green tests

### Docs (this expanse landed 2026-08-30 — all design-intent until code exists)

- **[docs/THEORY.md](docs/THEORY.md)** — the essay: why a compiler IR
  should be a fabric. Cells as values, wires as uses, regions as
  blocks; N4 (append-only history) as pass semantics; conservation
  (quilt-verilog F3's lesson, promoted) as IR semantics; provenance as
  a first-class queryable structure; honest relations to quilt-verilog,
  zeroclaw THESIS-V3, and MLIR.
- **[docs/DOCTRINE.md](docs/DOCTRINE.md)** — the eight working laws
  binding every lane: red/green pass tests, reachable hashes, measured
  numbers, conservation ledger, append-only history, archive-by-rename,
  verification-before-report, undersell/label-intent. One page,
  enforceable.
- **[docs/TUTORIAL.md](docs/TUTORIAL.md)** — compile a program you can
  watch: a tiny function walked by hand through the planned v0 textual
  IR — parse to fabric, constant fold as an appended diff, DCE as
  deletion-with-paperwork, provenance walk back to the parse, replay
  to any earlier tick. Labeled design-intent; the experiments lane
  makes it real or corrects it.
- **[docs/GLOSSARY.md](docs/GLOSSARY.md)** — fleet words and compiler
  words mapped to each other exactly (cell↔value, wire↔use,
  region↔block, tick↔pass, ledger↔def-use accounting…), with the real
  collisions (fold, tick) resolved by written rule.

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

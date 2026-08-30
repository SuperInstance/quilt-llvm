# DOCTRINE — the working laws for every lane

*One page. These bind every lane that touches quilt-llvm — arch,
experiments, and any rescue or review lane that follows. They are
inherited from the keel
([README](../README.md)), the fleet's
[quilt-verilog](https://github.com/SuperInstance/quilt-verilog/blob/master/docs/DOCTRINE.md)
doctrine, and its
[INCIDENTS](https://github.com/SuperInstance/quilt-verilog/blob/master/docs/INCIDENTS.md)
record — each law below exists because a fleet project learned it the
expensive way. Enforcement notes are part of the law, not commentary.*

**D1 — Red/green pass tests.** Every pass ships with tests that are
**red without the pass and green with it** — the suite must fail on a
fabric that hasn't been transformed and pass after. A test that would
pass either way tests nothing.
*Enforcement: CI runs each pass's suite against the pass stubbed out;
a suite that stays green fails the lane.*

**D2 — Reachable hashes.** Any commit hash cited in any doc must be
reachable in `git log`. A hash you can't `git show` is a fabrication,
not a citation.
*Enforcement: docs-lane link check + reviewer spot-check; laundered
hashes are an incident, not a typo.*

**D3 — Measured numbers.** No performance, coverage, or size claim
without the number and how it was measured. "Fast," "small,"
"significant" are banned unless a unit and a method follow them.
Unmeasured, say "unmeasured."
*Enforcement: claims cite a command and an output; see
[SILICON-EXPERIMENTS.md](https://github.com/SuperInstance/quilt-verilog/blob/master/docs/SILICON-EXPERIMENTS.md)
for the house format.*

**D4 — Conservation ledger.** A value admitted into a transform is
**delivered or explicitly dropped-with-ledger-entry** — never
silently vanishes. The verifier reconciles the ledger every tick; a
silent disappearance is a verification failure.
*Enforcement: the verifier's ledger check runs in every test; a pass
that loses a value fails D1 by construction.*

**D5 — Append-only history (N4).** Passes append diffs; nothing
rewrites. The fabric at tick k is the replay of diffs 1..k — replay
from any point must reproduce the fabric bit-for-bit.
*Enforcement: every test suite includes at least one replay-from-mid-
history check.*

**D6 — Archive-by-rename.** Nothing is deleted. Retired files,
superseded drafts, trimmed content get renamed (`*.archived-YYYYMMDD`
or moved to `_archive/`). Destroy-only operations need Casey's
explicit sign-off.
*Enforcement: PR diff shows renames, not removals.*

**D7 — Verification-before-report.** No lane reports green without
re-running the thing it claims is green, in this working tree, this
session. "It was green when I wrote the branch" is not a report.
*Enforcement: reports cite the re-run command + output; reviewers may
demand the re-run live.*

**D8 — Undersell, label intent.** Claims are cited or labeled
**design-intent**; failures and dead ends are recorded first-class
(an INCIDENTS-style entry), not folded quietly into success. If a
piece is a toy, the docs say toy.
*Enforcement: every doc carries its status line; vocabulary laundering
(claiming MLIR/LLVM words for unfinished work) is an incident.*

*Short version: **measured, reachable, conserved, append-only,
renamed-not-deleted, re-run before reported, undersold.** The
[THEORY](THEORY.md) explains why; the
[TUTORIAL](TUTORIAL.md) shows the shape; the
[GLOSSARY](GLOSSARY.md) fixes the words.*

# batten-spike — verified-outcome routing inside a toy pass pipeline

**Status: measured, all numbers reproducible.** `cargo test` (13 tests),
`cargo run --release` prints the full sweep. TOY, LABELED throughout:
cell-count cost, relative-size benefit, a fuzz corpus, 5 candidate
pipelines, 4-dim fabric features.

## The question

Can verified pass outcomes act as **battens** (batten-spline's epistemology:
verified anchors, declared fog, interpolated confidence) for routing a new
fabric through candidate pass pipelines — and does the routing earn its
keep vs (a) the exhaustive oracle and (b) a trivial fixed policy?

## Method

1. Corpus: 800 train + 200 test fabrics from `llvm-fabric`'s seeded fuzz
   generator (`gen_fabric`), disjoint seed ranges.
2. Pipelines: `none`, `fold`, `fold>dce`, `dce>fold`, `full`
   (`fold>dce>fold>dce`, the llvm-fabric default).
3. Per (fabric, pipeline): cost = sum of input cell counts per pass run;
   benefit = relative size reduction, zeroed if output fails verify.
   Score = `utility − 0.05 × rel_cost` (λ toy).
4. Battens: one spline per (pipeline × {utility, cost}) keyed by
   standardized features `[ln(cells), arith_frac, const_frac, depth/cells]`.
5. Route: argmax of interpolated score; oracle = argmax of measured score.

## Library vs reimplement — the call

**Reimplemented the minimal kernel** (~40 lines: Nadaraya–Watson + RBF +
fog density). Why not depend on `batten-spline` directly:

- it is a Python/numpy package; this is a zero-dependency Rust crate —
  no path dependency is possible across languages, and shelling out per
  query would dominate the measurement;
- `CascadeRouter` maps ONE confidence to LOCAL/CASCADE/CLOUD; pipeline
  routing needs a per-candidate estimate then argmax — the API shape
  doesn't fit;
- the age-decay half-life is dropped deliberately (static corpus, no
  wall-clock semantics). Everything else matches
  `BattenSpline.estimate_confidence` / `fog_density` semantics, and the
  port is covered by unit tests mirroring the library's behavior.

## llvm-fabric dependency — the call

**Reused via path dependency on a pinned vendored copy** (`_dep/llvm-fabric-vendor`,
a snapshot of quilt-llvm commit `2e5469e`, tests green, 76 passed). The
live `experiments/llvm-fabric` working tree had uncommitted WIP that did
not compile at spike time; the pin keeps this spike reproducible
independent of that lane. Regenerate: `git worktree add <tmp> 2e5469e &&
cp -r <tmp>/experiments/llvm-fabric _dep/llvm-fabric-vendor` (minus
`target/`).

## Headline numbers (WSL2, rustc 1.97.1, release build)

Best fog_scale (0.25): **61.5%** exact-match vs oracle (trivial
always-majority baseline: 54.0%), **26.2% cost saved** vs always-full,
mean score regret **0.018** (scores span ~[−0.2, 0.8]). Full sweep and
failure analysis: `docs/EXPERIMENTS.md` appendix. Verdict there —
undersold on purpose.

# R3 LANE 3 — pass graduation: the three spike passes join the maintained set

*Landed 2026-08-31 on branch `r3-lane3-pass-graduation` (worktree
`quilt-llvm-wt-r3lane3`), based on lane 1's `65cf06b` (rebased off
`72f0337` first — the lane-1 Edit vocabulary is the substrate the
graduated passes record through). House rules: measured or it didn't
happen; failures first-class; undersell.*

**What this lane is.** REGION-SPIKE's R3 mandate, executed: the three
proven passes — (1) const-branch fold, (2) region-DCE, (3) CFG-graft
inline — were spike code: invoked only by `bin/region-spike.rs` with
its private `PassCounts` verdict counters, absent from `src/passes/`
and the pipeline dispatch. This lane graduates them into the
maintained pass set without moving or touching the transforms
(their home stays `region.rs`; lane 1 already put their edit recording
on the full vocabulary).

## 0. Verdicts up front

Baseline = `65cf06b` (lane 1): 200 lib tests + 19 shape-audit, green.
This branch: **204 lib + 19 shape-audit green, 0 red** (`cargo build`
clean; the one pre-existing `is_bred` warning in `bin/region-spike.rs`
predates the lane and is left alone). All four new tests pass, and
every existing test is untouched and green — the lane adds, it does
not modify behavior.

## 1. What shipped

**`passes::verdict`** — the spike's `PassCounts` vocabulary, graduated
into the library: `PassVerdict { attempted, sites, verify_green,
verify_red, interp_preserved, interp_changed, unjudgeable,
replay_identical, replay_diverged }` plus `judge(f_before, f_after,
rec, funcs)`, which runs all three legs on one produced pass
application — verify on the output, the property oracle
(`region::interp`, budget 100_000, spike-fixed so verdicts stay
comparable) before/after, replay bit-identity of the record.
Semantics deliberately mirror the spike bin: a changed answer OR a
decidability flip is `interp_changed` (loudly a bug); a replay error
counts as diverged. `is_clean()` = zero changed answers + zero replay
divergence. Honest scope note in the module doc: `sites` is the
edit-count of the record; per-pass candidate counts stay in each
pass's `Stats` struct.

**Three pass modules**, thin wrappers fixing the pipeline shape
`(fabric, funcs) -> (fabric, DiffRecord)`; `pass_stats` variants keep
the `FoldStats` / `RegionDceStats` / `InlineStats` legs for
measurement:

- `passes::const_branch` (PASS_NAME `const-branch-fold`) — pass A:
  a Branch on a dataflow-constant cond becomes a Jump, phi-join
  maintenance in the dropped arm (the surgery constfold.rs v0 deferred
  to "v1", now named and registered).
- `passes::region_dce` (PASS_NAME `region-dce`) — pass B: unreachable
  regions removed via RemoveCells + RegionRemoved with join stripping
  (the removal `dce.rs` explicitly deferred).
- `passes::cfg_inline` (PASS_NAME `cfg-inline`) — pass C: the
  multi-region whole-CFG graft (`inline.rs` handles the single-region
  case; both remain registered — they are different passes with
  different guards).

**Pipeline registration** (`pipeline::run_named`): three new match
arms — the previously spike-only pass names now dispatchable; and
`PIPELINE_V2 = [constfold, dce, const-branch-fold, region-dce,
cfg-inline, constfold, dce]` with `run_v2(f, funcs)`: the spike passes
slot between the v1 pairs — fold makes arms dead, region-DCE sweeps
them, cfg-inline grafts, the closing pair cleans the graft. `run` /
`run_v1` / their pipelines unchanged.

## 2. Test coverage (per pass, reference numbers)

All tests assert the full triad: verify green after, conservation
(`conserve::population_audit`), and a clean `verdict::judge` (oracle
preserved + replay bit-identical). Plus a pass-specific shape assert:

- `const_branch::tests::green_fold_verifies_conserves_and_judges_clean`
  — const-conditioned diamond (`br %true`); the answer is 1 not 2,
  `FoldStats.folded == 1`.
- `region_dce::tests::green_dce_verifies_conserves_and_judges_clean`
  — unreachable region feeding a live phi; answer 7 preserved,
  2 regions remain, `regions_removed == 1`, `cells_removed == 2`.
- `cfg_inline::tests::green_inline_verifies_conserves_and_judges_clean`
  — main calling add2(20, 22); answer 42 preserved across the graft,
  no Call cells remain, `InlineStats.inlined == 1`.
- `pipeline::v2_tests::v2_pipeline_conserves_verifies_and_replays_every_stage`
  — 7 passes = 8 stages, conserves, verifies, ret folds through the
  graft to const 42, and **every stage replays bit-identically**
  (structural + canonical text) — the graduation criterion, per pass
  and end-to-end.

Counts: 4 new tests → **204 lib green** (+4 over lane 1's 200), 19
shape-audit unchanged.

## 3. Debts named (out of this lane's scope)

1. **`bin/region-spike.rs` still uses its private `PassCounts`** —
   the bin should be rewired onto `passes::verdict` so there is
   exactly one verdict vocabulary. Deferred deliberately: touching
   the spike bin is measurement-history churn, and the bin's counters
   aggregate across a corpus while `judge` is per-application.
   A corpus aggregator (`PassVerdict::merge`) is the natural next
   commit.
2. **`PASS_NAME` consts are documentation, not dispatch** —
   `run_named` matches on string literals (as `inline` already did);
   const-based dispatch is a nicer future shape but a wider refactor
   of all five arms.
3. **`PIPELINE_V2` order is a scout's claim, not a measured
   optimum** — the fold→DCE→inline ordering follows the spike build
   order ("largest diffs ship last"); no differential measurement of
   alternative orders was made.
4. The pre-existing `is_bred` warning in `region-spike.rs:343`
   (lane-1 base) — left for its owner.
5. Lane-1's debts carry forward unchanged: the §3.1 refusal class,
   the corpus drift owned by R3-2, drop_edge's 4 V01 arms.

## Merge notes vs lane 1

Rebased `r3-lane3-pass-graduation` from `72f0337` onto `65cf06b`
before any work — clean rebase, no conflicts. The graduated passes
depend on lane 1's Edit vocabulary (RegionRemoved, MoveCell,
RelabelJoin, RegionAdded) for their replay bit-identity; merging this
branch requires lane 1's branch first (or master must already carry
it). No overlap: this lane touches `src/passes/` + `pipeline.rs`
only; lane 1 touched `region.rs`/`diff.rs`/`replay.rs`/`fuzz.rs`/
`conserve.rs`/`fabric.rs`. The `pipeline.rs` change is additive
(new match arms + `PIPELINE_V2`/`run_v2`); a lane-1-later merge may
need trivial context resolution in `run_named`'s match.

*Verdict: three spike passes are now library citizens with one shared
verdict vocabulary; zero behavior change to any transform; 223 green
and every new stage replays bit-identically.*

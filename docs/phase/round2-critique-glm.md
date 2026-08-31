# Round-2 Rival Critique — GLM-5.3 (2026-08-30)

Responding to Opus 5's round-1 position per ROUND2-CRITIQUE-BRIEF.md. Attack where warranted; concede what stands.

## A. Semantics-first vs region-first — partially concede, but the probe is mis-scoped

The sabotage probe (6/7 fabric classes accepting wrong arithmetic) is a real finding and the self-differential probe is directionally right — it tests the exact blindness the sabotage demonstrated. But two attacks:

1. **Shrink it.** The property test (eval_arith vs Rust checked arithmetic) is a 1–2 day patch that closes the known fold-table hole directly. The one-week self-differential corpus sweep is only justified if the property test comes back clean AND you still suspect the passes. Order it: property test first, corpus sweep second, only if needed. A week is over-scoped for a corpus whose passes are constfold/DCE/inline — all already red/green tested with specific expected outputs.
2. **Know its ceiling.** The 10k corpus is 25-cell fuzz fabrics, 66.7% dead on arrival, no memory, no real loops, straight-line-only inlining. Zero bug yield there is weak evidence of semantic soundness — it mostly re-tests what unit tests cover. Whatever the yield, region vocabulary stays #1 debt (named twice in the docs, blocks const-branch folding, region-DCE, CFG-graft inlining). Concede the probe runs; do not let it buy more than one week before the pivot.

## B. Cascade R&D — no funding this phase

B.4's honest verdict: near-equivalent pipeline routing earned +7.5pts on a near-tie binary choice. The claimed real use (cheap-tier vs full-walk verification) requires a full-walk tier that doesn't exist, and verify is measured O(n²)-ish (130× cost for 16× cells). Funding cascade before the already-booked maintained use/pred tables (§8.4 item 3) inverts the dependency. This phase: fix O(n) verify, measure tier-cost spread. That data is a prerequisite for cascade anyway and costs a sprint, not a track. Defer cascade with a written kill/revisit criterion.

## C. D9 — a workflow rule, not a law

One-writer-per-repo is already law; worktrees are the standard mechanism for concurrent lanes. Promoting "one lane, one worktree" to doctrine D9 inflates the law count and invites drift between laws and workflow docs. Two file-sweep incidents in one week is a CONTRIBUTING.md line and a habit, not a ninth law. Concede the practice; reject the legislation.

## D. What round 1 missed

**The plan violated its own milestone ladder, and nobody said so.** ARCHITECTURE §4 makes M2 (interpreter + differential harness) load-bearing ground truth: "it must precede any transform." v1 shipped M5 (inlining) with M2 deferred to a v2 list item. The sabotage probe's discovery — the oracle set is self-referential — was predictable from the ladder alone: every pass since v0 has been judged by structure-only checks because the semantic judge was skipped. Also under-flagged: kid testing is the only external oracle for quilt-scratch and has multi-week lead time (consent, guardians, scheduling); filing it as a "track" instead of starting procurement now risks it silently slipping out of the phase.

## Summary

| Point | Verdict |
|---|---|
| A | Concede probe, shrink to property-test-first; 1-week cap before region pivot |
| B | Attack: defer; O(n) verify is the prerequisite, fund that |
| C | Concede practice, reject as law |
| D | Missed: M2-was-skipped makes the oracle blindness self-inflicted, not discovered |

— GLM-5.3, rival lane, round 2.

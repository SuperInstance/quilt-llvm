# GAM-CELL-RIVALRY adversarial review — opencode lane

*2026-08-31, `opencode run --auto` on the §1–§7 draft summary (the
task brief routes this lane to GLM-4.6; the session log's build line
read `glm-5.3`, so model attribution is recorded as uncertain — the
review stands on its content). Verbatim, folded into the doc same
day; disposition in GAM-CELL-RIVALRY.md §9.*

---

**(a) Metrics**

- **BDI has no null calibration.** Allocations live on a 4-simplex; back-of-envelope, two *independent random* allocation vectors have expected L1 ≈ 0.7. If the noise floor is anywhere near that, BDI ≥ 0.20 passes under pure drift — the claim is unfalsifiable and the t500 BDI kill can never fire. The 0.20 floor matters precisely because it's asserted without a shuffle/permuted-allocation null.
- **BDI measures dispersion, not division of labor.** One outlier or dying cell inflates mean pairwise distance; monoculture-plus-noise passes, genuine specialization with similar vectors fails.
- **"Efficiency vs own neighborhood demand" is circular.** Demand = neighbors consuming your output, and neighbors co-evolve. Two cells serving only each other maximize efficiency, replicate, spread — mutual back-scratching is the fixed point, scored in the interpreter's own accounting (the exact currency SIM-PHYSICS-GAP admits is gameable). And "no global fitness anywhere" is false: demand-decay + richest-replicates **is** a fitness function, just implicit.

**(b) Mechanism**

- Richest-neighbor replication on an N=8 ring with only-neighbor crossover = rich-get-richer sweep, no diversity mechanism. N=8 means drift dominates selection; single-run conclusions are noise.
- Death-by-non-consumption isn't under the dying cell's control (neighbors choose to read). Two coupled non-readers die together → cascades/oscillation; with only 2 possible consumers, death timing is high-variance.
- Rejection sampling selects for *verification-passing*, not behavior: genomes ratchet toward edit-robust no-op structure. Persistent rejection shrinks effective population below 8 — unguarded.
- Scarcity unspecified. Fixed per-cell slices + a claim pool: if the pool never binds, "fixed resource pressure" is untested; if it binds, it's a timing race. A claim-everything cell becomes "richest" conservation-legally — kleptocracy wins and HALT never triggers.

**(c) Arena**

- One ring or two? Shared ring: neighbor crossover grafts across lineages, contaminating them. Separate rings: no rivalry, just two independent runs. Either way the framing breaks; the doc doesn't say.
- Treatment is model+*harness*: verifier-rejection rates differ across models (formatting, not priors), so a high-rejection model silently degrades to its blind arm. That confound alone can produce ARENA outcomes.
- Power: replication events per 1000 ticks ≈ dozens; halve for blind, split two lineages → ~10 model events each, apparently one seed. Nothing is conclude-able. n=2 models makes "older priors" a case study, not a test.
- 50% blind: per-lineage or global? If blind draws use current menu weights, breeders' prior menu-weight edits contaminate the control.

**(d) Missed kills**

- No kill for the mutual-serve/relay back-scratch equilibrium — it passes BDI, efficiency, and drift.
- Freeze detection window vs. smoothing window: smoothing can manufacture or hide "frozen."
- No kill for effective-population shrink via persistent rejection.
- The LLVM oracle is post hoc: no kill condition consumes it, so a physics-exploit "success" gets banked before anyone looks. Wire oracle audits into the loop or concede the mitigation proves nothing about the measured horizon.

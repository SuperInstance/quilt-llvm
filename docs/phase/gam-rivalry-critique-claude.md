# GAM-CELL-RIVALRY adversarial review — Claude (Sonnet 5) lane

*2026-08-31, `claude -p` on the §1–§7 draft summary. Verbatim, folded
into the doc same day; disposition in GAM-CELL-RIVALRY.md §9.*

---

**Real flaws, ranked by severity:**

**1. Selection mechanism biases against differentiation.**
The "richest neighbor replicates" rule selects for efficiency *in the current local environment*. But you're measuring differentiation—behavioral drift and specialization. These aren't aligned: a cell that evolves to fit its neighborhood's immediate demand perfectly might not *explore* new strategies that a less-efficient neighbor could discover. If mutation rejection is high (not specified), replication becomes rare and drift stalls early. The result could be stable efficiency, not sustainable differentiation. Kill condition RIVALRY-COLLAPSE requires >t500 BDI persistence, but the selection mechanism discards low-efficiency variants that might drive future drift.

**2. "Efficiency vs own neighborhood demand" is circular.**
Richest = work delivered / budget consumed. But "work delivered" is defined locally (consumed by neighbors). So the selection pressure is "replicate if you fit your immediate demand." This is tautological: you're both measuring differentiation and selecting for convergence-to-local-fit. Cells that specialize to their neighborhood *by definition* become harder to dislodge—but that's not differentiation, that's locking in. The BDI metric (pairwise L1 distance on allocation vectors) is orthogonal to this selection rule; they can pull in opposite directions.

**3. BDI threshold (0.20) is unjustified and potentially gameable.**
No null distribution is provided. In an 8-cell ring with 4-d allocations, what's the baseline random BDI? What's the variance in early runs? If cells naturally fluctuate in BDI 0.15–0.25 due to noise, the 0.20 threshold could fire due to a convergence dip, not true monoculture. Also, smoothing window for "smoothed budget-allocation vectors" is unspecified—too-short windows let cells oscillate without behavioral divergence; too-long windows miss drift. Which is fatal here?

**4. Ring topology can't spawn niche demand without asymmetry.**
8 cells, 2 neighbors, symmetric ring. You claim ≥2 spatial demand niches drive differentiation. But if initial conditions and input-demand profiles are symmetric, niches don't emerge—they're imposed. If they're asymmetric by design, then "differentiation emerges under resource pressure" is partly tautological (you pre-structured the landscape). How is asymmetry set up?

**5. Model-Arena leaks, and 50/50 is unjustified.**
You say "no cross-lineage data," but when a model-A cell dies and model-B replicates into its slot, both lineages' neighbors see the transition. Model-A's breeder can't read model-B's genome, but can infer "model-B strategy worked there." This is indirect leakage. Also, 50% blind draws: no justification. If older models produce high-rejection mutations and blind draws have lower rejection, blind draws could dominate later stages, masking the effect you're measuring. Why 50/50 and not 70/30 or 30/70?

**6. Kill condition ARENA-NULL is underpowered.**
"Blind-GA control matches model-bred on BDI/lineage-separation." Blind-GA is baselining; if model-bred has lower variance (fewer replications = smoother dynamics), ARENA-NULL fails easily. But you've given no power analysis. How many runs to reject ARENA-NULL reliably? 40 replication events per lineage is ~2 rounds per slot—that's microscopic. The hypothesis (older models propose viable mutations) could be true but undetectable at this sample size.

**7. Drift-frozen kill (300 ticks) is vague.**
"Frozen" is not operationalized. Within ±0.01? ±0.05? Allocation vectors changing <5%? If cells cycle through allocations deterministically (due to budget reclamation patterns), vectors can stay "noisy" without *behaving* differently. The kill is too soft.

**Most critical:** Fix #1 (selection ≠ differentiation). Consider:
- Separate the fitness function from the differentiation metric.
- Or redefine selection as "replicates if it *solves a neighbor's demand* *that no other cell is solving*"—maximizes niche-fill, not efficiency.

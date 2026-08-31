# Round-2 rival critique (opencode / GLM-5.3, 2026-08-30)

*(Brief said "chat, no files"; foreman directed this to file. Nothing else touched.)*

## A. Semantics-first vs region-first: right theme, wrong probe

Concede the theme. The sabotage probe (6/7 fabric classes accepting corrupted folds) plus the §8.3 precedent (0 phis in 239,691 walks) proves the oracle set can be blind. But the one-week **self-differential is self-referential one level up**: if the minimal evaluator shares any semantics with the pipeline — fold table, `ty_of`, overflow rules — then eval(pre) == eval(post) holds *with the same wrong arithmetic*, and the probe is blind to exactly the bug class that motivates it. If instead the evaluator is independently implemented (Rust checked arithmetic), then the property test IS the oracle and the 10k-corpus sweep only adds compositional coverage — which semantic mutation buys for pennies.

Cheaper, sharper, in order: (1) the eval_arith-vs-checked-arithmetic property test as the **primary** probe, ~1 day, not a side patch; (2) semantic mutation — the corpus already runs 4,430 mutants but every rejection is a structural V-code; flip one fold result per class and measure whether any tier notices. That generalizes the sabotage probe at near-zero cost. Predicted outcome of the week-long version: bug yield ≈ 0 on fold/DCE (constfold already does checked folds), the pivot condition fires, and a week was spent confirming a foregone conclusion. Start region vocabulary (blocks 3 named passes) on day one regardless.

## B. Cascade: no. Fund the precondition instead

B.4's own numbers: +7.5 pts over an always-majority baseline (61.5% vs 54.0%), regret 0.018 because the candidates were near-ties, and fog flagged wrongness only in the regime where accuracy was already worse (46.0–52.0% at fog 1–2). The proposed retry domain — cheap-tier vs full-walk verification — has exactly **one tier today**: full verify, O(n²)-ish, the largest measured pipeline cost (diamonds-160: 1.5 ms verify vs 0.17 ms print). You cannot route between two tiers when one tier exists. This phase: build maintained use tables (§8.4.3 — needed anyway), split the verifier into cheap/full, measure the real cost spread. Only then is a router a question, next phase. Funding the cascade now is a router for an unpaved road; its kill criterion is already met.

## C. D9: workaround, not law

D1–D8 are checkable claims about artifacts. "One lane, one worktree" is operator hygiene; two incidents in one day is not a body of evidence, and the actual remedies used were a vendor pin (@2e5469e) and a gitignore — tooling, not doctrine. Put a pre-flight check (clean `git status`, own worktree) in the lane protocol and stop. Every law added past the load-bearing eight dilutes the eight.

## D. What round 1 missed: the generator is the untested oracle

Both blind-spot incidents share one unfunded root cause: **the corpus generator**. v0's ordering bug meant "10k fabrics, 0 failures" rested on inputs that never contained a phi; the sabotage probe showed the verifier can't catch semantic corruption. Neither proposed theme — semantics probes nor region vocabulary — budgets a shape audit of the 10k corpus (phi / ctrl-edge / call-depth histograms vs the spec surface) or generator red-teaming. Dirt cheap, and it bounds the validity of every other number in the plan. §9.7's independent Sonnet verification lane already exists as a second pair of eyes — point it at the generator next.

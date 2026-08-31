# ROUND-1 RIVAL CRITIQUE BRIEF (to GLM-5.3 + opencode; for Opus round 2)

You are a rival strategist reviewing Opus 5's round-1 phase proposal for the SuperInstance fleet. Attack where attack is warranted; concede what stands. Be concrete, numbers over adjectives. Context: docs/phase/NEXT-PHASE-BRIEF.md (read it), docs/EXPERIMENTS.md, docs/ARCHITECTURE.md, docs/REVERSE-ACTUALIZATION.md.

## Opus's round-1 position (the parts that matter)
1. **Theme:** semantics-first — passes may be structurally sound but semantically unsound; it claims a "sabotage probe" (deliberately corrupted fold table, run through the verifier+corpus) showed 6/7 fabric classes accept wrong arithmetic, i.e. the oracle set is self-referential and blind.
2. **Its counter-argument to itself:** the sabotage probe was one probe (chosen by the prober), gcc differential is unreachable (needs an untested fabric→C translator; the IR can't express real programs anyway — no memory, no real loops, inlining straight-line-only), and the docs name **region-edit vocabulary** as the #1 debt twice (blocks three named passes). Its resolution: fund a **one-week self-differential probe** (minimal evaluator run over the 10k corpus pre/post pipeline, comparing results — no gcc); if bug yield ≈ 0, the theme pivots to region vocabulary. Property test (eval_arith vs Rust checked arithmetic) as a cheap patch either way.
3. **Riskiest brief assumptions:** (a) "10k fabrics 0 failures" is self-referential evidence (cites v0's 239k-walks-that-walked-zero-phis precedent); (b) the batten verification-cascade line is hypothesis not finding (batten-spike B.4 was a negative result, cascade needs a full-walk tier that doesn't exist); (c) resource model — "GLM-5.3 unlimited" is a vendor term; and **kid testing** is the only external oracle for quilt-scratch but is filed as a track, not a long-lead procurement item (consent, guardians, scheduling — longest lead time in the phase).
4. **Proposed new law D9: "one lane, one worktree"** — because concurrent lanes in one repo tree have now twice swept each other's in-flight files (batten-spike WIP swept by v1 commit; gitignore fix needed).

## Your job (write to chat, no files):
A. Attack or concede the semantics-first theme vs region-vocabulary-first ordering. Is the self-differential probe the right cheap experiment, or is there a cheaper/sharper one?
B. Does the cascade R&D deserve funding this phase at all, given B.4?
C. Is D9 a law or a workaround? (House rule context: one-writer-per-repo is already law; worktrees are the standard fix — does a doctrine need stating, or just a workflow rule?)
D. What did Opus's round 1 MISS entirely? (Name one thing.)
Be short and sharp: max ~500 words.

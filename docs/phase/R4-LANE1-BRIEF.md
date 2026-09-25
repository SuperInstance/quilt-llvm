# R4-LANE1 BRIEF — External Differential (T1+T2)

You are R4 LANE 1 of quilt-llvm: build the EXTERNAL DIFFERENTIAL per the approved conception.

Repo: /home/eileen/projects/quilt-llvm, master 3be6ed0 (R3 merged, 226 tests green). Branch: `r4-lane1-external-differential` (cut fresh from master; a prior lane attempt died before any commit — nothing to recover). DeepSeek/DeepInfra REVOKED — do not call.

## Read first
`git show r4-conception:docs/phase/NEXT-PHASE-R4.md` — your lane is fully specified there: T1→T4 tiers, DECIDABLE-ONLY rule, narrowing named in every headline, sabotage corpora harvested from experiments/selfimprove/REJECTS.md if present.

## Mission (T1+T2 this lane)
Lower verify-green quilt fabrics to real LLVM IR (.ll), execute with lli/clang on this machine, compare against the fabric's own interpreter.
- T1: directed cases, bit-exact required.
- T2: scaled volume, fresh seeds.
- The oracle shares NO codebase with the accused — never import fabric internals into the comparison path. That independence is the entire point.
- Any mismatch = a finding with minimal reproducer + honest classification (fabric bug vs lowering bug vs oracle bug — all three are wins per tapestry doctrine).
- `cargo build` + `cargo test` green (226 existing + new). Incremental commits on your branch, no merge/push.

## Report back
Tiers reached, case counts, mismatches found (with reproducers), toolchain issues.

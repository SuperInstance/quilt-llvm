# SEM-MUTANTS — the semantic mutation tier (R1 lane 3)

*Branch `r1-sem-mutants`, 2026-08-30. Base: `040e44f` (NEXT-PHASE
planning tree). Suite at delivery: **123/123 green** (121 baseline +
2 battery-mechanics tests added by this lane). All numbers below were
measured this session in this worktree; reproduction commands in §7.*

---

## 0. What this lane was asked, and what it delivers

NEXT-PHASE §3 R1 lane 3: "The battery today runs 4,430 mutants and
every rejection is a structural V-code. Add mutants that are
structurally valid and semantically wrong (flip a fold result, swap an
arg binding, drop a phi join) and report the kill rate per tier."
Baseline to reproduce: **5/7 fold corruptions survive** (NEXT-PHASE §2).

Delivered, in two tiers because "mutant" means two different things
and conflating them would launder the numbers:

- **Tier S1 — fabric-level mutants** (`src/semmut.rs`, CLI `semmut`):
  2,000 corpus fabrics, each mutated by a semantic operator that keeps
  `verify()` green. Judges: the entire fabric-level battery (verify,
  text round-trip, provenance walks data+control, pipeline, weft
  law/chain, replay bit-identity, conservation).
- **Tier S2 — code-level mutants** (`scripts/sem-mutants.sh`): 20
  one-line, type-correct corruptions of the crate's own source — the
  §2 sabotage set systematized plus extensions. Judges: the cargo test
  suite (J1), and the suite with the ~55-line property-oracle fixture
  (J2) appended test-only during runs.

## 1. Headline table (the number the exit criterion asks for)

| tier | mutants | killed | **kill rate** |
|---|---:|---:|---:|
| T0 structural battery (today's `fuzz`, 10k corpus) | 4,430 | 3,063 | **69.1%** (every rejection a V-code; 1,367 still-valid survivors) |
| S1 fabric-level semantic (input mutants, control excluded) | 1,127 | 0 | **0.0%** |
| S1 tamper control (pipeline-output corruption) | 76 | 76 | **100%** (all by replay bit-identity) |
| S2 code-level semantic, judge J1 = suite | 20 | 5 | **25.0%** |
| S2 code-level semantic, judge J2 = suite + property oracle | 20 | 19 | **95.0%** |

**The one-line reading:** every judge this fleet owns is structural.
Feed it a legal fabric that computes the wrong number and it says OK —
1,127 out of 1,127 times. Corrupt the *pipeline's own output* and it
catches 76/76. The blind spot is precisely input semantics, not
tampering. At the code level, the suite alone kills 5/20; with the
property oracle 19/20 — which **meets the T1 close criterion (≥95%)**
on this battery, with one named exception (§5).

## 2. Tier S1 — structurally valid, semantically wrong fabrics

2,000 iterations, seed 24269, kinds round-robined (250 attempts each).
"judged" = mutant passed `verify()` (structurally valid by definition
of the tier) and was run through the full judge battery. Wrongness is
PROVEN by a ground-truth dataflow evaluator written directly against
Rust checked arithmetic (`semmut::eval_dataflow`) — the same basis as
the property oracle, deliberately not shared with `eval_arith`. Where
the ret value is not dataflow-decidable (params, phis, control flow),
the mutant goes to **unjudgeable** — no execution semantics exists
(NEXT-PHASE §2 residual), and this lane does not build one.

```
kind                   attempt  nosite  str-inval  judged  equiv  sem-wrong  unjudge  killed  kill%
const-off-by-one            250       0          0     250     32          5      213       0    0.0%
arith-sub-div-swap          250      68          0     182     24          3      155       0    0.0%
cmp-ordered-swap            250      53          0     197     30          4      163       0    0.0%
ret-value-swap              250     179          0      71      2         25       44       0    0.0%
branch-target-swap          250     106          0     144      0          0      144       0    0.0%
phi-operand-rebind          250      77         13     160      0          0      160       0    0.0%
join-drop-with-edge         250      98         29     123      0          0      123       0    0.0%
stage-tamper-control        250     174          0      76      0         76        0      76  100.0%
```

Readings, undersold:

- **0 kills on 1,127 input-level semantic mutants.** The judge set —
  verify, round-trip, both provenance walks, pipeline, weft, replay,
  conservation — never fires on a single one. "Still valid: 1367" in
  the T0 run is the same fact seen from the other side: the structural
  battery's survivors include semantic corruptions it has no vocabulary
  to notice.
- **The control fires 76/76, all by replay.** The battery is not deaf:
  corrupt a folded const *after history claims it* and replay
  bit-identity catches it every time. The machinery audits its own
  outputs; input semantics are simply nobody's job. This is the
  measured statement of the §2 residual, now with a denominator.
- **113 mutants are provably wrong (value changed) and survive.** That
  is the honest core number: confirmed-wrong, structurally legal,
  undetected. 1,002 more are structurally valid with semantics no
  existing oracle can even evaluate (control kinds + param-fed
  dataflow).
- **`join-drop-with-edge` exists and verifies.** "Drop a phi join" the
  *consistent* way (Br→Jmp on one arm + join/operand removal in the
  dropped arm's phis) is a legal fabric in 123/250 attempts (29
  attempts land V05/V07-invalid and are excluded). It changes the
  program forever after; nothing can even call it wrong. This is the
  T2 gate's residual class, now with a mutation operator that
  manufactures it on demand.
- **`nosite` is high for ret-value-swap (179/250)** — most regions
  offer no same-typed alternative for ret. Reported, not hidden; it
  bounds how much this kind can say (71 judged).

## 3. Tier S2 — code-level sabotage battery (the §2 set, systematized)

20 mutants (§2's nine + 11 extensions), each a one-line type-correct
source corruption, applied by exact-anchor patch, judged, restored
(clean-tree asserted by the driver). Raw artifact:
`experiments/llvm-fabric/scripts/sem-mutants-results.tsv`.

| mutant | kind | J1 suite | J2 suite+oracle |
|---|---|---|---|
| fold-add-i32-xy1 §2 | fold-result-flip | **KILL** (5 tests) | KILL |
| fold-add-i64-xy1 §2 | fold-result-flip | survive | **KILL** (oracle) |
| fold-sub-i32-swap §2 | operand-swap | survive | **KILL** (oracle) |
| fold-mul-i32-add §2 | fold-result-flip | survive | **KILL** (oracle) |
| fold-div-i32-mul §2 | fold-result-flip | **KILL** (1 test) | KILL |
| fold-cmp-lt-i32-le §2 | fold-result-flip | survive | **KILL** (oracle) |
| fold-cmp-ge-i64-gt §2 | fold-result-flip | survive | **KILL** (oracle) |
| inline-args-reversed §2 | arg-binding-swap | **KILL** (1 test) | KILL |
| decay-liveness-skip-first §2 | liveness-skip | **KILL** (6 tests) | KILL |
| fold-add-f64-xy1 | fold-result-flip | survive | **KILL** (oracle) |
| fold-sub-i64-swap | operand-swap | survive | **KILL** (oracle) |
| fold-mul-i64-add | fold-result-flip | survive | **KILL** (oracle) |
| fold-div-i64-mul | fold-result-flip | survive | **KILL** (oracle) |
| fold-div-f64-mul | fold-result-flip | survive | **KILL** (oracle) |
| fold-cmp-lt-f64-le | fold-result-flip | survive | **KILL** (oracle) |
| fold-cmp-eq-i32-flip | fold-result-flip | survive | **KILL** (oracle) |
| fold-add-i32-offbyone | off-by-one | **KILL** (5 tests) | KILL |
| fold-add-i64-offbyone | off-by-one | survive | **KILL** (oracle) |
| fold-div-i32-offbyone | off-by-one | survive | **KILL** (oracle) |
| **inline-noncomm-swap** | noncomm-reorder | **survive** | **survive** |

Per-kind kill rates:

| kind | n | J1 | J2 |
|---|---:|---:|---:|
| fold-result-flip | 12 | 2/12 | 12/12 |
| off-by-one | 3 | 1/3 | 3/3 |
| operand-swap | 2 | 0/2 | 2/2 |
| arg-binding-swap | 1 | 1/1 | 1/1 |
| liveness-skip | 1 | 1/1 | 1/1 |
| noncomm-reorder | 1 | 0/1 | **0/1** |

Baseline reproduction, exact: **5/7 §2 fold corruptions survive J1**
(killed: add-i32-xy1 by 5 tests; div-i32-mul by 1). With the oracle
fixture: **0/7 survive**. inline-args-reversed killed by exactly one
test (`green_inlines_straight_line_call`), decay-liveness by six —
both as documented in §2.

## 4. What kills what today (measured, not assumed)

- The five J1 kills are carried by **thirteen distinct tests** (union;
  decay-liveness alone by six); the entire i64/f64 fold surface, all
  cmp ops, and sub/mul values are carried by **zero** suite tests. `fold-div-i32-offbyone` surviving J1 is the
  sharpest single fact: the suite's only div test checks that 5/0
  doesn't fold — the *value* of any div that does fold is never
  asserted anywhere.
- The property-oracle fixture kills **14/14 fold-table mutants it
  covers** and, by construction, cannot see transform code: its 14
  marginal kills are all in `constfold.rs`. It is the right tool for
  the fold table and only the fold table.

## 5. Which mutation kinds survive the current judge set

1. **`inline-noncomm-swap` — survives everything (J1 and J2).** The
   inliner, when grafting, reverses the operands of every Sub/Div in
   the callee body ("reorder commutative args under a non-commutative
   op"). No suite test inlines a non-commutative callee — the one test
   that exercises inline at all uses `add2` — and no fold-table oracle
   can reach a transform. **This is a residual class no property
   oracle can kill** (first condition of the T2 gate), and the fix is
   cheap regardless of T2: one red test inlining a sub. Judgment, not
   measured: the same hole almost certainly exists for every transform
   detail exercised only by happy-path fixtures.
2. **All control-flow semantics (S1 kinds branch-target-swap,
   phi-operand-rebind, join-drop-with-edge).** 427 structurally valid
   fabric mutants, zero possible kills, because no execution semantics
   exists to be wrong against. Unjudgeable is not safe — it is
   *unmeasured*.
3. **Input-fabric dataflow semantics (S1 kinds const-off-by-one,
   arith-sub-div-swap, cmp-ordered-swap, ret-value-swap).** 113
   confirmed-wrong survivors. The property oracle (lane 1) does not
   help here either: it audits the fold *table*, not the fabrics fed
   into the pipeline. Judgment: closing this class means either an
   input-value oracle or accepting that fabric semantics are
   out-of-judge — the corpus contract says "valid by construction",
   nothing more.

## 6. T1/T2 bookkeeping (numbers for the gates)

- **T1 kill criterion (≥95% after the property oracle): met on this
  battery** — 19/20 = 95.0% at J2. With the judgment labeled: the 95%
  is concentrated entirely in the fold table; the criterion is met
  *because this battery's fold coverage is dense*, not because
  transforms are safe. §5.1 is the counterexample inside the same run.
- **T2 gate condition 1 ("residual class no property oracle can
  reach"): demonstrated twice** — `inline-noncomm-swap` (code-level)
  and the 427 unjudgeable control mutants (fabric-level). Condition 2
  (R3 ships CFG-graft inlining) remains unsatisfied, so M2 stays
  booked-not-scheduled per NEXT-PHASE §4.
- R2 is unaffected: S1's judge battery re-runs the full pipeline per
  mutant at ~0.1 ms/mutant-class marginal cost (2,000 iters ≈ 0.2 s
  release) — the semantic tier adds no verifier-cost pressure.

## 7. Reproduction

```
cd experiments/llvm-fabric
cargo test                                   # 123/123 green
cargo run --release --bin llvm-fabric -- fuzz          # T0: 4,430 mutants, V-code table
cargo run --release --bin llvm-fabric -- semmut --iters 2000 --seed 24269   # S1 (seed decimal for 0x5EED)
./scripts/sem-mutants.sh                     # S2: 20 mutants × 2 judges, ~2.5 min, restores tree
```

`scripts/sem-mutants-results.tsv` is the committed S2 artifact.
`scripts/sem_oracle_fixture.rs` is the property-oracle *judge fixture*
— lane 1 lands the in-tree version; this copy exists only so the
oracle column could be measured without waiting on the merge. When
lane 1 lands, rerun `./scripts/sem-mutants.sh` and the J2 column
becomes "suite + in-tree oracle" with no other change.

## 8. Caveats, stated plainly

- The J1 judge at battery time includes this lane's 2 new tests
  (battery mechanics only; none of the 20 mutants is killed by them —
  the §2 arithmetic reproduced at 5/7 with them present).
- S1's wrongness oracle (`eval_dataflow`) is a spec-mirror written
  against Rust primitive ops — the same basis as the property oracle.
  It proves mutants wrong; it does not and cannot judge control flow.
- S2 mutant count (20) is small and hand-curated, not exhaustive. The
  §2 nine were carried verbatim; extensions were chosen to cover the
  named kinds (off-by-one, noncommutative reorder) and the type grid
  (i32/i64/f64), not to maximize kill counts in either direction.
- 6.3% "tier total" for S1 including the control row would be
  misleading; §1 reports the control separately on purpose. A control
  that did *not* fire would have invalidated the battery, not flattered
  it.

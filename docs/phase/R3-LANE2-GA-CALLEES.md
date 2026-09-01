# R3 LANE 2 — GA generator: entry-acyclicity preserved, bred callees real

*Branch `r3-lane2-ga-generator-fix`, 2026-08-31. Base: master `72f0337`.
House rules: measured numbers or it didn't happen; failures first-class;
undersell. Reproduction commands in §7.*

## 0. The assignment and the verdict

REGION-SPIKE §3.3 measured that the GA corpus generator's mutation
pressure destroys inline-callee eligibility: **0/140 bred fabrics were
usable as CFG-graft-inline callees** (every bred fabric's entry region
had predecessors), so pass C ran on the 34-eligible GA *seed* corpus
only. NEXT-PHASE §4.4 grew the generator prerequisite accordingly: the
corpus needs "a GA mutation that preserves acyclic entries."

This lane is that fix. Verdict, measured on the default run
(pop 200 × 50 gens, seed 0x6A1C0, 0.29 s release):

| metric | master (before) | this lane |
|---|---|---|
| inline-eligible **bred** callees (verify + acyclic entry + uniform rets) | **0** | **140/200** (100% of the 140 verify-green) |
| final population with entry-acyclic entry | ~0 (ratchet) | **200/200** |
| acyclicity-guard refusals | n/a (no guard) | **0** |
| bred corpus under pipeline + `population_audit` | **13/157 verify-green fabrics fail** (V01 dangling operand, see §4) | **140/140 green** |
| region-spike pass C callee pool | 34 (0 bred) | **174 (140 bred / 34 seed)**; 120/120 programs verify-green, 95 decidable oracle-preserved, 0 changed |
| region-spike pass A fold sites on bred fabric | 7 | 417 (140/140 green, replay 140/140 identical) |
| region-spike pass B dce on bred fabric | **0 green / 157 RED** (§3) | 4 dead regions, 4/4 green |
| drop_edge on bred branch arms | 174/328 legal (53.0%), 4 V01 reds | **830/840 legal (98.8%), 0 reds** |
| reachable C-items (of 8) | 8, by gen 1–4 | 8, by gen 0–1 (max-in-pop 161) |
| `cargo test` (lib) | 181 pass / **6 fail** | **190 pass / 0 fail** (§3) |

Stability (full default runs): seed 46595 → 163 bred callees, 163/163
pipeline-green, verify 78.0%; seed 999331 → 155, 155/155, 80.0%. Default
seed verify pass-rate 71.0% (spike-era: ~79%) — the strictness has a
price, stated in §5; the other two seeds sit at 78–80%.

## 1. Diagnosis — why the ratchet only turned one way

Entry-acyclicity (no ctrl edge targets region 0) is *monotone false*
under the spike's operators: no mutation ever removes a ctrl edge, so
the first edge into entry is permanent — and two operators minted them
eagerly:

- **`mut_grow` v0** drew its back-edge target uniformly over ALL
  regions including entry (`rng.below(regions.len())`).
- **`crossover`** clamped every graft terminator that escaped the graft
  to `RegionId(0)` — a fresh back-edge into entry on every child whose
  graft's CFG left the graft (most of them).

Selection could not help: fitness had no term for entry shape, and the
~17% of gen-0 seeds that start entry-clean (34/200 also pass the full
callee predicate) were polluted within a few generations. **Measured on
master this session:** `region-spike` → "inline-eligible callees
(verify+acyclic entry+uniform rets): 0", callee pool 34 seed-corpus.

A second, subtler pressure: on master, a v0 draw targeting *entry*
usually **survived** selection (entry rarely carries phis, so V16
"predecessor without a join" does not fire), while a v0 draw targeting a
phi-bearing non-entry region died at V16. The survivable variant was
precisely the polluting one — which is also why master's bred population
accumulated 691 unreachable regions (the `ga{N}` corpses of surviving v0
draws) while this lane's measures 58 (32 fabrics; §5 debt 2).

## 2. The fix (four pieces, `ga.rs` unless noted)

1. **Acyclicity-preserving operators.** `mut_grow` v0 back-edges target
   NON-entry regions (`1 + rng.below(n-1)`; single-region fabrics
   self-loop the new region — region+edge growth, the C10 pressure the
   variant exists for, without touching entry). `crossover` clamps
   escaped graft terminators to a lazily-created **sink region**
   (`ga_sink`: a void `Ret` — verify-legal, unreachable, harmless)
   instead of entry. Parent A's entry shape is inherited exactly;
   children of cyclic parents stay cyclic (the guard preserves, it never
   launders).
2. **Rejection-sampling guard** in `run_inner`: a child of an
   entry-clean parent must stay entry-clean — a polluting crossover is
   replaced by the parent clone, a polluting mutation draw is skipped;
   every refusal is counted (`acyclicity_rejections`). With the
   operators above the count is **0 on every seed measured** — the
   counter exists so a future operator that reintroduces the pressure is
   COUNTED, not silent.
3. **Fitness callee-material bonus** (max +5.0 = half a C-item): +2.5
   for an entry-acyclic fabric, +2.5 more for the full fabric-level
   callee predicate. Without selection pressure the clean minority
   drifts extinct (nothing stops a run of bad tournament draws);
   the bonus keeps clean lines alive. It cannot buy an item: a fabric
   covering one more C-item always outranks a callee-bonus fabric.
4. **Reporting**: `RunReport` carries `entry_acyclic`, `bred_callees`
   (final-population counts; the recount is asserted in-test), and the
   guard counter; `ga-corpus` prints them and runs the **conservation
   battery** (`pipeline::run` = per-tick `population_audit`) over every
   verify-green bred fabric — the bred corpus had never been run through
   the pipeline before (§4 found a bug the first time it was).

Plus one repair to the spike's table hygiene (found by §4's battery, see
below): `mut_operand_shuffle` and `mut_consume_phi`'s branch-cond path
wrote operands through raw `cell_mut` writes, bypassing the R2
use-tables contract — they now use the sanctioned `Fabric::retarget`.

`callee_eligible(f)` mirrors the region spike's predicate verbatim
(verify + entry-acyclic + every ret single-value of one common type,
returns the common type); the call-site half (arity, entry-only call,
declared-type match) is program assembly, downstream of the corpus.

## 3. Pre-existing master red, fixed here in a separate commit

The brief said "181 tests passing" at `72f0337`. Measured: **181 pass,
6 FAIL** (lib). All six (`region::tests`: dce green/red ×3, inline
green/red ×2, bred end-to-end ×1) share one signature:
`V06: phi %N join 0 is 'X' but that region never branches to 'Y'`.

Root cause: the R2 use-tables merge made `verify`'s V06 read the
*maintained* pred/succ tables, but `region_remove`, `region_dce`, and
`cfg_graft_inline`/`inline_one` mutate through raw slab/Vec surgery
(region compaction, cell moves, terminator replacement) and never
re-derived them. `drop_edge`/`join_phi`/`const_branch_fold` go through
`replay::apply_edit` (table-maintaining), which is why they were green.
On the pre-merge spike worktree `predecessors()` was a fresh scan, so
the same code passed there — the merge, not the spike, was the
regression. Fix: `g.rebuild_tables()` at the three raw-surgery exits —
the exact escape hatch the use-tables contract documents ("re-derive
after raw surgery"). Three call sites, no behavior change beyond the
tables being true again; all 6 tests green, nothing else moved.
(Lane 1 owns region-edit kinds going forward; this is the minimal
unblock, flagged for their review.)

Net suite: **190 lib tests + 19 shape-audit bin tests green, 0 fail**
(181 pre-existing pass + 6 pre-existing red repaired + 3 new lane tests).

## 4. Found by the new battery: constfold V01 on bred fabric (fixed)

Running the bred corpus through `pipeline::run` — never done before this
lane — failed ~8% of verify-green bred fabrics with
`dce refuses unverified input: V01: dangling operand`.
Control on the OLD GA: 13/157 fail — same class, same rate, i.e.
pre-existing and merely unmeasured.

Root cause (reproduced minimally): the GA's `mut_operand_shuffle` (and
`mut_consume_phi`'s branch-cond variant) wrote operand slots with raw
`cell_mut` writes, so the maintained users table never learned about the
new wire. When constfold later folded the target cell, it retargeted
every user *in the table* — the shuffled wire was not in it — and the
fold's `remove_cell` left a dangling operand. The plain fuzz corpus
never raw-writes, which is why 10k-fabric corpus runs were always green.

Fixed by routing both operators through `Fabric::retarget` (§2). After:
**140/140, 163/163, 155/155** verify-green bred fabrics pass the full
pipeline (per-tick population_audit included) across the three seeds.

## 5. Debts, named

1. **Bred-callee count includes elites.** `bred_callees` counts the
   final population; an unmutated gen-0 survivor carried by elitism is
   indistinguishable from bred stock in that number. At these seeds the
   population is 50 generations deep and every elite slot is contested
   by bred children scoring higher, but the honest phrasing is
   "final-population callees", not "per-lineage bred callees".
2. **Dead-region material collapsed: 691 → 58.** The ratchet's dead
   twin (§1): survivable v0 draws were the entry-polluting ones, and
   they carried `ga{N}` corpses. With entry off the menu, v0 draws into
   phi-bearing regions die at V16, so `region_dce`'s bred-fabric
   material is now thin (4 sites/run vs 191 in the spike era). The seed
   corpus still supplies it (114/200 fabrics). If R3 wants bred
   dead-region material, it needs a v0 variant that targets phi-free
   non-entry regions deliberately — a one-menu-entry change, not done
   here (scope).
3. **Verify pass-rate on the default seed dropped to 71.0%**
   (spike-era ~79%; other seeds 78–80%). The stricter operators kill
   more children at selection. Not hidden; if it matters, the menu
   weights can be retuned — measurement first.
4. **Replay for region-granular edits is still 0** (dce/inline legs of
   region-spike: `replay identical 0/4, 0/120`) — the known
   diff-vocabulary debt (REGION-SPIKE §4.2), lane 1/3 territory,
   unchanged by this lane.
5. **`ga_sink`/`ga{N}` region names can collide** with graft source
   names (both come from `add_region` free-form names). Cosmetic-only
   today (verify does not key on names); a real namespace would want
   unique-ified names.

## 6. Test inventory (3 new, `ga::tests`)

- `callee_eligibility_predicate_semantics` — green/acyclic diamond is
  eligible and earns the bonus; entry-cyclic verifies but is ineligible;
  void ret breaks uniform rets.
- `mutation_and_crossover_preserve_entry_acyclicity` — the core
  invariant directly on the operators: 40 seed fabrics (8+ entry-clean
  guaranteed) × 30 generations × (3 mutation draws + crossover with a
  cyclic partner + crossover within the lineage); every descendant of a
  clean fabric stays clean; at least one verify-green descendant
  required (the fix must not make mutation all-invalid).
- `engine_breeds_callees_and_guard_stays_silent` — engine-level exit at
  test scale (pop 100 × 25 gens): guard refusals == 0, entry-acyclic
  > 0, bred callees > 0, report counts match a manual recount, every
  reported callee verifies with a pred-free entry.

## 7. Reproduction

```
cd experiments/llvm-fabric
cargo test                                   # 190 lib + 19 bin green
cargo run --release --bin ga-corpus            # default: 140 bred callees, 0 guard refusals
cargo run --release --bin ga-corpus --seed 46595   # 163
cargo run --release --bin ga-corpus --seed 999331  # 155
cargo run --release --bin region-spike         # independent measure: callee pool 174 (140 bred)
cargo run --release --bin attack-probe         # A1–A6,A7a caught; A7/A8/A9 within-law (unchanged)
cargo run --release --bin llvm-fabric -- fuzz --iters 10000   # 0 failures, 255,446 cells walked
```

— R3 lane 2, 2026-08-31. The GA is now a callee-material factory; what
it starved is fed.

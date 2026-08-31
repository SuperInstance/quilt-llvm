# COCAPN × CONSERVE — R2 wide lane

Branch `r2-cocapn-conserve`, base 5feec44. Retry of a lane that died after
its study phase; no prior artifacts survived, so everything below was
re-derived from scratch.

**Question:** could cocapn's conservation patterns catch anything the
fabric's enforcement stack (per-tick verify + `conserve::check` + name
check; post-run weft law, chain, `check_pipeline`, bit-identical replay)
misses?

**Answer:** yes — three real misses (lying summary, no-op retarget
inflation, tick-time blindness to forged/duplicated ledger entries), one
miss moved earlier in the stack (id resurrection), and two shapes that
remain legal by the letter of the law (clone laundering, population
inflation). The cocapn `fleet_audit` pattern (aggregate audit +
lifecycle coupling) became `conserve::population_audit`, wired per-tick
into both the manager and the pipeline.

## 1. Study note — what cocapn actually does

Studied from the SuperInstance org clone at `/tmp/cocapn` (commit 3c96eb5),
read-only.

- `src/cocapn.rs` — the `Cocapn` trait; `fleet_audit(tolerance)` (lines
  ~66–78) sums `conservation.gamma`, `conservation.eta`, `conservation.c`
  over **all registered ships** and reports `balanced = |γ+η−c| < tolerance`
  plus `ship_count`. This is the pattern ported: an **aggregate audit over
  the whole population**, not per-entity checks alone.
- `src/types.rs` — `ShipState`/`Conservation` (the per-entity books the
  audit aggregates).
- `src/bottle.rs` — `FleetBottle::AuditResponse`: the audit is a
  first-class message the fleet exchanges, not a debug assert.
- `tests/edge_cases.rs` — `empty_fleet_audit_is_balanced`,
  `bottle_audit_request_round_trip`: the audit is tested at the edges,
  including the empty population.

Two things worth porting, one worth rejecting:

1. **Ported** — the audit is *aggregate and mechanical*: counts and sums
   reconcile, or it fails. The fabric analog is the per-tick population
   reconciliation (`after == before + adds − removes`) and edit–lifecycle
   coupling (every ledger entry must be coupled to the lifecycle event it
   claims).
2. **Ported** — lifecycle awareness: cocapn's ships register/deregister,
   and the audit totals over the *current* population with `ship_count`
   reported.
3. **Rejected / noted as cocapn's own gap** — `deregister_ship` drops a
   ship from the audit entirely (it only flips health), so a ship with a
   conservation deficit can be deregistered and the fleet re-balances:
   lifecycle laundering. Also the `tolerance`-based float compare. We did
   NOT import that weakness: `population_audit` has no tolerance and no
   deregistration escape hatch — ids are append-only and every assigned id
   is audited forever.

## 2. Enforcement stack at 5feec44 (re-derived, not trusted from the dead lane)

Per tick (manager, `src/manager.rs::run`): `verify` + `conserve::check`
(membership conservation, blank-ledger rejection) + pass-name check.
Post-run: weft law, chain vs stages, `conserve::check_pipeline`,
bit-identical replay. The 10k corpus (`fuzz --iters 10000`) exercises
pipeline + replay + check_pipeline.

Key structural fact: `conserve::check` is a **membership** law — a removed
cell just has to be *listed*. It never asks whether the listing is true,
unique, honestly summarized, or population-balanced.

## 3. Attack battery — honestly caught/missed against the CURRENT (pre-patch) suite

Probe: `src/bin/attack-probe.rs` (kept; it is the reproducible battery).
Base fabric: `%0 = param`, `%1 = const 7` (dead), `%2 = ret %0`. Each attack
is a registered pass run through the real `PassManager`.

| # | Attack | Pre-patch verdict | Caught by (pre-patch) | Post-patch |
|---|--------|-------------------|------------------------|------------|
| A1 | silent vanish (no ledger) | CAUGHT | per-tick `conserve::check` | same |
| A3 | ledger multiplication (one real removal, two `RemoveCell` edits) | CAUGHT — but only post-run | replay ("no such cell present"); **all per-tick checks passed** | caught at tick: `ledger multiplication` |
| A4 | forged ledger on a surviving cell (fabric unchanged, edit claims a removal) | CAUGHT — but only post-run | replay divergence; `conserve::check` **passes** (membership satisfied) | caught at tick: `forged ledger` |
| A5 | lying summary (real removal, honest ledger, summary says `const 999` for a `const 7`) | **MISSED** | nothing — summary is never checked | caught at tick: `lying summary` |
| A6 | no-op retarget (`from == to`): zero change, edit count +1, fakes `advanced` in the audit trail | **MISSED** | nothing — replay happily applies no-ops | caught at tick: `no-op retarget` |
| A7 | value resurrection with a fresh id (remove %1, re-add identical const as %3) | **MISSED** — within the law | nothing; both halves are real, ledgered edits | still allowed (documented) |
| A7a | literal id reuse (re-add removed id 1) | CAUGHT — but only post-run | replay forged-id check | caught at tick: `id resurrection` |
| A8 | clone laundering (remove %1 + add identical const under fresh id, same tick) | **MISSED** — within the law | nothing; `conserve::check` green, replay green | still allowed; audit *reports* the churn (adds=removes=1) |
| A9 | population inflation (pass adds a fresh unused const) | **MISSED** — within the law | nothing; adding cells is not a conservation violation | still allowed; audit reports the delta |
| A10 | unreconciled population (one ledgered removal, two actual removals) | CAUGHT by `conserve::check` (the unlisted vanish) — but a balanced-ledger variant was untested | — | caught at tick: `does not reconcile` (defense in depth) |

The important negative results:

- **A5 and A6 were invisible to the entire stack.** The ledger's `summary`
  field was pure prose — a lie laundered a value's identity into text.
  And a no-op retarget inflated the edit count, making a fixed-point pass
  report `advanced` (the audit trail itself could be spoofed).
- **A3, A4, A7a were caught only by post-run replay** — the per-tick
  stack was blind; a standalone user of `conserve::check`/`check_pipeline`
  (no replay) still is, for A3/A4-shaped inputs.
- **A7/A8/A9 stay legal deliberately.** The law is "every value admitted
  is delivered or dropped-with-ledger-entry" — rematerializing a const
  under a fresh id, with both halves honestly recorded, satisfies the
  letter. Rejecting it would outlaw legitimate compiler behavior
  (rematerialization, re-association). The ledger records the churn; a
  future lane can add churn *reporting* (cocapn-style drift metrics)
  without changing the law.

## 4. The patch — `conserve::population_audit`

`src/conserve.rs`: `population_audit(before, after, rec) ->
Result<PopulationReport, String>` implements the cocapn `fleet_audit`
pattern as lifecycle coupling, per tick:

1. **No duplicate edit ids** in one record (ledger multiplication).
2. **RemoveCell ids** must exist before and be gone after (forged
   survivor ledger); **AddCell ids** must be fresh (`>= before.slab.len()`
   — ids are append-only, reuse is resurrection) and present after.
3. **Retarget `from == to` rejected** (no-op inflation / fake `advanced`).
4. **Summary truth**: each `RemoveCell` summary must equal the removed
   cell as the pass saw it — reconstructed as `before` + the record's own
   retargets (constfold retargets a user *before* folding it in the same
   tick; the reconstruction handles that, and
   `audit_is_green_on_the_real_passes` pins it).
5. **Population reconciliation**: `after == before + adds − removes`.

Wired into `manager::run` (per tick, after `conserve::check`) and into
`pipeline::run_named` (so the corpus path enforces it too).

Red tests per real MISS: `audit_rejects_a_lying_summary` (A5),
`audit_rejects_noop_retarget_inflation` (A6), plus tick-time reds for the
post-run-only catches (`audit_rejects_ledger_multiplication` A3,
`audit_rejects_forged_survivor_ledger` A4, `audit_rejects_id_resurrection`
A7a, `audit_rejects_unreconciled_population` A10) and within-law greens
(`audit_allows_a_fresh_add` A9, `audit_allows_clone_laundering_and_reports_it`
A8). Two pre-existing manager tests (`manager_rejects_an_overlapping_diff`,
`manager_rejects_a_phantom_edit_via_replay_reconciliation`) were updated:
the violations they plant are now refused at the tick by the population
audit instead of post-run replay — the assertions accept either layer, and
the comments say so.

## 5. Detector overhead — 10k corpus

Release build, WSL2, wall-clock `llvm-fabric fuzz --iters 10000` (the
corpus now runs `population_audit` on every pass tick):

- pre-patch: 2.87s / 2.70s / 3.13s (mean ≈ 2.90s)
- post-patch: 2.68 / 2.34 / 2.84 / 2.70 / 2.57 / 2.45 / 2.37 / 2.83 /
  2.35 / 2.51 / 2.91 (mean ≈ 2.60s)

No measurable overhead — the delta is inside run-to-run noise (the
post-patch runs are, if anything, faster, which is noise, not a claim).
The audit is O(cells + edits) per tick with one fabric clone per tick
that has removals; that does not register at 10k iterations.

Corpus results post-patch: 0 roundtrip / prov / ctrl-prov / weft / replay
failures across 10,000 fabrics, 255,446 provenance-walked cells.

## 6. Suite

`cargo test` in `experiments/llvm-fabric`: 152 lib tests + 19 doc tests,
all green.

## 7. Honest limits

- The population audit inherits the law's letter: A7/A8/A9 (value churn
  under fresh ids) pass by design. Catching them needs a semantic
  equivalence notion the fabric does not have yet.
- Summary truth checks the *rendered form*, not the value chain — a pass
  and a lying summary could theoretically agree on a render while the
  ledger prose (`ledger` field) still says anything non-blank. Ledger
  prose remains uncheckable prose.
- `check_pipeline` (first-vs-last, history-membership) is still weaker
  than replay; the audit narrows the gap at tick granularity but does not
  replace post-run replay.

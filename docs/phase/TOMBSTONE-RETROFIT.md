# TOMBSTONE-RETROFIT — M4.1, the tit-quilt forget law on the decay ledger

Branch `r1-tombstone` (side lane R1, from cross-pollination finding #2 in
`docs/CROSS-POLLINATION.md`, 2026-08-30). One lane, one worktree:
`/home/eileen/projects/quilt-llvm-wt-tombstone`.

## The transfer, in one paragraph

tit-quilt (SuperInstance/tit-quilt, `tit_quilt/cells.py`) shipped the exact
law M4's decay wanted: **the provenance-integrity law — nothing
witness-referenced is ever destroyed; `FORGET` never deletes, it
tombstones** (cell identity, version, witness; the value dropped and
replaced by its hash; append-only store; idempotent re-forget). This lane
retrofits that law onto M4's death certificates: a decay death is now a
**FORGET** whose certificate carries the full tombstone body — content
hash, kind tag, witness list (the cell's operands at forget time) — the
fabric keeps an append-only graveyard, the Weft diff ledger records the
forget without deletion, replay rebuilds the graveyard from the ledger
line alone, and provenance walks resolve *through* tombstones instead of
dead-ending. A forged FORGET — a complete, internally-consistent
tombstone for a LIVE cell — is rejected exactly like a forged death.

## What transferred cleanly (law → law)

| tit-quilt (`cells.py`, `engine.py`) | quilt-llvm M4.1 (`src/decay.rs`, …) |
|---|---|
| `Tombstone{cell_id, kind, version, value_hash, witness, …}` | `decay::Tombstone{cell, kind, tick, vhash, witness, killer}` — plus a killer (our certificates audit *who* forgot) |
| `forget()`: drop value, keep hash + identity + witness; idempotent; no delete path | `Fabric::forget(&DeathCert)`: refuses a cert that does not describe the cell (hash + witness mismatch), idempotent re-forget (the first record stands), appends to `Fabric::tombstones`, removes from slab; there is no API that deletes a tombstone |
| tombstones.json: separate append-only file | `Fabric::tombstones`: append-only Vec on the fabric; the *carrier* is the Weft diff ledger line — `death{… vhash=0x… kind=… wit=%3,%0}` — and `replay::apply_edit` rebuilds the graveyard bit-identically from the ledger alone |
| `witness_closure` walks live cells then tombstones | `prov::provenance` walks live cells then tombstones: a forgotten cell renders as `%4 = arith <forgotten tick=1 vhash=0x…>` and the walk follows the witness list to roots (`check_prov` accepts tombstoned roots) |
| value dropped, replaced by hash | `vhash = fnv1a64(render_cell)` — the cell's canonical text hash; the ledger's `summary` keeps the human form, the tombstone keeps only the machine identity |

The tombstone-witness fit was *tighter* than the survey's "~30-line
retrofit" guess suggested, because M4 had already built the receiving
socket: certificates were already machine-parsed ledger lines, already
recomputed against the pre-tick fabric, already tamper-tested. The
tombstone body is three more recomputed fields in the same discipline.
Measured in diff-of-code: +747/−52 lines across six files (about half
of that tests); the survey's ~30-line estimate covers `Tombstone` +
`forget()` alone — the rest is enforcement (verifier codes, replay
rebuild, provenance walk) the law does not survive without.

## What did NOT transfer (and why — the honest ledger)

1. **Tombstones are not inside the Weft hash chain.** `sign::fabric_sig`
   hashes `text::print`, which renders the live fabric only. A tampered
   tombstone store does not break `verify_chain`. Mitigation (same
   defense class M4 already uses): tombstone integrity is enforced by
   *recomputation*, not by chain — `verify_deaths` re-derives vhash,
   kind, witness, users, and liveness from the pre-tick fabric, and V18/V19
   (below) police the store's shape. Chaining the graveyard into the
   Weft tick signature is a real follow-up (it would also close the
   pre-existing gap that diff *records* themselves are not chained).
2. **No graveyard-wide witness-closure law.** tit-quilt can require every
   tombstone witness to resolve (live or tombstoned) because *all*
   forgetting goes through `forget()`. quilt-llvm's ledger also carries
   non-tombstone removals — constfold folds, the old `dce` pass — so a
   hard verifier law would false-positive on legitimate mixed pipelines
   (a tick-2 fold can orphan a tick-1 tombstone's witness). Deferred;
   the forget-time check (witnesses are operands of a present cell) and
   the provenance walk's live-then-graveyard resolution carry the
   practical load.
3. **No versioned re-forget.** tit-quilt's cells re-derive and version-
   bump; quilt fabrics are SSA — one id, one definition, ids never
   reused (N4). `Tombstone.tick` is the death tick (tit's `version`),
   and idempotence means the first record stands forever.
4. **No `witness[]` on every value return.** tit-quilt witnesses every
   value as it flows; we witness only at forget time (the def-provenance
   operands). M4's `no-demand` witness (the demand-closure measurement)
   remains the *death* witness; the tombstone witness list is the
   *provenance* witness. Two witness notions, one record — the naming in
   the ledger line keeps them apart (`witness=no-demand` vs `wit=…`).
5. **Cold/downgrade lanes (`cold_downgrade`) have no analog yet** —
   quilt cells carry no hot/cold state. That's M5+ material (use-count
   aging), not this lane.

## Enforcement (all new, all mechanical)

- `verify_deaths` (M4 check, extended): the tombstone body recomputes —
  vhash over the pre-tick canonical text, kind tag, witness list — or
  the forget is rejected with the mismatch. The LIVE-cell rejection is
  unchanged and now proven stronger: it fires *even when every tombstone
  field is correct* (`red_forged_forget_of_a_live_cell_is_rejected_like_forged_deaths`,
  and the runnable proof `examples/bogus_kill.rs`, upgraded to forge a
  complete measured tombstone).
- Verifier codes (verify.rs): **V18** — a tombstone for a cell still in
  the slab is a forged FORGET (forgetting removes; it never deletes).
  **V19** — duplicate tombstones are a forged graveyard (forget is
  idempotent; one cell, one tombstone). Every managed tick verifies
  these; empty graveyards (all pre-M4.1 fabrics) are unaffected.
- Replay (replay.rs): a parseable `death{…}` ledger line lands its
  tombstone on apply — the manager's bit-identity check now covers the
  graveyard end-to-end (`green_replay_rebuilds_the_graveyard_bit_identically`).
- Conservation and the progress law are untouched: a forget is still a
  ledgered RemoveCell; tombstones add a second, undeletable record of it.

## Red/green per feature

House naming: `red_` tests prove the negative (tamper rejected), `green_`
prove the positive. Each red was additionally proven *load-bearing*: with
the M4.1 checks surgically removed (vhash/kind/witness recomputation and
V18/V19), exactly the five new red tests fail (128 pass / 5 fail); with
the checks restored, 133/133 pass.

| feature | tests |
|---|---|
| F1 certificate carries the tombstone body (hash, kind, witness) | `certificate_render_parse_round_trips` (extended: pre-retrofit forms no longer parse), `green_decay_kill_carries_a_certificate_that_verifies` (extended) |
| F2 `forget()` law: tombstone-not-delete, idempotent, anti-mismap | `green_forget_tombstones_never_deletes_and_is_idempotent`, `red_forget_rejects_a_certificate_that_does_not_match_the_cell` |
| F3 graveyard laws V18/V19 | `red_v18_tombstone_for_a_present_cell_is_a_forged_forget`, `red_v19_duplicate_tombstones_are_a_forged_graveyard`, `green_an_empty_graveyard_changes_nothing` |
| F4 forged-FORGET rejection (the headline) | `red_forged_forget_of_a_live_cell_is_rejected_like_forged_deaths`, `red_forged_tombstone_hash_is_rejected`, `red_forged_witness_list_is_rejected`, `red_forged_kind_tag_is_rejected`, `examples/bogus_kill.rs` |
| F5 provenance through tombstones | `green_provenance_of_a_forgotten_value_resolves_through_tombstones`, `green_certificates_and_graveyard_agree` |
| F6 replay/manager integration + corpus reconciliation | `green_replay_rebuilds_the_graveyard_bit_identically`, `curve_invariants_hold_on_a_small_corpus` (extended: tombstones == certified kills over the 200-fabric corpus) |

## Measured ledger overhead (vs the 2.3 KB/fabric baseline)

Same corpus, same seeds, same pipeline (`cargo run --release -- decay-curve`,
10,000 fabrics, seed 0xD3CA5; baseline re-measured at 040e44f in a clean
clone — it reproduces EXPERIMENTS.md §9 exactly):

| | baseline (040e44f) | retrofit (r1-tombstone) | delta |
|---|---|---|---|
| ledger bytes / fabric (mean) | 2,351.0 B | 3,137.7 B | **+786.6 B (+33.4%)** |
| certified decay kills | 184,433 | 184,433 | 0 (semantics unchanged) |
| tombstones recorded | — | 184,433 | == kills (reconciles) |
| bogus deaths | 0 | 0 | — |
| **cost per forget** | — | — | **+42.7 B** (`vhash=0x… kind=… wit=…`) |

One-line interpretation: the forget law costs ~43 bytes per death — the
hex hash (22), the kind tag (~7), and the witness list (~2 per operand,
`-` for roots; the corpus's dead cells are mostly const-like). Against
the 2.3 KB/fabric ledger baseline that is a one-third growth of the
ledger for a graveyard that never forgets. EXPERIMENTS.md §9 already
booked history-compaction as future work; tombstones make it more
urgent, and the `value dropped, hash kept` discipline is what keeps the
per-death cost O(1) in cell size rather than O(cell).

## Tests

- before (branch point 040e44f): **121 passed, 0 failed**
- after (r1-tombstone): **133 passed, 0 failed** (+12; 10 new + 2 extended
  families), `cargo test` in `experiments/llvm-fabric`
- runnable proof: `cargo run --release -- example bogus_kill` →
  `death rejected: bogus kill of %1 — the cell is LIVE; demand chain
  %1 -> %2 -> %3 holds it`, exit 0.

## Files touched

- `src/decay.rs` — Tombstone, kind_tag, DeathCert{vhash,kind,witness} +
  measure(), forget-integrated dce_decay, extended verify_deaths, curves
  count tombstones; tests
- `src/fabric.rs` — `tombstones` field, `tombstone_of`, `forget()`
- `src/verify.rs` — V18/V19 + tests
- `src/replay.rs` — graveyard rebuild from ledger lines
- `src/prov.rs` — walk through tombstones, check_prov update; tests
- `examples/bogus_kill.rs` — forged-FORGET proof upgraded to a complete
  measured tombstone

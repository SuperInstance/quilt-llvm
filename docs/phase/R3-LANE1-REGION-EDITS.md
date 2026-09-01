# R3 LANE 1 — region-granular edit kinds + the DCE replay gap

*Landed 2026-08-31 on branch `r3-lane1-region-edit-kinds` (worktree
`quilt-llvm-wt-r3lane1`), branch point `72f0337`. House rules:
measured or it didn't happen; failures first-class; undersell.*

**What this lane is.** REGION-SPIKE §4.2's named debt, executed: the
R2 spike proved region compaction, cell moves, and join relabels have
NO Edit kind, so region-DCE replayed **0/140** and inline **0/34**
(inexpressible, not divergent — §3.4). This lane adds the kinds,
rewires every region op onto them through `replay::apply_edit`, and —
because that deleted the ops' raw-mutation paths — also repairs what
the 72f0337 merge had silently broken: **six red master tests** whose
root cause was region edits desyncing the R2 use tables that verify's
V06/V16 read.

## 0. Verdicts up front

Baseline = pristine `72f0337`, this branch, both `region-spike`
(release, same machine, same session), same seeds — the corpus is
bit-identical across the two (157 bred fabrics, deterministic), so
every delta below is the lane's, not the GA's.

| pass | master `72f0337` | this lane |
|---|---|---|
| const-branch fold | 157/157 green, replay 157/157 | unchanged (untouched path) |
| **region-DCE** | **0/157 green (ALL red: table desync)**, replay 0 | **155/157 green** (2 = the named §3.1 refusal class), **replay identical 155/155** |
| **CFG-graft inline** | 24/34 green (10 red: desync), replay 0/24 | **34/34 green, replay identical 34/34** |
| interp changed answers, any leg | 0 | **0** |

The DCE replay gap — the lane's named deliverable, previously
0/140 on the spike corpus — is **closed**: 155/155 bit-identical on
today's corpus, plus the red-condition test proving the gap is the
edit kind's to close (the same record minus its RegionRemoved edit
fails to reproduce). Inline replay closes as a consequence of the
vocabulary being complete (RegionAdded was needed for MoveCell to have
a destination; the spike's §4.2 list did not name it — it is the
smallest supersede of that list).

**Corpus drift, named honestly:** the spike's published material
(140 bred, 142 const-branch sites, 564 arms, 99.3% phi-join
verify-legal, 0 red) is NOT reproducible on the merged master —
`72f0337`'s ga restoration breeds a different corpus (157 verify-green,
**7** const-branch sites, **328** arms, 53.0% verify-legal, 150
single-join refusals, 4 V01-refused arms). Verified byte-identical on
pristine master and on this branch: the drift predates the lane and
belongs to the GA lane (R3-2). The phi-join *operator* (drop_edge /
strip_src_joins) is untouched by this lane — diff vs `72f0337` is
empty on those functions.

## 1. What shipped

**The vocabulary (`diff::Edit`)** — REGION-SPIKE §4.2, plus RegionAdded:

- `RegionAdded { id, name }` — append-only region ids
  (`id == regions.len()` at apply time; the AddCell slab law, applied
  to regions).
- `RegionRemoved { id, name }` — region must be EMPTY at apply time
  (cells ride their own RemoveCell edits); ids above compact down one,
  every surviving reference remapped (cell.region, Br/Jmp targets,
  phi joins). `name` is validated against the fabric — the
  anti-forgery twin of Retarget's `from`.
- `MoveCell { id, from, to, index }` — id-stable region move; `from`
  validated like Retarget's.
- `RelabelJoin { phi, from, to }` — one phi join label moves; operands
  and use wires untouched (phi joins are not use edges).

**Sanctioned mutators (`Fabric`)**, maintaining the R2 tables:
`remove_region` (succ/pred rows leave with the region; ids inside
surviving rows shift; users rows untouched — cell ids never move),
`move_cell` (succ row recomputed when a terminator leaves/lands a
region's last position), `relabel_join` (labels only — V06/V16 read
at verify time). The R2 derivability law holds through all of them
(`fabric::tests::region_mutators`, plus derive==maintained asserted
on every replayed stage of every new test).

**Every region op now records and APPLIES real edits**
(`replay::apply_edit` per edit — bit-identity by construction):
`region_add`/`region_graft` (RegionAdded + AddCells),
`region_remove` (RemoveCells + RegionRemoved), `region_dce`
(per-dead-region interleaved removals in ascending order, a shift
counter for post-compaction ids — the batch raw-remap block deleted),
`inline_one` (RegionAdded for the continuation and every grafted
region; MoveCell per moved post-call cell and the old terminator,
ids stable; RelabelJoin per relabeled phi; step-5 grafting plans
fresh ids on paper — no slab reservation — so every AddCell satisfies
apply_edit's next-free-id law).

**`conserve::population_audit` understands the kinds.** Population-
neutral arms in the ledger check; the summary-reconstruction walk is
now a single in-order pass that renders each RemoveCell at its stream
position and applies AddCell/region edits through `apply_edit` —
strictly more faithful than the old before+all-retargets, and what
lets a RegionRemoved find its region emptied of already-ledgered
cells. Region records pass `check` + `population_audit` (asserted).

**The corpus law extended (fuzz.rs).** `corpus_run` now runs
`region_dce` on every generated fabric: tables must stay derivable on
the result and every replayed stage, replay must reproduce
bit-identically, conservation must hold. The §3.1 refusal class is
counted (`region_dce_refused`), never failed; anything else errs
naming the seed. The 10k-corpus test carries it; the 400-iter test
pins `green + refused == valid`.

## 2. Suite counts

Master `72f0337`: **181 green + 6 RED** (the desync; "181 tests
passing" was the merge's belief — `region_dce`/`region_remove`/
`inline_one` results verified against stale pred rows on every
invocation). This branch: **200 green, 0 red** (`cargo test`, full
run; +11 vocabulary tests in commit 1, +2 region-pass tests and
several extended asserts in commit 2; the 19 shape-audit measurer
tests unchanged and green). Every previously-red test passes for the
honest reason: the raw paths that desynced are deleted, not patched
around.

## 3. Measured (release, this session; corpus identical to master's)

DCE throughput ~9.9k pass invocations/s, **42.9k region
removals/s** (spike: ~5.6k — the use tables plus edit-path placement).
Inline ~1.1k invocations/s (spike ~730). Replay costs nothing
observable at corpus scale. Raw ops: region_add ~168k/s,
region_remove ~336k/s incl. refusals.

## 4. Debts named (out of this lane's scope)

1. **The §3.1 refusal class stands**, now 2/157 bred + 4/114
   seed-corpus dce fabrics (spike measured 4/140 + 4/114): a live
   phi's only join on the dying region with a non-const cross-region
   operand. Needs value duplication or fold→DCE composition (spike
   §4.3).
2. **Corpus drift is the GA lane's**: const-branch material collapsed
   (142→7 sites) and drop_edge's verify-legal rate fell (99.3%→53.0%,
   with 4 V01-refused arms) on the post-merge corpus. The operator is
   unchanged (empty diff on drop_edge/strip_src_joins vs master);
   the material changed under it. R3-2 owns the generator.
3. **drop_edge's 4 V01 arms** (refused loudly, never silent) — a
   sub-case of the §3.1 class surfacing as verify failures instead of
   the named refusal; worth folding into strip's guard diagnosis when
   the GA material returns.
4. **`region_graft` measured 0 closed grafts** on this corpus (the
   bench's donor operands are never closed) — pre-existing bench
   limitation, kept honest.

*Verdict: the vocabulary extension closed the DCE replay gap (0/140 →
155/155) and, as a consequence of doing it without raw mutation,
restored region-DCE and inline to green on a corpus where the merged
master had them 0/157 and 24/34. Zero changed answers anywhere the
oracle can see.*

//! R2 region-edit spike — the R3 gate-decider (GATE-W2 §4, NEXT-PHASE §6).
//!
//! The question this module answers, with measurements: can the three
//! blocked passes' CORE SURGERY (const-branch fold, region-DCE,
//! CFG-graft inline) be expressed as verify-legal edits on real bred
//! fabrics, reusing `semmut`'s join-drop-with-edge operator rather
//! than reimplementing it?
//!
//! API surface (spike-grade; ugly is allowed, legal is not optional):
//!
//! * [`region_add`] — add an empty region (caller must populate it
//!   with a terminator before `verify` will pass; V03).
//! * [`region_remove`] — remove ONE region: refuses if anything
//!   reachable-from-entry still points at it; compacts region ids and
//!   remaps every reference. Direct mutation + recorded note: NOT
//!   expressible in the current `Edit` vocabulary (the §6 finding).
//! * [`drop_edge`] — Br→Jmp on one arm plus phi-join/operand
//!   maintenance in the dropped arm. This IS the semmut
//!   join-drop-with-edge operator, factored out, made
//!   semantics-preserving, and strengthened: a phi that would drop to
//!   ZERO joins (V05, semmut's dominant failure) is collapsed to its
//!   last operand (retarget users, remove the phi) when V12 allows,
//!   else kept as a legal one-join mux. Fully expressible as
//!   AddCell/RemoveCell/Retarget edits → replayable bit-identically.
//! * [`join_phi`] — add a join+operand entry to a phi when a new
//!   control edge appears (the inverse maintenance).
//! * [`region_graft`] — copy a region from a donor fabric into this
//!   one with id/region remapping (the CFG-inline primitive).
//!
//! The three passes built on the vocabulary:
//!
//! * [`const_branch_fold`] — Br with dataflow-const cond → Jmp on the
//!   taken arm via drop_edge (the pass constfold.rs explicitly
//!   deferred: "folding a const branch into a jump requires phi-join
//!   maintenance").
//! * [`region_dce`] — remove every region unreachable from entry,
//!   with phi-join maintenance for live phis that joined on the
//!   removed regions (the pass dce.rs explicitly deferred).
//! * [`cfg_graft_inline`] — inline a MULTI-REGION callee into the
//!   caller's CFG (the pass passes/inline.rs explicitly defers to
//!   exactly this vocabulary).
//!
//! Property oracle: [`interp`] is a spike-grade concrete interpreter
//! for decidable fabrics (every branch cond on the executed path a
//! dataflow const; params unjudgeable; calls resolved through a
//! program). "Semantics preserved" = interp answer unchanged. Fabrics
//! whose answer is None are UNJUDGEABLE — counted, never assumed.

use crate::cell::{Cell, CellKind};
use crate::diff::{DiffRecord, Edit};
use crate::fabric::Fabric;
use crate::id::{CellId, RegionId};
use crate::replay;
use crate::semmut::eval_dataflow;
use crate::ty::{ConstVal, Type};
use crate::verify::verify;
use std::collections::{BTreeMap, BTreeSet};

// ======================================================================
// Raw ops — the region-edit vocabulary
// ======================================================================

/// Add an empty region. NOTE: an empty region violates V03 — the
/// caller populates it (cells + terminator) before verifying. Edits:
/// none expressible (no RegionAdded kind); a note is recorded.
pub fn region_add(f: &Fabric, name: &str) -> (Fabric, RegionId, DiffRecord) {
    let mut g = f.clone();
    let r = g.add_region(name);
    let mut rec = DiffRecord::new("region-add");
    rec.notes.push(format!(
        "region '{}' added as region {} — NOT expressible in the Edit vocabulary (no RegionAdded kind); recorded as a note",
        name, r.0
    ));
    (g, r, rec)
}

/// Regions reachable from entry over terminator edges.
pub fn reachable_regions(f: &Fabric) -> BTreeSet<u32> {
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let entry = match f.entry() {
        Some(e) => e,
        None => return seen,
    };
    let mut work = vec![entry];
    seen.insert(entry.0);
    while let Some(r) = work.pop() {
        for s in f.successors(r) {
            if seen.insert(s.0) {
                work.push(s);
            }
        }
    }
    seen
}

/// Remove one region `r`: its cells vanish (RemoveCell edits with
/// ledger entries), the region leaves the Vec, and every region id
/// AFTER it is compacted down by one with all references remapped
/// (cell.region, Jump/Branch targets, phi joins).
///
/// Refuses (Err) when the removal could not be verify-legal:
/// * `r` is the entry region;
/// * any OTHER region's terminator targets `r`;
/// * any phi outside `r` joins on `r` (strip those joins first —
///   see `region_dce`, which does exactly that).
///
/// The id compaction + remap is direct mutation: the current Edit
/// vocabulary has no RegionRemoved/region-renumber kinds. The record
/// carries RemoveCell edits (replayable cell-level history) plus a
/// note naming the inexpressible part.
pub fn region_remove(f: &Fabric, r: RegionId) -> Result<(Fabric, DiffRecord), String> {
    let entry = f.entry().ok_or("region_remove: fabric has no regions")?;
    if r == entry {
        return Err("region_remove: refusing to remove the entry region".into());
    }
    if f.region(r).is_none() {
        return Err(format!("region_remove: no such region {}", r));
    }
    for (i, _) in f.regions.iter().enumerate() {
        let from = RegionId(i as u32);
        if from == r {
            continue;
        }
        for s in f.successors(from) {
            if s == r {
                return Err(format!(
                    "region_remove: '{}' still targets '{}' — strip the edge first",
                    f.region_name(from),
                    f.region_name(r)
                ));
            }
        }
    }
    for id in f.cells() {
        if let Some(c) = f.cell(id) {
            if c.region != r {
                if let CellKind::Phi { joins } = &c.kind {
                    if joins.contains(&r) {
                        return Err(format!(
                            "region_remove: phi {} outside '{}' joins on it — strip joins first",
                            id, r
                        ));
                    }
                }
            }
        }
    }

    let mut g = f.clone();
    let mut rec = DiffRecord::new("region-remove");

    // 1. remove the region's cells, with ledger entries (expressible)
    let cell_ids: Vec<CellId> = g.region(r).map(|x| x.cells.clone()).unwrap_or_default();
    for id in cell_ids {
        let summary = crate::text::render_cell(&g, id);
        let region = g.cell(id).expect("present").region;
        let cells = &mut g.regions[region.0 as usize].cells;
        let pos = cells.iter().position(|&c| c == id).expect("listed");
        cells.remove(pos);
        g.slab[id.0 as usize] = None;
        rec.edits.push(Edit::RemoveCell {
            id,
            ledger: format!("region-remove: '{}' unreachable, all cells dropped with it", f.region_name(r)),
            summary,
        });
    }

    // 2. compact the region Vec + remap every surviving reference
    //    (direct mutation — inexpressible in the Edit vocabulary)
    g.regions.remove(r.0 as usize);
    let remap = |x: &mut RegionId| {
        if x.0 > r.0 {
            x.0 -= 1;
        }
    };
    for c in g.slab.iter_mut().flatten() {
        remap(&mut c.region);
        match &mut c.kind {
            CellKind::Branch { then_r, else_r } => {
                remap(then_r);
                remap(else_r);
            }
            CellKind::Jump { target } => remap(target),
            CellKind::Phi { joins } => {
                for j in joins.iter_mut() {
                    remap(j);
                }
            }
            _ => {}
        }
    }
    rec.notes.push(format!(
        "region '{}' removed and ids {}.. compacted — NOT expressible in the Edit vocabulary (no RegionRemoved kind); recorded as a note",
        f.region_name(r),
        r.0 + 1
    ));
    Ok((g, rec))
}

/// Strip the `src` join (and its operand) from every phi in `dead_arm`,
/// replacing each such phi in place. Strategy per phi:
/// * joins > 1: keep the phi minus the src join (a smaller mux).
/// * joins == 1 (the stripped join is its only one — semmut's V05/V06
///   killer): collapse the phi to its single operand (retarget users,
///   remove the phi) when every user may legally use that operand
///   under V12; else MATERIALIZE the operand as a const in the phi's
///   region when it is a const; else refuse (Err). Keeping the stale
///   join is never an option (V06).
///
/// All changes are expressed as RemoveCell/AddCell/Retarget edits and
/// applied through `replay::apply_edit`, so the edit list reproduces
/// the fabric bit-identically by construction.
fn strip_src_joins(
    g: &mut Fabric,
    rec: &mut DiffRecord,
    src: RegionId,
    dead_arm: RegionId,
    ledger: &str,
) -> Result<(), String> {
    let phi_ids: Vec<CellId> = g
        .region(dead_arm)
        .map(|x| x.cells.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|&id| {
            matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Phi { joins }) if joins.contains(&src))
        })
        .collect();
    for phi in phi_ids {
        let (index, joins, operands, region) = match g.cell(phi) {
            Some(c) => match &c.kind {
                CellKind::Phi { joins } => (
                    g.index_in_region(phi).expect("listed"),
                    joins.clone(),
                    c.operands.clone(),
                    c.region,
                ),
                _ => unreachable!("filtered above"),
            },
            None => continue,
        };
        let pos = joins.iter().position(|&j| j == src).expect("filtered");
        let users: Vec<(CellId, u32)> = g.uses_of(phi);
        let summary = crate::text::render_cell(g, phi);

        if joins.len() == 1 {
            // collapse candidate: every user must be able to use
            // `operands[0]` (V12: same-region-earlier, or operand in entry)
            let op = operands[0];
            let op_region = g.cell(op).map(|c| c.region);
            let op_in_entry = op_region == Some(RegionId(0));
            let collapse_ok = users.iter().all(|&(u, slot)| {
                let uc = match g.cell(u) {
                    Some(c) => c,
                    None => return false,
                };
                match &uc.kind {
                    // phi user: V07 governs — the operand must be
                    // defined in THIS SLOT'S join region (or entry)
                    CellKind::Phi { joins } => {
                        let j = match joins.get(slot as usize) {
                            Some(&j) => j,
                            None => return false,
                        };
                        op_region == Some(j) || op_in_entry
                    }
                    // non-phi user: V12 — same region (earlier) or entry
                    _ => {
                        if op_in_entry && uc.region == RegionId(0) {
                            // entry-defined operand, entry user: positional check
                            return g.index_in_region(op).map_or(false, |p| {
                                p < g.index_in_region(u).unwrap_or(0)
                            });
                        }
                        op_region == Some(uc.region) || op_in_entry
                    }
                }
            });
            if collapse_ok {
                for (u, slot) in users {
                    let from = g.cell(u).expect("present").operands[slot as usize];
                    let e = Edit::Retarget { cell: u, slot, from, to: op };
                    rec.edits.push(e.clone());
                    replay::apply_edit(g, &e)?;
                }
                let e = Edit::RemoveCell {
                    id: phi,
                    ledger: format!("{}: phi collapsed to {} (single join left)", ledger, op),
                    summary,
                };
                rec.edits.push(e.clone());
                replay::apply_edit(g, &e)?;
                continue;
            }
            // materialize: a CONST operand can be copied into ENTRY at
            // index 0 — legal for every user kind (V12 entry-exempt
            // for non-phi users, V07 entry-exempt for phi users)
            if let Some(CellKind::Const { ty, val }) = g.cell(op).map(|c| c.kind.clone()) {
                let material = Cell::new(RegionId(0), CellKind::Const { ty, val });
                let mid = CellId(g.slab.len() as u32);
                let e = Edit::AddCell { id: mid, index: 0, cell: material };
                rec.edits.push(e.clone());
                replay::apply_edit(g, &e)?;
                for (u, slot) in users {
                    let from = g.cell(u).expect("present").operands[slot as usize];
                    let e = Edit::Retarget { cell: u, slot, from, to: mid };
                    rec.edits.push(e.clone());
                    replay::apply_edit(g, &e)?;
                }
                let e = Edit::RemoveCell {
                    id: phi,
                    ledger: format!(
                        "{}: phi materialized as const {} (operand was a const from '{}')",
                        ledger,
                        mid,
                        g.region_name(op_region.unwrap_or(region))
                    ),
                    summary,
                };
                rec.edits.push(e.clone());
                replay::apply_edit(g, &e)?;
                rec.notes.push(format!("{}: phi {} materialized as const {}", ledger, phi, mid));
                continue;
            }
            return Err(format!(
                "{}: phi {} has only the stripped join and operand {} (non-const, defined in '{}') cannot legally replace it — refusing",
                ledger,
                phi,
                op,
                g.region_name(op_region.unwrap_or(region))
            ));
        }

        // joins > 1: replace the phi minus the src join
        let mut new_phi = Cell::new(
            region,
            CellKind::Phi {
                joins: joins.iter().enumerate().filter(|(i, _)| *i != pos).map(|(_, &j)| j).collect(),
            },
        );
        new_phi.operands = operands.iter().enumerate().filter(|(i, _)| *i != pos).map(|(_, &o)| o).collect();
        {
            let e = Edit::RemoveCell { id: phi, ledger: format!("{}: phi replaced (join '{}' stripped)", ledger, g.region_name(src)), summary };
            rec.edits.push(e.clone());
            replay::apply_edit(g, &e)?;
        }
        let new_id = CellId(g.slab.len() as u32);
        let e = Edit::AddCell { id: new_id, index, cell: new_phi };
        rec.edits.push(e.clone());
        replay::apply_edit(g, &e)?;
        for (u, slot) in users {
            let from = g.cell(u).expect("present").operands[slot as usize];
            let e = Edit::Retarget { cell: u, slot, from, to: new_id };
            rec.edits.push(e.clone());
            replay::apply_edit(g, &e)?;
        }
    }
    Ok(())
}

/// Drop the control edge `src → dead_arm`: the Branch at the end of
/// `src` becomes a Jump on the OTHER arm, and every phi in `dead_arm`
/// that joined on `src` drops that join+operand pair (with the
/// zero-join collapse / one-join-keep strategy above). This is the
/// semmut `JoinDropWithEdge` surgery, factored and made
/// semantics-preserving (semmut kept a RANDOM arm; the pass keeps the
/// arm the program actually takes).
///
/// Verifies the result before returning it. Errors name the V-code.
pub fn drop_edge(f: &Fabric, src: RegionId, dead_arm: RegionId) -> Result<(Fabric, DiffRecord), String> {
    let term = f
        .region(src)
        .and_then(|r| r.cells.last().copied())
        .ok_or_else(|| format!("drop_edge: region '{}' has no terminator", f.region_name(src)))?;
    let keep = match f.cell(term).map(|c| &c.kind) {
        Some(CellKind::Branch { then_r, else_r }) => {
            if then_r == else_r {
                return Err(format!(
                    "drop_edge: branch in '{}' targets '{}' twice — no edge to drop",
                    f.region_name(src),
                    f.region_name(*then_r)
                ));
            }
            if *then_r == dead_arm {
                *else_r
            } else if *else_r == dead_arm {
                *then_r
            } else {
                return Err(format!(
                    "drop_edge: branch in '{}' does not target '{}'",
                    f.region_name(src),
                    f.region_name(dead_arm)
                ));
            }
        }
        _ => {
            return Err(format!(
                "drop_edge: region '{}' does not end in a branch",
                f.region_name(src)
            ))
        }
    };

    let mut g = f.clone();
    let mut rec = DiffRecord::new("drop-edge");
    let index = g.index_in_region(term).expect("listed");
    let jump = Cell::new(src, CellKind::Jump { target: keep });
    let summary = crate::text::render_cell(&g, term);
    let e = Edit::RemoveCell {
        id: term,
        ledger: format!(
            "edge-drop: br -> jump '{}' (arm '{}' dropped)",
            g.region_name(keep),
            g.region_name(dead_arm)
        ),
        summary,
    };
    rec.edits.push(e.clone());
    replay::apply_edit(&mut g, &e)?;
    let jid = CellId(g.slab.len() as u32);
    let e = Edit::AddCell { id: jid, index, cell: jump };
    rec.edits.push(e.clone());
    replay::apply_edit(&mut g, &e)?;

    strip_src_joins(&mut g, &mut rec, src, dead_arm, "edge-drop")?;

    match verify(&g) {
        Ok(()) => Ok((g, rec)),
        Err(e) => Err(format!("drop_edge produced {}: {}", e.code, e.detail)),
    }
}

/// Add a join+operand entry to a phi: the maintenance inverse of
/// drop_edge, for when a NEW control edge `join → phi's region`
/// appears. Guards mirror V06/V14/V16/V07/V13. Expressible edits
/// (phi replacement), replay-applied, verified before return.
pub fn join_phi(
    f: &Fabric,
    phi: CellId,
    join: RegionId,
    value: CellId,
) -> Result<(Fabric, DiffRecord), String> {
    let (region, joins, operands) = match f.cell(phi).map(|c| (c.region, c.kind.clone(), c.operands.clone())) {
        Some((r, CellKind::Phi { joins }, ops)) => (r, joins, ops),
        _ => return Err(format!("join_phi: {} is not a phi", phi)),
    };
    if joins.contains(&join) {
        return Err(format!("join_phi: phi {} already joins on '{}'", phi, f.region_name(join)));
    }
    // the edge must be real: join's terminator targets phi's region
    let targets = f.successors(join);
    if !targets.contains(&region) {
        return Err(format!(
            "join_phi: '{}' does not target '{}' — the join would violate V06",
            f.region_name(join),
            f.region_name(region)
        ));
    }
    let val_region = f.cell(value).map(|c| c.region);
    if val_region != Some(join) && val_region != Some(RegionId(0)) {
        return Err(format!("join_phi: value {} not defined in '{}' or entry — V07", value, f.region_name(join)));
    }
    let ty = f.ty_of(value);
    let first = f.ty_of(operands[0]);
    if ty != first {
        return Err(format!("join_phi: type mismatch — V13 ({:?} vs {:?})", ty, first));
    }

    let mut g = f.clone();
    let mut rec = DiffRecord::new("join-phi");
    let index = g.index_in_region(phi).expect("listed");
    let users: Vec<(CellId, u32)> = g.uses_of(phi);
    let summary = crate::text::render_cell(&g, phi);
    let e = Edit::RemoveCell {
        id: phi,
        ledger: format!("join-phi: phi replaced (join '{}' added)", g.region_name(join)),
        summary,
    };
    rec.edits.push(e.clone());
    replay::apply_edit(&mut g, &e)?;
    let mut new_phi = Cell::new(region, CellKind::Phi { joins: joins.iter().copied().chain(std::iter::once(join)).collect() });
    new_phi.operands = operands.iter().copied().chain(std::iter::once(value)).collect();
    let new_id = CellId(g.slab.len() as u32);
    let e = Edit::AddCell { id: new_id, index, cell: new_phi };
    rec.edits.push(e.clone());
    replay::apply_edit(&mut g, &e)?;
    for (u, slot) in users {
        let from = g.cell(u).expect("present").operands[slot as usize];
        let e = Edit::Retarget { cell: u, slot, from, to: new_id };
        rec.edits.push(e.clone());
        replay::apply_edit(&mut g, &e)?;
    }
    match verify(&g) {
        Ok(()) => Ok((g, rec)),
        Err(e) => Err(format!("join_phi produced {}: {}", e.code, e.detail)),
    }
}

/// Graft `donor_region` from `donor` into `f` as a NEW region:
/// cells are copied (fresh ids, same order), operands remapped through
/// `cell_map` (every operand must be mapped — donor operands that are
/// params should be pre-bound by the caller), phi joins and terminator
/// targets remapped through `region_map` (unmapped target regions are
/// an error: the graft must be closed under regions it names).
///
/// Records AddCell edits for the copied cells. Region creation itself
/// is a note (inexpressible). Does NOT verify — the caller composes
/// (this is a primitive, not a pass).
pub fn region_graft(
    f: &Fabric,
    donor: &Fabric,
    donor_region: RegionId,
    cell_map: &BTreeMap<CellId, CellId>,
    region_map: &BTreeMap<u32, RegionId>,
    name: &str,
) -> Result<(Fabric, RegionId, DiffRecord), String> {
    let mut g = f.clone();
    let new_r = g.add_region(name);
    let mut rec = DiffRecord::new("region-graft");
    rec.notes.push(format!(
        "region '{}' grafted as '{}' — RegionAdded inexpressible; cells carried as AddCell edits",
        donor.region_name(donor_region),
        name
    ));
    region_graft_into(&mut g, donor, donor_region, new_r, cell_map, region_map, &mut rec)?;
    Ok((g, new_r, rec))
}

/// The graft core: copy `donor_region`'s cells into the EXISTING
/// dest region of `g` (the caller pre-creates dest regions so
/// region_map is complete before any cell is copied). Mutates g and
/// rec in place; Err names the first ungraftable cell.
fn region_graft_into(
    g: &mut Fabric,
    donor: &Fabric,
    donor_region: RegionId,
    dest: RegionId,
    cell_map: &BTreeMap<CellId, CellId>,
    region_map: &BTreeMap<u32, RegionId>,
    rec: &mut DiffRecord,
) -> Result<(), String> {
    {
    let src_ids: Vec<CellId> = donor.region(donor_region).map(|x| x.cells.clone()).unwrap_or_default();
    let mut local_map: BTreeMap<CellId, CellId> = cell_map.clone();
    for did in src_ids {
        let dc = donor.cell(did).ok_or_else(|| format!("graft: donor cell {} missing", did))?;
        let mut cell = Cell::new(dest, dc.kind.clone());
        let mut ops = vec![];
        for &op in &dc.operands {
            let m = local_map.get(&op).copied().ok_or_else(|| {
                format!(
                    "graft: operand {} of donor cell {} is unmapped (bind params / graft dependencies first)",
                    op, did
                )
            })?;
            ops.push(m);
        }
        cell.operands = ops;
        // remap kind-level region references
        match &mut cell.kind {
            CellKind::Branch { then_r, else_r } => {
                *then_r = *region_map.get(&then_r.0).ok_or_else(|| {
                    format!("graft: branch target '{}' unmapped", donor.region_name(*then_r))
                })?;
                *else_r = *region_map.get(&else_r.0).ok_or_else(|| {
                    format!("graft: branch target '{}' unmapped", donor.region_name(*else_r))
                })?;
            }
            CellKind::Jump { target } => {
                *target = *region_map.get(&target.0).ok_or_else(|| {
                    format!("graft: jump target '{}' unmapped", donor.region_name(*target))
                })?;
            }
            CellKind::Phi { joins } => {
                for j in joins.iter_mut() {
                    *j = *region_map.get(&j.0).ok_or_else(|| {
                        format!("graft: phi join '{}' unmapped", donor.region_name(*j))
                    })?;
                }
            }
            CellKind::Param { .. } => {
                return Err(format!(
                    "graft: param {} in a non-entry donor region — V12 makes it ungraftable (bind it to a value first)",
                    did
                ))
            }
            _ => {}
        }
        let nid = CellId(g.slab.len() as u32);
        let index = g.regions[dest.0 as usize].cells.len();
        g.regions[dest.0 as usize].cells.push(nid);
        g.slab.push(Some(cell.clone()));
        rec.edits.push(Edit::AddCell { id: nid, index, cell });
        local_map.insert(did, nid);
    }
    Ok(())
    }
}

// ======================================================================
// The property oracle — a spike-grade concrete interpreter
// ======================================================================

/// Concrete execution of a fabric: walk regions from entry; at each
/// Branch, the condition must be dataflow-decidable (a const through
/// consts/arith/cmp — `semmut::eval_dataflow`, i.e. Rust's own checked
/// arithmetic, NOT the fold table); phis resolve through the region the
/// executed path entered FROM; calls resolve by substituting const
/// args into the callee and interpreting it. `None` = unjudgeable
/// (param-dependent value, undecidable branch, call depth/step budget
/// exceeded, void ret). Spike-grade and honest: `came_from` keeps the
/// LAST entry into a region, so phi chains across loop iterations read
/// the latest entry — a documented limitation, fine for the corpus's
/// small acyclic fabrics (C6: no phi participates in loop-carried
/// dataflow; loops are control-wires only).
pub fn interp(f: &Fabric, funcs: &BTreeMap<String, Fabric>, budget: usize) -> Option<ConstVal> {
    interp_inner(f, funcs, budget, &mut BTreeMap::new(), 0)
}

fn interp_inner(
    f: &Fabric,
    funcs: &BTreeMap<String, Fabric>,
    budget: usize,
    came: &mut BTreeMap<u32, RegionId>,
    depth: u32,
) -> Option<ConstVal> {
    if depth > 16 {
        return None; // call-graph depth budget (recursion = unjudgeable)
    }
    let mut pc = f.entry()?;
    came.remove(&pc.0); // entry is entered from nowhere
    let mut steps = 0usize;
    loop {
        if steps > budget {
            return None; // infinite loop / runaway walk
        }
        steps += 1;
        let region = f.region(pc)?;
        let term = *region.cells.last()?;
        let c = f.cell(term)?;
        match &c.kind {
            CellKind::Ret => {
                return match c.operands.first() {
                    Some(&op) => eval_cell(f, op, came, funcs, budget, depth),
                    None => Some(ConstVal::I1(true)), // void ret: the run itself is the answer
                };
            }
            CellKind::Jump { target } => {
                came.insert(target.0, pc);
                pc = *target;
            }
            CellKind::Branch { then_r, else_r } => {
                let cond = eval_cell(f, c.operands[0], came, funcs, budget, depth)?;
                let next = match cond {
                    ConstVal::I1(true) => *then_r,
                    ConstVal::I1(false) => *else_r,
                    _ => return None, // non-bool condition: unjudgeable (cannot happen post-verify)
                };
                came.insert(next.0, pc);
                pc = next;
            }
            _ => return None, // not a terminator: malformed
        }
    }
}

fn eval_cell(
    f: &Fabric,
    id: CellId,
    came: &BTreeMap<u32, RegionId>,
    funcs: &BTreeMap<String, Fabric>,
    budget: usize,
    depth: u32,
) -> Option<ConstVal> {
    eval_cell_b(f, id, came, funcs, budget, depth, 0)
}

fn eval_cell_b(
    f: &Fabric,
    id: CellId,
    came: &BTreeMap<u32, RegionId>,
    funcs: &BTreeMap<String, Fabric>,
    budget: usize,
    depth: u32,
    b: usize,
) -> Option<ConstVal> {
    if b > budget {
        return None;
    }
    let c = f.cell(id)?;
    match &c.kind {
        CellKind::Const { val, .. } => Some(val.clone()),
        CellKind::Arith { .. } | CellKind::Cmp { .. } => {
            let a = eval_cell_b(f, c.operands[0], came, funcs, budget, depth, b + 1)?;
            let b2 = eval_cell_b(f, c.operands[1], came, funcs, budget, depth, b + 1)?;
            // arithmetic via constfold's kernels — the same kernels the
            // R1 fold-table oracle audits against Rust's checked
            // arithmetic (constfold.rs oracle module), NOT a restatement
            match &c.kind {
                CellKind::Arith { op, .. } => crate::passes::constfold::eval_arith(*op, a, b2),
                CellKind::Cmp { op } => crate::passes::constfold::eval_cmp(*op, a, b2),
                _ => unreachable!(),
            }
        }
        CellKind::Phi { joins } => {
            let region = c.region;
            let from = came.get(&region.0).copied()?;
            let pos = joins.iter().position(|&j| j == from)?;
            eval_cell_b(f, c.operands[pos], came, funcs, budget, depth, b + 1)
        }
        CellKind::Call { name, .. } => {
            let callee = funcs.get(name)?;
            let mut args = vec![];
            for &op in &c.operands {
                args.push(eval_cell_b(f, op, came, funcs, budget, depth, b + 1)?);
            }
            interp_call(callee, &args, funcs, budget, depth)
        }
        _ => None, // params, terminators: no value
    }
}

/// Interpret a callee with params bound to const args: a shallow
/// clone with Param cells replaced by Const cells (params live only in
/// the callee's entry — V12).
fn interp_call(
    callee: &Fabric,
    args: &[ConstVal],
    funcs: &BTreeMap<String, Fabric>,
    budget: usize,
    depth: u32,
) -> Option<ConstVal> {
    let mut g = callee.clone();
    let entry = g.entry()?;
    let ids: Vec<CellId> = g.region(entry)?.cells.clone();
    let mut i = 0;
    for id in ids {
        if let Some(c) = g.cell(id) {
            if let CellKind::Param { ty } = &c.kind {
                let val = args.get(i).copied()?;
                let mut cell = c.clone();
                cell.kind = CellKind::Const { ty: *ty, val };
                g.slab[id.0 as usize] = Some(cell);
                i += 1;
            }
        }
    }
    if i != args.len() {
        return None; // arity drift: unjudgeable
    }
    interp_inner(&g, funcs, budget, &mut BTreeMap::new(), depth + 1)
}

// ======================================================================
// Pass A — const-branch fold (blocked pass #1)
// ======================================================================

#[derive(Debug, Default, Clone)]
pub struct FoldStats {
    pub branches_seen: usize,
    pub const_conditioned: usize, // cond dataflow-decidable
    pub folded: usize,            // drop_edge applied + verify green
    pub phi_collapses: usize,     // zero-join phis collapsed to operands
    pub kept_one_join: usize,     // zero-join phis kept as 1-join muxes
    pub failed: usize,            // drop_edge refused / verify red
}

/// Fold every Branch whose condition is a dataflow constant into a
/// Jump on the taken arm, with phi-join maintenance in the dropped
/// arm (drop_edge). Reuses the semmut surgery; the difference from
/// semmut's mutation is that the KEPT arm is the one the constant
/// selects — semantics-preserving by construction, which the property
/// oracle (interp) checks. Fixpoint sweeps (folding can expose more
/// const branches via... it cannot, today — conds fold only if const
/// already; kept at 2 sweeps max for the future constfold feed-in).
pub fn const_branch_fold(f: &Fabric) -> Result<(Fabric, DiffRecord, FoldStats), String> {
    if let Err(e) = verify(f) {
        return Err(format!("const_branch_fold refuses unverified input: {}", e));
    }
    let mut g = f.clone();
    let mut rec = DiffRecord::new("const-branch-fold");
    let mut st = FoldStats::default();
    for _sweep in 0..2 {
        let mut changed = false;
        for ri in 0..g.regions.len() as u32 {
            let src = RegionId(ri);
            let (term, cond) = match g
                .region(src)
                .and_then(|r| r.cells.last().copied())
                .and_then(|t| g.cell(t).map(|c| (t, c.operands.first().copied())))
            {
                Some((t, Some(c))) => (t, c),
                _ => continue,
            };
            if !matches!(g.cell(term).map(|c| &c.kind), Some(CellKind::Branch { .. })) {
                continue;
            }
            st.branches_seen += 1;
            // decide the condition against Rust arithmetic (semmut oracle)
            let val = eval_dataflow(&g, cond, 0);
            let taken = match val {
                Some(ConstVal::I1(true)) => {
                    st.const_conditioned += 1;
                    match g.cell(term).map(|c| c.kind.clone()) {
                        Some(CellKind::Branch { then_r, .. }) => then_r,
                        _ => continue,
                    }
                }
                Some(ConstVal::I1(false)) => {
                    st.const_conditioned += 1;
                    match g.cell(term).map(|c| c.kind.clone()) {
                        Some(CellKind::Branch { else_r, .. }) => else_r,
                        _ => continue,
                    }
                }
                _ => continue, // param-dependent: honestly not foldable
            };
            let dead = match g.cell(term).map(|c| c.kind.clone()) {
                Some(CellKind::Branch { then_r, else_r }) => {
                    if then_r == taken { else_r } else { then_r }
                }
                _ => continue,
            };
            let collapses = rec.notes.iter().filter(|n| n.contains("collapsed to")).count();
            let keeps = rec.notes.iter().filter(|n| n.contains("kept as one-join mux")).count();
            match drop_edge(&g, src, dead) {
                Ok((g2, mut r2)) => {
                    g = g2;
                    rec.edits.append(&mut r2.edits);
                    rec.notes.append(&mut r2.notes);
                    st.folded += 1;
                    st.phi_collapses +=
                        rec.notes.iter().filter(|n| n.contains("collapsed to")).count() - collapses;
                    st.kept_one_join +=
                        rec.notes.iter().filter(|n| n.contains("kept as one-join mux")).count() - keeps;
                    changed = true;
                }
                Err(e) => {
                    st.failed += 1;
                    rec.notes.push(format!("skip branch in '{}': {}", g.region_name(src), e));
                }
            }
        }
        if !changed {
            break;
        }
    }
    if let Err(e) = verify(&g) {
        return Err(format!("const_branch_fold produced an invalid fabric: {}", e));
    }
    Ok((g, rec, st))
}

// ======================================================================
// Pass B — region-DCE (blocked pass #2)
// ======================================================================

#[derive(Debug, Default, Clone)]
pub struct RegionDceStats {
    pub regions_before: usize,
    pub regions_removed: usize,
    pub cells_removed: usize,
    pub live_phis_stripped: usize, // joins dropped from live phis
    pub phi_collapses: usize,
}

/// Remove every region unreachable from entry (the pass dce.rs
/// defers: "unreachable REGIONS are not removed"). Does the phi-join
/// maintenance the removal implies: live phis that joined on a removed
/// region drop those joins (same strip strategy as drop_edge, here
/// applied BEFORE the compaction so the join labels still name live
/// regions).
pub fn region_dce(f: &Fabric) -> Result<(Fabric, DiffRecord, RegionDceStats), String> {
    if let Err(e) = verify(f) {
        return Err(format!("region_dce refuses unverified input: {}", e));
    }
    let live = reachable_regions(f);
    let mut st = RegionDceStats { regions_before: f.regions.len(), ..Default::default() };
    let dead: Vec<u32> = (0..f.regions.len() as u32).filter(|i| !live.contains(i)).collect();
    if dead.is_empty() {
        return Ok((f.clone(), DiffRecord::new("region-dce"), st));
    }

    let mut g = f.clone();
    let mut rec = DiffRecord::new("region-dce");
    let dead_set: BTreeSet<u32> = dead.iter().copied().collect();

    // 1. phi maintenance FIRST: live phis drop joins naming dead
    //    regions (strip while the labels are still meaningful).
    let live_cell_ids: Vec<CellId> = g.cells().collect();
    for id in live_cell_ids {
        let region = match g.cell(id) {
            Some(c) => c.region,
            None => continue,
        };
        if dead_set.contains(&region.0) {
            continue; // phis in dead regions die with their region
        }
        let joins = match g.cell(id).map(|c| c.kind.clone()) {
            Some(CellKind::Phi { joins }) => joins,
            _ => continue,
        };
        let dead_joins: Vec<RegionId> = joins.iter().copied().filter(|j| dead_set.contains(&j.0)).collect();
        if dead_joins.is_empty() {
            continue;
        }
        // strip one dead join at a time through the shared strategy
        for dj in dead_joins {
            let before_notes = rec.notes.len();
            strip_src_joins(&mut g, &mut rec, dj, region, "region-dce")?;
            st.live_phis_stripped += 1;
            if rec.notes[before_notes..].iter().any(|n| n.contains("collapsed to")) {
                st.phi_collapses += 1;
            }
            // (strip_src_joins never leaves an empty phi: it collapses
            // or keeps a one-join mux; nothing to abort on here)
        }
    }

    // 2. batch-remove the dead set: strip their cells (RemoveCell
    //    edits with ledger), drop the regions from the Vec in one
    //    pass, compact ids once. (Per-region `region_remove` would
    //    refuse here: dead regions may target other dead regions.)
    {
        let mut keep_regions: Vec<crate::fabric::Region> = vec![];
        for (i, r) in g.regions.into_iter().enumerate() {
            if dead_set.contains(&(i as u32)) {
                st.regions_removed += 1;
                continue;
            }
            keep_regions.push(r);
        }
        let orig_len = keep_regions.len() + dead_set.len();
        g.regions = keep_regions;
        // old -> new id map for survivors (region ids are dense 0..n)
        let mut id_map: BTreeMap<u32, u32> = BTreeMap::new();
        {
            let mut next = 0u32;
            for i in 0..orig_len as u32 {
                if dead_set.contains(&i) {
                    continue;
                }
                id_map.insert(i, next);
                next += 1;
            }
        }
        // dead cells out of the slab FIRST (their region ids are the
        // OLD ids; after the remap below they would masquerade as
        // survivors), with ledger edits
        for id in (0..g.slab.len() as u32).map(CellId).collect::<Vec<_>>() {
            if g.cell(id).is_some() {
                let r = g.cell(id).unwrap().region;
                if id_map.contains_key(&r.0) {
                    continue; // survivor
                }
                let summary = crate::text::render_cell(&g, id);
                g.slab[id.0 as usize] = None;
                st.cells_removed += 1;
                rec.edits.push(Edit::RemoveCell {
                    id,
                    ledger: "region-dce: region unreachable from entry, cells dropped with it".into(),
                    summary,
                });
            }
        }
        // then renumber the survivors' region references
        let remap = |x: &mut RegionId| {
            if let Some(n) = id_map.get(&x.0) {
                x.0 = *n;
            }
        };
        for c in g.slab.iter_mut().flatten() {
            remap(&mut c.region);
            match &mut c.kind {
                CellKind::Branch { then_r, else_r } => {
                    remap(then_r);
                    remap(else_r);
                }
                CellKind::Jump { target } => remap(target),
                CellKind::Phi { joins } => {
                    for j in joins.iter_mut() {
                        remap(j);
                    }
                }
                _ => {}
            }
        }
        rec.notes.push(format!(
            "{} dead regions removed, region ids compacted — NOT expressible in the Edit vocabulary (no RegionRemoved kind)",
            dead.len()
        ));
    }
    if let Err(e) = verify(&g) {
        return Err(format!("region_dce produced an invalid fabric: {}", e));
    }
    Ok((g, rec, st))
}

// ======================================================================
// Pass C — CFG-graft inline (blocked pass #3)
// ======================================================================

#[derive(Debug, Default, Clone)]
pub struct InlineStats {
    pub calls_seen: usize,
    pub inlined: usize,
    pub skipped: usize,
    pub regions_grafted: usize,
    pub phis_built: usize,
    pub skips: Vec<String>,
}

/// Inline MULTI-REGION callees into the caller's CFG — the surgery
/// passes/inline.rs defers to exactly this vocabulary ("CFG grafting
/// needs the region-edit diff vocabulary"). Per call site (entry
/// region only; uses confined to the call's region — every skip is
/// noted, never silent):
///
/// 1. fresh continuation region K receives the caller's post-call
///    cells + the caller's terminator (moved, ids stable);
/// 2. the callee's entry body grafts into the caller's entry at the
///    call position (params rebound to args — the arch doc's rule);
/// 3. the callee's non-entry regions graft as fresh regions via
///    region_graft, Ret terminators becoming Jumps to K;
/// 4. the caller's entry terminator is REPLACED by the callee's entry
///    terminator (targets remapped); the old terminator's edges now
///    flow from K, so successor phis relabel their join entry→K
///    (same edge, new source — V06/V16 preserved exactly);
/// 5. a phi in K carries the return value from every ret region
///    (skipped when the single ret's value is entry-defined);
/// 6. uses of the call retarget to that value; the call cell is
///    removed with a conservation ledger entry.
pub fn cfg_graft_inline(
    caller: &Fabric,
    funcs: &BTreeMap<String, Fabric>,
) -> Result<(Fabric, DiffRecord, InlineStats), String> {
    if let Err(e) = verify(caller) {
        return Err(format!("cfg_graft_inline refuses unverified input: {}", e));
    }
    for (name, f) in funcs {
        if let Err(e) = verify(f) {
            return Err(format!("cfg_graft_inline refuses callee '{}': {}", name, e));
        }
    }
    let mut g = caller.clone();
    let mut rec = DiffRecord::new("cfg-inline");
    let mut st = InlineStats::default();

    let call_sites: Vec<CellId> = g
        .regions
        .first()
        .map(|r| r.cells.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|&id| matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Call { .. })))
        .collect();

    for call_id in call_sites {
        st.calls_seen += 1;
        let (callee_name, ret_ty) = match g.cell(call_id).map(|c| c.kind.clone()) {
            Some(CellKind::Call { name, ret_ty }) => (name, ret_ty),
            _ => unreachable!(),
        };
        let (g2, notes2, edits2, st2, ok) = inline_one(&g, call_id, &callee_name, ret_ty, funcs)?;
        g = g2;
        rec.notes.extend(notes2);
        if !ok {
            st.skipped += 1;
            st.skips.push(format!("{}: skipped (see notes)", call_id));
            continue;
        }
        rec.edits.extend(edits2);
        st.inlined += 1;
        st.regions_grafted += st2.regions_grafted;
        st.phis_built += st2.phis_built;
    }

    if let Err(e) = verify(&g) {
        return Err(format!("cfg_graft_inline produced an invalid fabric: {}", e));
    }
    Ok((g, rec, st))
}

/// Inline one call site. Returns (fabric, notes, stats, inlined?).
/// All edits are applied directly (region-level moves/relabels are
/// inexpressible in the Edit vocabulary — notes carry them; the cell
/// adds/removes/retargets that ARE expressible are recorded as edits
/// by the caller-visible record via `edits_out`).
#[allow(clippy::too_many_arguments)]
fn inline_one(
    g: &Fabric,
    call_id: CellId,
    callee_name: &str,
    ret_ty: Type,
    funcs: &BTreeMap<String, Fabric>,
) -> Result<(Fabric, Vec<String>, Vec<Edit>, InlineStats, bool), String> {
    let mut edits: Vec<Edit> = vec![];
    let mut notes: Vec<String> = vec![];
    let mut st = InlineStats::default();
    let callee = match funcs.get(callee_name) {
        Some(c) => c,
        None => {
            notes.push(format!("skip {}: callee '{}' not provided", call_id, callee_name));
            return Ok((g.clone(), notes, edits, st, false));
        }
    };
    let entry = RegionId(0);
    if !callee.predecessors(entry).is_empty() {
        notes.push(format!(
            "skip {}: callee '{}' is cyclic at entry (entry has predecessors)",
            call_id, callee_name
        ));
        return Ok((g.clone(), notes, edits, st, false));
    }
    // every ret returns exactly one value of the call's declared type
    let mut ret_ops: Vec<CellId> = vec![];
    for id in callee.cells() {
        if let Some(c) = callee.cell(id) {
            if let CellKind::Ret = &c.kind {
                match c.operands.first() {
                    Some(&op) if callee.ty_of(op) == Some(ret_ty) => ret_ops.push(op),
                    _ => {
                        notes.push(format!(
                            "skip {}: callee '{}' has a ret that is void or type-mismatched",
                            call_id, callee_name
                        ));
                        return Ok((g.clone(), notes, edits, st, false));
                    }
                }
            }
        }
    }
    if ret_ops.is_empty() {
        notes.push(format!("skip {}: no usable ret in '{}'", call_id, callee_name));
        return Ok((g.clone(), notes, edits, st, false));
    }
    // guards about the call site's surroundings (each skip is noted)
    let call_pos = g.index_in_region(call_id).expect("listed");
    let entry_cells = g.region(entry).expect("entry").cells.clone();
    let post: Vec<CellId> = entry_cells[call_pos + 1..].to_vec();
    let post_body: Vec<CellId> = post[..post.len().saturating_sub(1)].to_vec();
    // (a skip returns the UNTOUCHED fabric — h does not exist yet)
    macro_rules! skip {
        ($why:expr) => {{
            let mut n = vec![format!("skip {} ({}): {}", call_id, callee_name, $why)];
            n.extend(notes.iter().cloned());
            return Ok((g.clone(), n, edits, st.clone(), false));
        }};
    }
    let region_of = |id: CellId| g.cell(id).map(|c| c.region);
    let foreign_use_of_call = g.cells().any(|u| {
        region_of(u) != Some(entry)
            && g.cell(u).map(|c| c.operands.contains(&call_id)).unwrap_or(false)
    });
    if foreign_use_of_call {
        skip!("call used outside entry (V12 after graft)");
    }
    let foreign_use_of_post = g.cells().any(|u| {
        g.cell(u)
            .map(|c| {
                c.region != entry && c.operands.iter().any(|o| post_body.contains(o))
            })
            .unwrap_or(false)
    });
    if foreign_use_of_post {
        skip!("post-call cell used outside entry (V12 after move)");
    }
    if post_body.iter().any(|&p| matches!(g.cell(p).map(|c| &c.kind), Some(CellKind::Phi { .. }))) {
        skip!("phi among post-call cells (joins would break on move)");
    }
    // ANY phi (entry phis included: back-edge phis may take entry
    // post-cell operands — V07 would fire once they move)
    let any_phi_on_post = g.cells().any(|u| {
        matches!(g.cell(u).map(|c| &c.kind), Some(CellKind::Phi { .. }))
            && g.cell(u).map(|c| c.operands.iter().any(|o| post_body.contains(o))).unwrap_or(false)
    });
    if any_phi_on_post {
        skip!("phi operand is a post-call cell (V07 after move)");
    }
    let params: Vec<CellId> = callee
        .region(entry)
        .map(|r| r.cells.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|&id| matches!(callee.cell(id).map(|c| &c.kind), Some(CellKind::Param { .. })))
        .collect();
    let args: Vec<CellId> = g.cell(call_id).map(|c| c.operands.clone()).unwrap_or_default();
    if params.len() != args.len() {
        skip!("arity mismatch");
    }

    let mut h = g.clone();

    // 1. continuation region K; move post cells + the old terminator
    //    into it (ids stable — MoveCell is inexpressible; noted)
    let k = h.add_region(format!("{}_cont", callee_name));
    notes.push(format!(
        "continuation region '{}' added — RegionAdded inexpressible in the Edit vocabulary",
        callee_name
    ));
    {
        let mut moved = post_body.clone();
        moved.push(post.last().copied().expect("entry ends in a terminator"));
        let n_moved = moved.len();
        h.regions[entry.0 as usize].cells.truncate(call_pos);
        for id in moved {
            if let Some(c) = h.cell_mut(id) {
                c.region = k;
            }
            h.regions[k.0 as usize].cells.push(id);
        }
        notes.push(format!(
            "{} entry cells moved to '{}' — MoveCell inexpressible in the Edit vocabulary",
            n_moved,
            callee_name
        ));
    }

    // 2. region map: callee entry -> caller entry; every other callee
    //    region -> a fresh empty region (pre-created so the map is
    //    complete before any cell with region references is copied)
    let mut region_map: BTreeMap<u32, RegionId> = BTreeMap::new();
    region_map.insert(0, entry);
    for ri in 1..callee.regions.len() as u32 {
        let r = h.add_region(format!("{}_{}", callee_name, callee.region_name(RegionId(ri))));
        region_map.insert(ri, r);
        st.regions_grafted += 1;
        notes.push(format!(
            "region '{}' grafted as fresh region — RegionAdded inexpressible",
            callee.region_name(RegionId(ri))
        ));
    }

    // 3. cell map starts as params -> args
    let mut cell_map: BTreeMap<CellId, CellId> = BTreeMap::new();
    for (p, a) in params.iter().zip(args.iter()) {
        cell_map.insert(*p, *a);
    }

    // 4. graft the callee entry BODY (everything before the
    //    terminator, minus params) into caller entry at the call
    //    position (params rebound to args; operands resolve through
    //    the growing cell_map — a verified callee has no forward refs;
    //    the terminator itself is step 6's job)
    let mut insert_at = call_pos;
    let entry_body: Vec<CellId> = callee
        .region(entry)
        .map(|r| {
            let n = r.cells.len();
            r.cells[..n.saturating_sub(1)].to_vec() // all but the terminator
        })
        .unwrap_or_default();
    for &cid in entry_body.iter() {
        let cc = callee.cell(cid).expect("present");
        match &cc.kind {
            CellKind::Param { .. } => continue,
            _ => {}
        }
        let mut mapped = cc.clone();
        mapped.region = entry;
        let mut ops = vec![];
        for &op in &cc.operands {
            let m = cell_map.get(&op).copied().ok_or_else(|| {
                format!(
                    "inline {}: callee entry operand {} of cell {} unmapped",
                    call_id, op, cid
                )
            })?;
            ops.push(m);
        }
        mapped.operands = ops;
        let nid = h.insert_cell(entry, insert_at, mapped.clone());
        edits.push(Edit::AddCell { id: nid, index: insert_at, cell: mapped });
        cell_map.insert(cid, nid);
        insert_at += 1;
    }

    // 5. graft every non-entry callee region. TWO GLOBAL passes:
    //    pass A pre-registers fresh ids for EVERY grafted cell first
    //    (phi operands may reference cells in any grafted region —
    //    phis are exempt from V12's same-region rule, so resolution
    //    needs the full map before any cell is filled); pass B then
    //    resolves kinds + operands and places. Ret terminators ->
    //    Jump(K); other terminators and phi joins remapped through
    //    region_map.
    for ri in 1..callee.regions.len() as u32 {
        let dr = RegionId(ri);
        let dest = *region_map.get(&ri).expect("pre-created");
        let src_ids: Vec<CellId> = callee.region(dr).map(|x| x.cells.clone()).unwrap_or_default();
        for &did in &src_ids {
            let nid = CellId(h.slab.len() as u32);
            h.slab.push(None); // reserve the slot; filled in pass B
            h.regions[dest.0 as usize].cells.push(nid);
            cell_map.insert(did, nid);
        }
    }
    for ri in 1..callee.regions.len() as u32 {
        let dr = RegionId(ri);
        let dest = *region_map.get(&ri).expect("pre-created");
        let src_ids: Vec<CellId> = callee.region(dr).map(|x| x.cells.clone()).unwrap_or_default();
        // pass B: resolve kinds + operands, fill the slots
        for &did in &src_ids {
            let dc = callee.cell(did).expect("present");
            let mut kind = dc.kind.clone();
            match &mut kind {
                CellKind::Ret => {
                    kind = CellKind::Jump { target: k };
                }
                CellKind::Jump { target } => {
                    *target = *region_map.get(&target.0).expect("callee regions all mapped");
                }
                CellKind::Branch { then_r, else_r } => {
                    *then_r = *region_map.get(&then_r.0).expect("callee regions all mapped");
                    *else_r = *region_map.get(&else_r.0).expect("callee regions all mapped");
                }
                CellKind::Phi { joins } => {
                    for j in joins.iter_mut() {
                        *j = *region_map.get(&j.0).expect("callee regions all mapped");
                    }
                }
                CellKind::Param { .. } => {
                    return Err(format!(
                        "inline: param {} in callee non-entry region — V12 makes it ungraftable",
                        did
                    ));
                }
                _ => {}
            }
            let mut cell = Cell::new(dest, kind);
            let mut ops = vec![];
            for &op in &dc.operands {
                let m = cell_map.get(&op).copied().ok_or_else(|| {
                    format!(
                        "inline {}: operand {} of donor cell {} unmapped (donor uses a value outside its own regions — not closed)",
                        call_id, op, did
                    )
                })?;
                ops.push(m);
            }
            cell.operands = ops;
            let nid = *cell_map.get(&did).expect("pre-registered");
            let index = h.regions[dest.0 as usize]
                .cells
                .iter()
                .position(|&x| x == nid)
                .expect("pre-listed");
            h.slab[nid.0 as usize] = Some(cell.clone());
            edits.push(Edit::AddCell { id: nid, index, cell });
        }
    }

    // 6. caller entry terminator := callee entry terminator
    //    (Ret -> Jump(K); targets remapped). The OLD terminator is
    //    already K's terminator (moved in step 1).
    {
        let cal_term = *callee
            .region(entry)
            .and_then(|r| r.cells.last())
            .expect("callee entry has a terminator");
        let cc = callee.cell(cal_term).expect("present");
        let new_kind = match &cc.kind {
            CellKind::Ret => CellKind::Jump { target: k },
            CellKind::Jump { target } => CellKind::Jump {
                target: *region_map.get(&target.0).expect("mapped"),
            },
            CellKind::Branch { then_r, else_r } => CellKind::Branch {
                then_r: *region_map.get(&then_r.0).expect("mapped"),
                else_r: *region_map.get(&else_r.0).expect("mapped"),
            },
            _ => unreachable!("terminator"),
        };
        let mut new_term = Cell::new(entry, new_kind);
        if let Some(&c) = cc.operands.first() {
            // branch cond: through the cell map (entry body cell or arg)
            if let Some(m) = cell_map.get(&c) {
                new_term.operands = vec![*m];
            } else {
                // cond defined in caller entry already (grafted cells
                // occupy call_pos..; the cond must precede them) — use as-is
                new_term.operands = vec![c];
            }
        }
        let at = h.regions[entry.0 as usize].cells.len();
        h.add_cell(entry, new_term.clone());
        edits.push(Edit::AddCell { id: CellId(h.slab.len() as u32 - 1), index: at, cell: new_term });
        notes.push(format!(
            "entry terminator replaced by the callee entry terminator — terminator replacement + Ret->Jump kind change inexpressible in the Edit vocabulary"
        ));
    }

    // 7. relabel joins on the OLD entry-successors: the moved
    //    terminator carries the same edges from K now (V06/V16 exact)
    for s in h.successors(k) {
        let phi_ids: Vec<CellId> = h
            .region(s)
            .map(|x| x.cells.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|&id| {
                matches!(h.cell(id).map(|c| &c.kind), Some(CellKind::Phi { joins }) if joins.contains(&entry))
            })
            .collect();
        for pid in phi_ids {
            if let Some(c) = h.cell_mut(pid) {
                if let CellKind::Phi { joins } = &mut c.kind {
                    for j in joins.iter_mut() {
                        if *j == entry {
                            *j = k;
                        }
                    }
                }
            }
            notes.push(format!(
                "phi {} join relabeled entry -> continuation — join relabel inexpressible in the Edit vocabulary",
                pid
            ));
        }
    }

    // 8. the return value: a phi in K over the ret regions, unless the
    //    single ret's value is entry-defined (then direct retarget)
    let ret_val: CellId = {
        let mapped: Vec<CellId> =
            ret_ops.iter().map(|&o| cell_map.get(&o).copied().unwrap_or(o)).collect();
        if mapped.len() == 1
            && h.cell(mapped[0]).map(|c| c.region == entry).unwrap_or(false)
        {
            mapped[0]
        } else {
            let mut joins = vec![];
            let mut ops = vec![];
            for (&o, &m) in ret_ops.iter().zip(mapped.iter()) {
                let rr = callee.cell(o).map(|c| c.region).unwrap_or(entry);
                let jr = if rr == entry { entry } else { *region_map.get(&rr.0).expect("mapped") };
                joins.push(jr);
                ops.push(m);
            }
            let mut phi = Cell::new(k, CellKind::Phi { joins });
            phi.operands = ops;
            let pid = h.insert_cell(k, 0, phi.clone());
            edits.push(Edit::AddCell { id: pid, index: 0, cell: phi });
            st.phis_built += 1;
            notes.push(format!(
                "return phi {} built in the continuation — AddCell-expressible, its RegionAdded context is not",
                pid
            ));
            pid
        }
    };

    // 9. retarget uses of the call to the return value; the call cell
    //    leaves with a conservation ledger entry
    for (u, slot) in h.uses_of(call_id) {
        let c = h.cell_mut(u).expect("present");
        let from = c.operands[slot as usize];
        c.operands[slot as usize] = ret_val;
        edits.push(Edit::Retarget { cell: u, slot, from, to: ret_val });
    }
    let summary = crate::text::render_cell(&h, call_id);
    h.slab[call_id.0 as usize] = None; // already unlisted from entry in step 1
    edits.push(Edit::RemoveCell {
        id: call_id,
        ledger: format!(
            "cfg-inlined '{}': {} regions grafted, {} params bound to caller args, ret via {}",
            callee_name,
            callee.regions.len() - 1,
            params.len(),
            ret_val
        ),
        summary,
    });
    Ok((h, notes, edits, st, true))
}


// ======================================================================
// Tests — red/green per pass, sabotage battery, op sanity
// ======================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Diamond with a CONST condition: entry br(true) -> t/el -> j;
    /// arms carry different consts; ret the phi. The answer is 1
    /// (then-arm), not 2.
    fn const_diamond() -> Fabric {
        let text = "fabric v0\n\
region entry\n\
  %0 = const i1 true\n\
  %1 = br %0, t, el\n\
region t\n\
  %2 = const i32 1\n\
  %3 = jump j\n\
region el\n\
  %4 = const i32 2\n\
  %5 = jump j\n\
region j\n\
  %6 = phi [t: %2] [el: %4]\n\
  %7 = ret %6\n";
        crate::text::parse(text).expect("const diamond parses")
    }

    /// A fabric with an UNREACHABLE region (nothing enters 'dead');
    /// reachable side is const-decidable, answer 7.
    fn with_unreachable() -> Fabric {
        let text = "fabric v0\n\
region entry\n\
  %0 = const i32 7\n\
  %1 = jump live\n\
region dead\n\
  %2 = const i32 99\n\
  %3 = jump live\n\
region live\n\
  %4 = phi [entry: %0] [dead: %2]\n\
  %5 = ret %4\n";
        crate::text::parse(text).expect("unreachable fabric parses")
    }

    /// Multi-region (diamond) callee + a call site with post-call uses.
    fn inline_prog() -> (Fabric, BTreeMap<String, Fabric>) {
        let main_text = "fabric v0\n\
region entry\n\
  %0 = const i32 10\n\
  %1 = const i32 32\n\
  %2 = call i32 pick %0, %1\n\
  %3 = const i32 5\n\
  %4 = arith.add i32 %2, %3\n\
  %5 = ret %4\n";
        let callee_text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = param i32\n\
  %2 = const i1 true\n\
  %3 = br %2, t, el\n\
region t\n\
  %4 = jump j\n\
region el\n\
  %5 = jump j\n\
region j\n\
  %6 = phi [t: %0] [el: %1]\n\
  %7 = ret %6\n";
        let mut funcs = BTreeMap::new();
        funcs.insert("pick".to_string(), crate::text::parse(callee_text).unwrap());
        (crate::text::parse(main_text).unwrap(), funcs)
    }

    // ---------------- interp ----------------

    #[test]
    fn interp_judges_decidable_diamonds() {
        let f = const_diamond();
        assert_eq!(interp(&f, &BTreeMap::new(), 10_000), Some(ConstVal::I32(1)));
        // param-dependent condition: unjudgeable
        let mut g = const_diamond();
        g.slab[0] = None;
        let mut p = g.cell_mut(CellId(1)).unwrap().clone();
        // swap the const for a param: rebuild by hand
        let _ = &mut p;
        let text = "fabric v0\n\
region entry\n\
  %0 = param i1\n\
  %1 = br %0, t, el\n\
region t\n\
  %2 = const i32 1\n\
  %3 = jump j\n\
region el\n\
  %4 = const i32 2\n\
  %5 = jump j\n\
region j\n\
  %6 = phi [t: %2] [el: %4]\n\
  %7 = ret %6\n";
        let h = crate::text::parse(text).unwrap();
        assert_eq!(interp(&h, &BTreeMap::new(), 10_000), None, "param cond must be unjudgeable");
    }

    #[test]
    fn interp_resolves_calls_through_consts() {
        let (main, funcs) = inline_prog();
        assert_eq!(
            interp(&main, &funcs, 10_000),
            Some(ConstVal::I32(15)),
            "pick(10,32) with true cond = 10; +5 = 15"
        );
    }

    // ---------------- drop_edge (the semmut surgery, factored) ----------------

    #[test]
    fn drop_edge_is_verify_legal_and_replayable() {
        let f = const_diamond();
        let (g, rec) = drop_edge(&f, RegionId(0), RegionId(2)).expect("drop the el arm");
        assert!(verify(&g).is_ok(), "verify green after the drop");
        // replay bit-identity: the edits alone reproduce the fabric
        let mut h = crate::diff::History::new();
        h.push(rec);
        let (stages, final_r) = crate::replay::replay(&f, &h).expect("replay");
        assert_eq!(stages.len(), 2);
        assert_eq!(final_r, g, "replay must reproduce drop_edge bit-identically");
        // the surgery happened: entry's branch is now a jump to t
        let term = g.region(RegionId(0)).unwrap().cells.last().copied().unwrap();
        assert!(matches!(&g.cell(term).unwrap().kind, CellKind::Jump { target } if *target == RegionId(1)));
        // j's phi KEEPS both joins: el still targets j (only joins
        // naming the BRANCH's region are stripped — el's join names el)
        let j_phi = g.region(RegionId(3)).unwrap().cells[0];
        match &g.cell(j_phi).unwrap().kind {
            CellKind::Phi { joins } => assert_eq!(joins, &vec![RegionId(1), RegionId(2)]),
            ref k => panic!("expected phi, got {:?}", k),
        }
    }

    #[test]
    fn drop_edge_zero_join_phi_collapses() {
        // 'el' has phi-less entry; make the JOIN region's phi single-
        // join by giving el no phi... instead: j's phi joins both arms;
        // dropping the t arm leaves one join (kept mux); construct a
        // fabric where the dropped arm is a phi's ONLY join: entry
        // branches to t twice... simplest: br(t, t)? V-legal (same
        // target twice) but drop_edge refuses. Instead: a second pred.
        let text = "fabric v0\n\
region entry\n\
  %0 = const i1 false\n\
  %1 = br %0, solo, out\n\
region solo\n\
  %2 = const i32 9\n\
  %3 = jump merge\n\
region out\n\
  %4 = ret %0\n\
region merge\n\
  %5 = phi [solo: %2]\n\
  %6 = ret %5\n";
        let f = crate::text::parse(text).expect("solo parses");
        assert!(verify(&f).is_ok());
        // dropping edge entry->solo: merge's phi would go zero-join;
        // nothing else ever enters merge, so verify would fail IF we
        // kept an empty phi. The strategy must collapse or keep legal.
        let (g, _rec) = drop_edge(&f, RegionId(0), RegionId(1)).expect("drop solo arm");
        assert!(verify(&g).is_ok(), "verify green despite zero-join phi");
    }

    // ---------------- pass A: const-branch fold ----------------

    #[test]
    fn fold_green_preserves_semantics_and_replays() {
        let f = const_diamond();
        let (g, rec, st) = const_branch_fold(&f).expect("fold");
        assert!(st.folded >= 1, "the const branch must fold");
        assert!(verify(&g).is_ok());
        // property oracle: answer unchanged
        assert_eq!(interp(&f, &BTreeMap::new(), 10_000), Some(ConstVal::I32(1)));
        assert_eq!(interp(&g, &BTreeMap::new(), 10_000), Some(ConstVal::I32(1)));
        // replay bit-identity over the whole pass
        let mut h = crate::diff::History::new();
        h.push(rec);
        let (_, final_r) = crate::replay::replay(&f, &h).expect("replay");
        assert_eq!(final_r, g, "fold must be replayable bit-identically");
        // the branch is gone: entry now ends in a jump
        let term = g.region(RegionId(0)).unwrap().cells.last().copied().unwrap();
        assert!(matches!(g.cell(term).unwrap().kind, CellKind::Jump { .. }));
    }

    #[test]
    fn fold_red_wrong_arm_flips_the_answer() {
        // SABOTAGE: fold to the DEAD arm (what a broken cond decode
        // would do). The oracle must fire.
        let f = const_diamond();
        let (g, _rec, _st) = const_branch_fold(&f).expect("fold");
        // sabotage by hand: retarget the folded jump to the other arm
        let term = g.region(RegionId(0)).unwrap().cells.last().copied().unwrap();
        let mut bad = g.clone();
        if let Some(c) = bad.cell_mut(term) {
            if let CellKind::Jump { target } = &mut c.kind {
                *target = RegionId(2); // el: the arm the program never took
            }
        }
        // verify still green (structure is fine) — the ORACLE must catch it
        assert!(verify(&bad).is_ok(), "sabotage is structurally legal");
        assert_ne!(
            interp(&bad, &BTreeMap::new(), 10_000),
            interp(&g, &BTreeMap::new(), 10_000),
            "the property oracle MUST fire on the wrong-arm fold"
        );
        assert_eq!(interp(&bad, &BTreeMap::new(), 10_000), Some(ConstVal::I32(2)));
    }

    #[test]
    fn fold_red_wrong_join_strip_breaks_verify() {
        // SABOTAGE: strip the join from the KEPT arm's phis (wrong
        // region's phi maintenance) — V06 must fire.
        let f = const_diamond();
        let mut bad = f.clone();
        // strip the t-join from j's phi while keeping the el edge
        let phi = CellId(6);
        if let Some(c) = bad.cell_mut(phi) {
            if let CellKind::Phi { joins } = &mut c.kind {
                joins.retain(|&j| j != RegionId(1));
            }
            // (operands length now mismatched — V05 fires too; either way red)
        }
        assert!(verify(&bad).is_err(), "wrong-arm join strip must fail verify");
    }

    // ---------------- pass B: region-DCE ----------------

    #[test]
    fn dce_green_removes_unreachable_preserves_semantics() {
        let f = with_unreachable();
        assert!(verify(&f).is_ok());
        let (g, rec, st) = region_dce(&f).expect("dce");
        assert_eq!(st.regions_removed, 1, "the dead region must go");
        assert_eq!(g.regions.len(), 2, "entry + live remain");
        assert!(verify(&g).is_ok());
        assert_eq!(interp(&f, &BTreeMap::new(), 10_000), Some(ConstVal::I32(7)));
        assert_eq!(interp(&g, &BTreeMap::new(), 10_000), Some(ConstVal::I32(7)));
        // the live phi's dead join was stripped (single mux remains)
        let phi = g.region(RegionId(1)).unwrap().cells[0];
        match &g.cell(phi).unwrap().kind {
            CellKind::Phi { joins } => assert_eq!(joins, &vec![RegionId(0)]),
            ref k => panic!("expected phi, got {:?}", k),
        }
        // RemoveCell edits carry ledger entries (conservation shape)
        assert!(rec.edits.iter().any(|e| matches!(e,
            Edit::RemoveCell { ledger, .. } if ledger.contains("region-dce"))));
    }

    #[test]
    fn dce_red_removing_reachable_is_refused() {
        let f = with_unreachable();
        // raw op: removing the LIVE region must be refused (targeted)
        let err = region_remove(&f, RegionId(2)).expect_err("live region is targeted by entry");
        assert!(err.contains("still targets"), "{}", err);
        // and the pass itself never removes reachable regions:
        let (g, _rec, st) = region_dce(&f).unwrap();
        assert_eq!(st.regions_removed, 1);
        assert!(g.region(RegionId(0)).is_some() && g.region(RegionId(1)).is_some());
    }

    #[test]
    fn dce_red_dead_join_left_behind_fails_verify() {
        // SABOTAGE: remove the dead region's cells+region but NOT the
        // live phi's join naming it — V06 must fire (the finding semmut
        //'s PhiOperandRebind-style silent-wrong-wire guard prevents).
        let f = with_unreachable();
        let mut bad = f.clone();
        // erase the dead region wholesale, keep the phi join
        bad.regions.remove(1);
        for id in (0..bad.slab.len() as u32).map(CellId).collect::<Vec<_>>() {
            if let Some(c) = bad.cell(id) {
                if c.region.0 == 1 {
                    bad.slab[id.0 as usize] = None;
                }
            }
        }
        // fix the operand's dangling reference so V01 doesn't fire
        // first: rebind phi operand %2 (now dangling) to %0
        if let Some(c) = bad.cell_mut(CellId(4)) {
            c.operands[1] = CellId(0);
        }
        // region ids shifted: entry=0, live=1; phi joins say [0, 1(dead)]
        // '1' now names live itself — V06 fires (entry no longer targets live via dead)
        assert!(verify(&bad).is_err(), "stale joins must fail verify");
    }

    // ---------------- pass C: CFG-graft inline ----------------

    #[test]
    fn inline_green_multi_region_callee_verify_and_semantics() {
        let (f, funcs) = inline_prog();
        assert!(crate::program::verify_program(&crate::program::Program {
            order: vec!["main".into(), "pick".into()],
            funcs: {
                let mut m = BTreeMap::new();
                m.insert("main".into(), f.clone());
                for (k, v) in &funcs {
                    m.insert(k.clone(), v.clone());
                }
                m
            },
        })
        .is_ok());
        let (g, rec, st) = cfg_graft_inline(&f, &funcs).expect("inline");
        assert_eq!(st.inlined, 1, "the diamond call must inline");
        assert_eq!(st.regions_grafted, 3, "callee t/el/j graft as fresh regions");
        assert!(verify(&g).is_ok(), "verify green after the CFG graft");
        // property oracle: pick(10,32)=10 with true cond; +5 = 15 before and after
        assert_eq!(interp(&f, &funcs, 10_000), Some(ConstVal::I32(15)));
        assert_eq!(interp(&g, &BTreeMap::new(), 10_000), Some(ConstVal::I32(15)));
        // the call is gone; no call cells anywhere
        assert!(g.cells().all(|id| !matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Call { .. }))));
        // provenance now crosses the graft (the payoff v1 could not
        // reach): the ret (moved to the continuation) walks through the
        // return phi into the grafted diamond, down to caller consts
        let ret = g
            .cells()
            .find(|&id| matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Ret)))
            .expect("the ret survives in the continuation");
        let ret_op = g.cell(ret).unwrap().operands[0];
        let walk = crate::prov::render(&crate::prov::provenance(&g, ret_op).unwrap());
        assert!(walk.contains("const i32 10"), "caller arg 10 reached through the graft: {}", walk);
        assert!(walk.contains("const i32 5"), "post-call const 5 reached: {}", walk);
        assert!(!walk.contains("call"), "no call leaves: {}", walk);
        // conservation-shaped ledger on the removed call
        assert!(rec.edits.iter().any(|e| matches!(e,
            Edit::RemoveCell { ledger, .. } if ledger.contains("cfg-inlined 'pick'"))));
    }

    #[test]
    fn inline_red_misbound_phi_join_fails_verify() {
        // SABOTAGE the graft: relabel the continuation's return-phi
        // join to a region that is not a predecessor — V06 must fire.
        let (f, funcs) = inline_prog();
        let (g, _rec, st) = cfg_graft_inline(&f, &funcs).unwrap();
        assert_eq!(st.inlined, 1);
        // find the return phi (in the continuation region)
        let cont = g
            .cells()
            .find(|&id| matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Phi { .. })))
            .expect("return phi exists");
        let mut bad = g.clone();
        let region = bad.cell(cont).unwrap().region;
        let other = (0..bad.regions.len() as u32)
            .map(RegionId)
            .find(|r| *r != region && !bad.predecessors(region).contains(r))
            .expect("a non-predecessor region exists");
        if let Some(c) = bad.cell_mut(cont) {
            if let CellKind::Phi { joins } = &mut c.kind {
                joins[0] = other;
            }
        }
        assert!(verify(&bad).is_err(), "misbound join must fail verify");
    }

    #[test]
    fn inline_red_wrong_value_retarget_flips_answer() {
        // SABOTAGE: retarget the post-call use to the WRONG argument
        // (arg 1 instead of the selected arg 0) — the oracle fires.
        let (f, funcs) = inline_prog();
        let (g, _rec, _st) = cfg_graft_inline(&f, &funcs).unwrap();
        // find the post-call add (the use of the call value)
        let add = g
            .cells()
            .find(|&id| matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Arith { .. })))
            .expect("the add survives the graft");
        let mut bad = g.clone();
        // rebind its slot-0 to const 32 (the not-taken arm's value)
        let c32 = bad
            .cells()
            .find(|&id| matches!(bad.cell(id).map(|c| &c.kind), Some(CellKind::Const { val: ConstVal::I32(32), .. })))
            .expect("const 32 present");
        if let Some(c) = bad.cell_mut(add) {
            c.operands[0] = c32;
        }
        assert!(verify(&bad).is_ok(), "sabotage is structurally legal");
        assert_eq!(interp(&bad, &BTreeMap::new(), 10_000), Some(ConstVal::I32(37)), "32+5");
        assert_ne!(
            interp(&bad, &BTreeMap::new(), 10_000),
            interp(&g, &BTreeMap::new(), 10_000),
            "the property oracle MUST fire on the misbound value"
        );
    }

    #[test]
    fn inline_skips_are_noted_never_silent() {
        // cyclic-entry callee: skipped with a note, fabric untouched
        let (f, _) = inline_prog();
        let cyc = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = param i32\n\
  %2 = const i1 true\n\
  %3 = br %2, t, el\n\
region t\n\
  %4 = jump j\n\
region el\n\
  %5 = jump j\n\
region j\n\
  %6 = phi [t: %0] [el: %1]\n\
  %7 = br %2, entry, j\n";
        // (entry gains a predecessor via j->entry: cyclic)
        let _ = cyc; // cyclic callee cannot even parse+verify: use void ret instead
        let void_callee = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = param i32\n\
  %2 = ret\n";
        let mut funcs = BTreeMap::new();
        funcs.insert("pick".to_string(), crate::text::parse(void_callee).unwrap());
        let (g, rec, st) = cfg_graft_inline(&f, &funcs).expect("inline runs");
        assert_eq!(st.inlined, 0);
        assert_eq!(st.skipped, 1);
        assert_eq!(g, f, "skip leaves the fabric untouched");
        assert!(rec.notes.iter().any(|n| n.contains("void")), "skip note: {:?}", rec.notes);
    }

    // ---------------- join_phi + region_add sanity ----------------

    #[test]
    fn join_phi_adds_the_missing_mux_input() {
        // add a new edge entry2 -> live; live's phi must gain a join
        let f = with_unreachable();
        let (mut g, new_r, _rec) = region_add(&f, "entry2");
        let val = {
            let v = g.add_cell(new_r, Cell::new(new_r, CellKind::Const { ty: Type::I32, val: ConstVal::I32(11) }));
            g.add_cell(new_r, Cell::new(new_r, CellKind::Jump { target: RegionId(2) }));
            v
        };
        assert!(verify(&g).is_err(), "V16 fires: new pred without a mux input");
        let (g2, _rec2) = join_phi(&g, CellId(4), new_r, val).expect("join added");
        assert!(verify(&g2).is_ok(), "join completes the mux");
        // and the phi picks the new value on the new path
        assert_eq!(interp(&g2, &BTreeMap::new(), 10_000), Some(ConstVal::I32(7)));
    }

    #[test]
    fn region_remove_standalone_refuses_referenced_regions() {
        let f = const_diamond();
        // 't' is targeted by entry's branch: refused
        let err = region_remove(&f, RegionId(1)).expect_err("t is targeted");
        assert!(err.contains("still targets"), "{}", err);
        // entry itself: refused
        let err = region_remove(&f, RegionId(0)).expect_err("entry never removable");
        assert!(err.contains("entry"), "{}", err);
    }

    // ---------------- GA-bred fabric end-to-end (the real-material leg) ----
    // The full bred-corpus measurements live in bin/region-spike; this
    // pins that a GA-BRED fabric (not a hand fixture) survives each
    // pass verify-green, deterministic seed.

    fn bred_const_branch_fabric() -> Fabric {
        // breed deterministically until a fabric with a const-cond
        // branch appears (mut_grow's jmp->br arm mints exactly those)
        let mut rng = crate::fuzz::Rng::new(0x5EED1);
        for _ in 0..5_000 {
            let f = crate::fuzz::gen_fabric(&mut rng);
            let mut g = crate::ga::mutate_breed(&f, &mut rng);
            for _ in 0..3 {
                g = crate::ga::mutate_breed(&g, &mut rng);
            }
            if verify(&g).is_err() {
                continue;
            }
            let has_const_br = g.regions.iter().enumerate().any(|(i, r)| {
                r.cells.last().and_then(|&t| g.cell(t)).map(|c| {
                    matches!(&c.kind, CellKind::Branch { .. })
                        && matches!(
                            c.operands.first().and_then(|&o| g.cell(o)).map(|cc| &cc.kind),
                            Some(CellKind::Const { .. })
                        )
                }).unwrap_or(false) && g.successors(RegionId(i as u32)).len() == 2
            });
            if has_const_br {
                return g;
            }
        }
        panic!("GA never bred a const-branch fabric in 5000 tries (material claim failed)");
    }

    #[test]
    fn bred_fabric_survives_fold_and_dce() {
        let f = bred_const_branch_fabric();
        assert!(verify(&f).is_ok());
        // fold: verify green, semantics preserved wherever decidable
        let (g1, rec1, st1) = const_branch_fold(&f).expect("fold on bred fabric");
        assert!(verify(&g1).is_ok());
        assert!(st1.const_conditioned >= 1, "bred fabric had const-conditioned branches");
        let (a, b) = (interp(&f, &BTreeMap::new(), 100_000), interp(&g1, &BTreeMap::new(), 100_000));
        assert_eq!(a, b, "semantics preserved (decidable or jointly unjudgeable)");
        // replay bit-identity for the fold
        let mut h = crate::diff::History::new();
        h.push(rec1);
        let (_, final_r) = crate::replay::replay(&f, &h).expect("replay");
        assert_eq!(final_r, g1);
        // then DCE whatever the fold stranded, compose green
        let (g2, _rec2, st2) = region_dce(&g1).expect("dce after fold");
        assert!(verify(&g2).is_ok());
        assert_eq!(interp(&g2, &BTreeMap::new(), 100_000), b, "dce preserves semantics");
        let _ = st2;
    }
}

/// Replace entry params with small consts (the interp twin trick):
/// yields a decidable fabric for the property oracle without touching
/// the fabric under test. Verify stays green (Param -> Const of the
/// same type; V12 holds).
pub fn constify(f: &Fabric) -> Fabric {
    let mut g = f.clone();
    let entry = match g.entry() {
        Some(e) => e,
        None => return g,
    };
    let ids: Vec<CellId> = g.region(entry).map(|r| r.cells.clone()).unwrap_or_default();
    let mut i = 0u32;
    for id in ids {
        if let Some(c) = g.cell(id) {
            if let CellKind::Param { ty } = &c.kind {
                let v = i.wrapping_mul(3).wrapping_add(1);
                let val = match ty {
                    Type::I1 => ConstVal::I1(v % 2 == 0),
                    Type::I32 => ConstVal::I32(v as i32),
                    Type::I64 => ConstVal::I64(v as i64),
                    Type::F64 => ConstVal::F64(v as f64 / 4.0),
                };
                let mut cell = c.clone();
                cell.kind = CellKind::Const { ty: *ty, val };
                g.slab[id.0 as usize] = Some(cell);
                i += 1;
            }
        }
    }
    g
}

#[cfg(test)]
mod materialize_tests {
    use super::*;

    /// The materialize path: a phi whose only join is the dropped
    /// edge's source (a NON-ENTRY region) and whose operand is a CONST
    /// defined there — not legally usable by the phi's users (V12),
    /// but copyable: the phi becomes a const copy in its own region.
    /// Verify green, semantics preserved, edit stream replayable.
    #[test]
    fn drop_edge_materializes_cross_region_const_operand() {
        // entry -> p ; p brs to taken/dead; dead's phi joins ONLY on p
        // and takes a CONST defined in p.
        let text = "fabric v0\n\
region entry\n\
  %0 = const i1 true\n\
  %1 = jump p\n\
region p\n\
  %2 = const i1 true\n\
  %3 = const i64 42i64\n\
  %4 = br %2, taken, dead\n\
region taken\n\
  %5 = ret %0\n\
region dead\n\
  %6 = phi [p: %3]\n\
  %7 = arith.sub i64 %6, %6\n\
  %8 = ret %7\n";
        let f = crate::text::parse(text).expect("parses");
        assert!(verify(&f).is_ok());
        let (g, rec) = drop_edge(&f, RegionId(1), RegionId(3)).expect("drop the dead arm");
        assert!(verify(&g).is_ok());
        // the phi is GONE, replaced by a materialized const in dead
        assert!(!g.cells().any(|id| matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Phi { .. }))));
        assert!(rec.notes.iter().any(|n| n.contains("materialized as const")), "{:?}", rec.notes);
        // replayable: the edits alone reproduce the fabric
        let mut h = crate::diff::History::new();
        h.push(rec);
        let (_, fr) = crate::replay::replay(&f, &h).expect("replay");
        assert_eq!(fr, g);
        // semantics: the program takes the taken arm (ret %0 = true);
        // after the drop it still does — and the dead arm's own
        // materialized body would answer 0, but it never runs
        assert_eq!(interp(&f, &BTreeMap::new(), 1_000), Some(ConstVal::I1(true)));
        assert_eq!(interp(&g, &BTreeMap::new(), 1_000), Some(ConstVal::I1(true)));
    }

    /// The refusal path: only join is the dropped source and the
    /// operand is a NON-CONST cell defined in that (non-entry) region —
    /// no legal rewrite exists (V12 bans the cross-region use, no
    /// const to copy); the op must refuse. This is semmut's V05/V06
    /// failure class, now diagnosed and named.
    #[test]
    fn drop_edge_refuses_uncollapsible_nonconst_single_join() {
        let text = "fabric v0\n\
region entry\n\
  %0 = const i1 true\n\
  %1 = jump p\n\
region p\n\
  %2 = const i1 false\n\
  %3 = arith.add i1 %2, %2\n\
  %4 = br %2, taken, dead\n\
region taken\n\
  %5 = ret %0\n\
region dead\n\
  %6 = phi [p: %3]\n\
  %7 = ret %6\n";
        let f = crate::text::parse(text).expect("parses");
        assert!(verify(&f).is_ok());
        let err = drop_edge(&f, RegionId(1), RegionId(3)).expect_err("non-const operand cannot legally replace the phi");
        assert!(err.contains("cannot legally replace"), "{}", err);
    }
}

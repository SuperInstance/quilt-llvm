//! Replay: fold a history's machine-applicable edits over the original
//! fabric to reproduce every intermediate fabric, bit-identically
//! (structural equality — PartialEq — plus canonical text).
//!
//! Replay validates as it applies: a forged or corrupted edit fails with
//! a precise error instead of producing a plausible-but-wrong fabric.
//! That is the N4 payoff: history is not just a log, it is checkable.

use crate::cell::{Cell, CellKind};
use crate::diff::{Edit, History};
use crate::fabric::Fabric;
use crate::id::CellId;

/// Apply one edit. Order matters: AddCell ids must be the next free slab
/// slot; RemoveCell must find the cell present; Retarget must find the
/// `from` operand in place. All three go through the sanctioned fabric
/// mutators, so replayed fabrics carry maintained use/pred tables too
/// (the derivability law holds on every replayed stage — tested).
pub fn apply_edit(f: &mut Fabric, e: &Edit) -> Result<(), String> {
    match e {
        Edit::AddCell { id, index, cell } => {
            if id.0 as usize != f.slab.len() {
                return Err(format!(
                    "AddCell {} is not the next free id ({}) — history is forged or out of order",
                    id,
                    f.slab.len()
                ));
            }
            place(f, *id, *index, cell.clone())?;
            Ok(())
        }
        Edit::RemoveCell { id, ledger, .. } => {
            // M4.1 (tit-quilt retrofit): a parseable death certificate
            // carries a tombstone — the ledger line IS the graveyard's
            // carrier, so replay rebuilds it bit-identically from the
            // ledger alone. Non-certificate removals (constfold folds,
            // the old dce prose) tombstone nothing.
            let had = f.remove_cell(*id).is_some();
            if !had {
                return Err(format!("RemoveCell {}: no such cell present", id));
            }
            if let Some(cert) = crate::decay::DeathCert::parse(ledger, *id) {
                f.tombstones.push(cert.tombstone());
            }
            Ok(())
        }
        Edit::Retarget { cell, slot, from, to } => {
            let c = f.cell(*cell).ok_or_else(|| format!("Retarget: {} not present", cell))?;
            let s = *slot as usize;
            let got = *c
                .operands
                .get(s)
                .ok_or_else(|| format!("Retarget: {} has no slot {}", cell, slot))?;
            if got != *from {
                return Err(format!(
                    "Retarget: {}.{} is {} but history says {} — history does not match the fabric",
                    cell, slot, got, from
                ));
            }
            f.retarget(*cell, *slot, *to).ok_or_else(|| format!("Retarget: {} not present", cell))?;
            Ok(())
        }
        Edit::RegionAdded { id, name } => {
            if id.0 as usize != f.regions.len() {
                return Err(format!(
                    "RegionAdded {} is not the next region id ({}) — history is forged or out of order",
                    id,
                    f.regions.len()
                ));
            }
            f.add_region(name.clone());
            Ok(())
        }
        Edit::RegionRemoved { id, name } => {
            match f.region(*id).map(|r| r.name.clone()) {
                None => {
                    return Err(format!("RegionRemoved {}: no such region", id));
                }
                Some(n) if n != *name => {
                    return Err(format!(
                        "RegionRemoved {}: region is named {:?} but history says {:?} — history does not match the fabric",
                        id, n, name
                    ));
                }
                _ => {}
            }
            if f.region(*id).map(|r| !r.cells.is_empty()).unwrap_or(true) {
                return Err(format!(
                    "RegionRemoved {}: region still lists cells — cells ride their own RemoveCell edits first",
                    id
                ));
            }
            f.remove_region(*id)
                .map(|_| ())
                .ok_or_else(|| format!("RegionRemoved {}: removal failed", id))
        }
        Edit::MoveCell { id, from, to, index } => {
            let cur = f
                .cell(*id)
                .map(|c| c.region)
                .ok_or_else(|| format!("MoveCell: {} not present", id))?;
            if cur != *from {
                return Err(format!(
                    "MoveCell: {} lives in region {} but history says {} — history does not match the fabric",
                    id, cur, from
                ));
            }
            if f.region(*to).is_none() {
                return Err(format!("MoveCell: destination region {} does not exist", to));
            }
            f.move_cell(*id, *to, *index)
                .map(|_| ())
                .ok_or_else(|| format!("MoveCell {}: move failed", id))
        }
        Edit::RelabelJoin { phi, from, to } => {
            if f.relabel_join(*phi, *from, *to).is_none() {
                return match f.cell(*phi).map(|c| &c.kind) {
                    None => Err(format!("RelabelJoin: phi {} not present", phi)),
                    Some(CellKind::Phi { joins }) if !joins.contains(from) => Err(format!(
                        "RelabelJoin: phi {} has no join on {} — history does not match the fabric",
                        phi, from
                    )),
                    _ => Err(format!("RelabelJoin: {} is not a phi", phi)),
                };
            }
            Ok(())
        }
    }
}

fn place(f: &mut Fabric, id: CellId, index: usize, cell: Cell) -> Result<(), String> {
    if f.regions.get(cell.region.0 as usize).is_none() {
        return Err(format!("AddCell {}: region {} does not exist", id, cell.region));
    }
    f.slab.push(Some(cell.clone()));
    let r = &mut f.regions[cell.region.0 as usize];
    let idx = index.min(r.cells.len());
    r.cells.insert(idx, id);
    f.register_cell(id, idx);
    Ok(())
}

/// Replay a whole history over a starting fabric. Returns every
/// intermediate fabric after each record (index 0 = the original), plus
/// the final fabric.
pub fn replay(f0: &Fabric, history: &History) -> Result<(Vec<Fabric>, Fabric), String> {
    let mut stages = vec![f0.clone()];
    let mut cur = f0.clone();
    for rec in &history.records {
        for e in &rec.edits {
            apply_edit(&mut cur, e)?;
        }
        stages.push(cur.clone());
    }
    Ok((stages, cur))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, CellKind};
    use crate::diff::DiffRecord;
    use crate::id::RegionId;
    use crate::ty::{ConstVal, Type};

    fn base() -> Fabric {
        // entry: %0 = const i32 2 ; %1 = ret
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(2) }));
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![CellId(0)];
        f.add_cell(e, r);
        f
    }

    fn add_const_edit(f: &mut Fabric, region: RegionId, v: i32) -> Edit {
        let id = CellId(f.slab.len() as u32);
        let cell = Cell::new(region, CellKind::Const { ty: Type::I32, val: ConstVal::I32(v) });
        let index = f.regions[region.0 as usize].cells.len() - 1; // before ret
        place(f, id, index, cell.clone()).unwrap();
        Edit::AddCell { id, index, cell }
    }

    #[test]
    fn replay_reproduces_single_add() {
        let f0 = base();
        let mut f = f0.clone();
        let e = f.entry().unwrap();
        let edit = add_const_edit(&mut f, e, 7);
        let mut rec = DiffRecord::new("test");
        rec.edits.push(edit);
        let mut h = History::new();
        h.push(rec);
        let (stages, final_f) = replay(&f0, &h).unwrap();
        assert_eq!(stages.len(), 2);
        assert_eq!(final_f, f, "replay must reproduce the edited fabric structurally");
    }

    #[test]
    fn forged_id_is_rejected() {
        let f0 = base();
        let mut f = f0.clone();
        let e = f.entry().unwrap();
        let edit = add_const_edit(&mut f, e, 7);
        let mut forged = edit.clone();
        if let Edit::AddCell { id, .. } = &mut forged {
            id.0 = 99; // not the next free id
        }
        let mut rec = DiffRecord::new("forged");
        rec.edits.push(forged);
        let mut h = History::new();
        h.push(rec);
        let err = replay(&f0, &h).unwrap_err();
        assert!(err.contains("forged"), "{}", err);
    }

    #[test]
    fn tampered_retarget_is_rejected() {
        let f0 = base();
        let mut f = f0.clone();
        let e = f.entry().unwrap();
        let add = add_const_edit(&mut f, e, 7);
        let new_id = match &add {
            Edit::AddCell { id, .. } => *id,
            _ => unreachable!(),
        };
        // retarget ret's operand from %0 to the new const
        let rt = Edit::Retarget { cell: CellId(1), slot: 0, from: CellId(0), to: new_id };
        f.cell_mut(CellId(1)).unwrap().operands[0] = new_id;
        let mut rec = DiffRecord::new("t");
        rec.edits.push(add);
        rec.edits.push(rt.clone());
        let mut h = History::new();
        h.push(rec);
        // untampered replay works and reproduces
        let (_, final_f) = replay(&f0, &h).unwrap();
        assert_eq!(final_f, f);
        // tamper: claim the retarget came from a different cell
        let mut bad = History::new();
        let mut rec2 = DiffRecord::new("t");
        rec2.edits.push(Edit::Retarget { cell: CellId(1), slot: 0, from: CellId(9), to: new_id });
        bad.push(rec2);
        let err = replay(&f0, &bad).unwrap_err();
        assert!(err.contains("does not match"), "{}", err);
    }
}

#[cfg(test)]
mod region_edit_tests {
    //! The R3 region-granular vocabulary (RegionAdded / RegionRemoved /
    //! MoveCell / RelabelJoin): replay round-trips, forged edits are
    //! rejected naming the lie, and the replayed stages keep the use
    //! tables derivable (the R2 law extended to region edits).
    use super::*;
    use crate::cell::CellKind;
    use crate::diff::DiffRecord;
    use crate::id::RegionId;
    use crate::usetables::UseTables;
    use crate::verify::verify;

    /// entry -> live ; 'dead' is unreachable, referenced by live's phi.
    /// The exact shape region_dce eats (region.rs fixture `with_unreachable`).
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
        crate::text::parse(text).expect("parses")
    }

    /// The region-dce-shaped stream: dead cells out (ledgered), the
    /// live phi rebuilt without the dead join, the region removed with
    /// id compaction. Every edit goes through `apply_edit`; the final
    /// fabric must equal an independently-built twin bit-identically.
    #[test]
    fn region_removed_stream_replays_bit_identically() {
        let f0 = with_unreachable();
        assert!(verify(&f0).is_ok());

        let mut rec = DiffRecord::new("region-dce");
        rec.edits.push(Edit::RemoveCell {
            id: CellId(2),
            ledger: "region-dce: region unreachable from entry".into(),
            summary: "%2 = const i32 99".into(),
        });
        rec.edits.push(Edit::RemoveCell {
            id: CellId(3),
            ledger: "region-dce: region unreachable from entry".into(),
            summary: "%3 = jump live".into(),
        });
        rec.edits.push(Edit::RemoveCell {
            id: CellId(4),
            ledger: "region-dce: phi replaced (join 'dead' stripped)".into(),
            summary: "%4 = phi [entry: %0] [dead: %2]".into(),
        });
        let mut phi = Cell::new(RegionId(2), CellKind::Phi { joins: vec![RegionId(0)] });
        phi.operands = vec![CellId(0)];
        rec.edits.push(Edit::AddCell { id: CellId(6), index: 0, cell: phi });
        rec.edits.push(Edit::Retarget { cell: CellId(5), slot: 0, from: CellId(4), to: CellId(6) });
        rec.edits.push(Edit::RegionRemoved { id: RegionId(1), name: "dead".into() });

        let mut h = History::new();
        h.push(rec);
        let (stages, final_f) = replay(&f0, &h).expect("replay");

        // the twin, built through the sanctioned mutators directly
        let mut twin = f0.clone();
        twin.remove_cell(CellId(2));
        twin.remove_cell(CellId(3));
        twin.remove_cell(CellId(4));
        let mut new_phi = Cell::new(RegionId(2), CellKind::Phi { joins: vec![RegionId(0)] });
        new_phi.operands = vec![CellId(0)];
        twin.place_cell(CellId(6), new_phi).expect("place");
        twin.move_cell(CellId(6), RegionId(2), 0).expect("phi keeps its position (index 0)");
        twin.retarget(CellId(5), 0, CellId(6)).expect("retarget");
        let gone = twin.remove_region(RegionId(1)).expect("region leaves");
        assert_eq!(gone.name, "dead");
        assert_eq!(gone.cells, Vec::<CellId>::new(), "empty region removed");

        assert_eq!(final_f, twin, "replay must reproduce the region-edited fabric");
        assert_eq!(final_f.regions.len(), 2);
        assert_eq!(final_f.region_name(RegionId(1)), "live", "ids compacted: live 2 -> 1");
        let jump = final_f.region(RegionId(0)).unwrap().cells.last().copied().unwrap();
        assert!(matches!(final_f.cell(jump).map(|c| &c.kind), Some(CellKind::Jump { target }) if *target == RegionId(1)));
        assert!(verify(&final_f).is_ok(), "the replayed fabric verifies (tables intact)");
        for st in &stages {
            assert_eq!(UseTables::derive(st), st.tables, "R2 law: region edits keep tables derivable");
        }
    }

    #[test]
    fn region_added_appends_deterministically() {
        let f0 = with_unreachable();
        let mut rec = DiffRecord::new("t");
        rec.edits.push(Edit::RegionAdded { id: RegionId(3), name: "cont".into() });
        let mut h = History::new();
        h.push(rec);
        let (_, final_f) = replay(&f0, &h).expect("replay");
        assert_eq!(final_f.regions.len(), 4);
        assert_eq!(final_f.region_name(RegionId(3)), "cont");
        assert_eq!(UseTables::derive(&final_f), final_f.tables);
    }

    #[test]
    fn move_cell_and_relabel_join_replay() {
        // move entry's jump into a fresh continuation, then relabel
        // live's phi join entry -> cont (the same edge, new source)
        let f0 = with_unreachable();
        let mut rec = DiffRecord::new("t");
        rec.edits.push(Edit::RegionAdded { id: RegionId(3), name: "cont".into() });
        rec.edits.push(Edit::MoveCell { id: CellId(1), from: RegionId(0), to: RegionId(3), index: 0 });
        rec.edits.push(Edit::RelabelJoin { phi: CellId(4), from: RegionId(0), to: RegionId(3) });
        let mut h = History::new();
        h.push(rec);
        let (stages, final_f) = replay(&f0, &h).expect("replay");
        assert_eq!(final_f.cell(CellId(1)).unwrap().region, RegionId(3));
        assert_eq!(final_f.region(RegionId(3)).unwrap().cells, vec![CellId(1)]);
        match final_f.cell(CellId(4)).unwrap().kind {
            CellKind::Phi { ref joins } => assert!(joins.contains(&RegionId(3)) && !joins.contains(&RegionId(0))),
            ref k => panic!("phi kept: {:?}", k),
        }
        for st in &stages {
            assert_eq!(UseTables::derive(st), st.tables);
        }
    }

    // ---- forged / mismatched edits are rejected, naming the lie ----

    fn one_edit_history(e: Edit) -> History {
        let mut rec = DiffRecord::new("forged");
        rec.edits.push(e);
        let mut h = History::new();
        h.push(rec);
        h
    }

    #[test]
    fn forged_region_removed_nonempty_is_rejected() {
        let f0 = with_unreachable();
        let h = one_edit_history(Edit::RegionRemoved { id: RegionId(1), name: "dead".into() });
        let err = replay(&f0, &h).unwrap_err();
        assert!(err.contains("still lists cells"), "{}", err);
    }

    #[test]
    fn forged_region_removed_wrong_name_is_rejected() {
        let f0 = with_unreachable();
        let h = one_edit_history(Edit::RegionRemoved { id: RegionId(1), name: "not-dead".into() });
        let err = replay(&f0, &h).unwrap_err();
        assert!(err.contains("history says"), "{}", err);
    }

    #[test]
    fn forged_region_removed_absent_region_is_rejected() {
        let f0 = with_unreachable();
        let h = one_edit_history(Edit::RegionRemoved { id: RegionId(9), name: "dead".into() });
        let err = replay(&f0, &h).unwrap_err();
        assert!(err.contains("no such region"), "{}", err);
    }

    #[test]
    fn forged_region_added_out_of_order_is_rejected() {
        let f0 = with_unreachable();
        let h = one_edit_history(Edit::RegionAdded { id: RegionId(5), name: "x".into() });
        let err = replay(&f0, &h).unwrap_err();
        assert!(err.contains("forged or out of order"), "{}", err);
    }

    #[test]
    fn forged_move_cell_wrong_from_is_rejected() {
        let f0 = with_unreachable();
        let h = one_edit_history(Edit::MoveCell {
            id: CellId(0),
            from: RegionId(1), // %0 lives in entry (0), not dead
            to: RegionId(2),
            index: 0,
        });
        let err = replay(&f0, &h).unwrap_err();
        assert!(err.contains("history does not match the fabric"), "{}", err);
    }

    #[test]
    fn forged_move_cell_missing_destination_is_rejected() {
        let f0 = with_unreachable();
        let h = one_edit_history(Edit::MoveCell {
            id: CellId(0),
            from: RegionId(0),
            to: RegionId(7),
            index: 0,
        });
        let err = replay(&f0, &h).unwrap_err();
        assert!(err.contains("does not exist"), "{}", err);
    }

    #[test]
    fn forged_relabel_join_missing_join_is_rejected() {
        let f0 = with_unreachable();
        let h = one_edit_history(Edit::RelabelJoin { phi: CellId(4), from: RegionId(2), to: RegionId(0) });
        let err = replay(&f0, &h).unwrap_err();
        assert!(err.contains("no join on"), "{}", err);
    }
}

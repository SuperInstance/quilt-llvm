//! Replay: fold a history's machine-applicable edits over the original
//! fabric to reproduce every intermediate fabric, bit-identically
//! (structural equality — PartialEq — plus canonical text).
//!
//! Replay validates as it applies: a forged or corrupted edit fails with
//! a precise error instead of producing a plausible-but-wrong fabric.
//! That is the N4 payoff: history is not just a log, it is checkable.

use crate::cell::Cell;
use crate::diff::{Edit, History};
use crate::fabric::Fabric;
use crate::id::CellId;

/// Apply one edit. Order matters: AddCell ids must be the next free slab
/// slot; RemoveCell must find the cell present; Retarget must find the
/// `from` operand in place.
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
        Edit::RemoveCell { id, .. } => {
            let cell = f.cell(*id).ok_or_else(|| format!("RemoveCell {}: no such cell present", id))?;
            let region = cell.region;
            let cells = &mut f.regions[region.0 as usize].cells;
            let pos = cells
                .iter()
                .position(|&c| c == *id)
                .ok_or_else(|| format!("RemoveCell {}: not listed in its region", id))?;
            cells.remove(pos);
            f.slab[id.0 as usize] = None;
            Ok(())
        }
        Edit::Retarget { cell, slot, from, to } => {
            let c = f.cell_mut(*cell).ok_or_else(|| format!("Retarget: {} not present", cell))?;
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
            c.operands[s] = *to;
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

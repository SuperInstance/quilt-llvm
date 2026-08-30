//! Conservation law, made mechanical:
//!
//! every value admitted into a transform is either delivered or explicitly
//! dropped-with-ledger-entry — never silently vanishes.
//!
//! check(before, after, diff) fails naming any cell that vanished or
//! appeared without a matching ledger/edit entry.

use crate::diff::{DiffRecord, Edit, History};
use crate::fabric::Fabric;
use crate::id::CellId;

pub fn check(before: &Fabric, after: &Fabric, rec: &DiffRecord) -> Result<(), String> {
    // Empty ledger entries do not count as conservation.
    for e in &rec.edits {
        if let Edit::RemoveCell { id, ledger, .. } = e {
            if ledger.trim().is_empty() {
                return Err(format!("conservation violated: {} removed with a blank ledger entry", id));
            }
        }
    }
    let removed: Vec<CellId> = rec
        .edits
        .iter()
        .filter_map(|e| match e {
            Edit::RemoveCell { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    let added: Vec<CellId> = rec
        .edits
        .iter()
        .filter_map(|e| match e {
            Edit::AddCell { id, .. } => Some(*id),
            _ => None,
        })
        .collect();

    let before_ids: Vec<CellId> = before.cells().collect();
    let after_ids: Vec<CellId> = after.cells().collect();

    for id in &before_ids {
        if after.cell(*id).is_none() && !removed.contains(id) {
            return Err(format!("conservation violated: {} vanished without a ledger entry", id));
        }
    }
    for id in &after_ids {
        if before.cell(*id).is_none() && !added.contains(id) {
            return Err(format!("conservation violated: {} appeared without an AddCell edit", id));
        }
    }
    Ok(())
}

/// Same law over a whole pipeline history (first fabric vs last fabric).
pub fn check_pipeline(before: &Fabric, after: &Fabric, history: &History) -> Result<(), String> {
    for id in before.cells() {
        if after.cell(id).is_none() && !history_removed_contains(history, id) {
            return Err(format!("conservation violated: {} vanished across pipeline", id));
        }
    }
    for id in after.cells() {
        if before.cell(id).is_none() && !history_added_contains(history, id) {
            return Err(format!("conservation violated: {} appeared across pipeline", id));
        }
    }
    Ok(())
}

fn history_removed_contains(h: &History, id: CellId) -> bool {
    h.records.iter().any(|r| {
        r.edits.iter().any(|e| match e {
            Edit::RemoveCell { id: i, ledger, .. } => *i == id && !ledger.trim().is_empty(),
            _ => false,
        })
    })
}

fn history_added_contains(h: &History, id: CellId) -> bool {
    h.records.iter().any(|r| {
        r.edits.iter().any(|e| match e {
            Edit::AddCell { id: i, .. } => *i == id,
            _ => false,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, CellKind};
    use crate::ty::{ConstVal, Type};

    fn one_const() -> Fabric {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        f.add_cell(e, Cell::new(e, CellKind::Ret));
        f
    }

    #[test]
    fn identical_fabrics_conserve() {
        let f = one_const();
        let rec = DiffRecord::new("noop");
        assert!(check(&f, &f, &rec).is_ok());
    }

    #[test]
    fn vanish_without_ledger_is_caught() {
        let before = one_const();
        let mut after = before.clone();
        after.slab[0] = None; // silent vanish
        after.regions[0].cells.retain(|&c| c != CellId(0));
        let rec = DiffRecord::new("badpass");
        let err = check(&before, &after, &rec).unwrap_err();
        assert!(err.contains("vanish"), "{}", err);
    }

    #[test]
    fn ledgered_removal_conerves() {
        let before = one_const();
        let mut after = before.clone();
        after.slab[0] = None;
        after.regions[0].cells.retain(|&c| c != CellId(0));
        let mut rec = DiffRecord::new("goodpass");
        rec.edits.push(Edit::RemoveCell {
            id: CellId(0),
            ledger: "dead: no path to a terminator".into(),
            summary: "%0 = const i32 1".into(),
        });
        assert!(check(&before, &after, &rec).is_ok());
    }

    #[test]
    fn empty_ledger_is_caught() {
        let before = one_const();
        let mut after = before.clone();
        after.slab[0] = None;
        after.regions[0].cells.retain(|&c| c != CellId(0));
        let mut rec = DiffRecord::new("sneaky");
        rec.edits.push(Edit::RemoveCell { id: CellId(0), ledger: "   ".into(), summary: String::new() });
        let err = check(&before, &after, &rec).unwrap_err();
        assert!(err.contains("blank ledger"), "blank ledger must not count: {}", err);
    }
}

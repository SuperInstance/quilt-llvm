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

/// Lifecycle coupling (the cocapn `fleet_audit` pattern, ported to
/// cells): every ledger edit must be coupled to the lifecycle it claims.
/// Per tick, from the *before* and *after* fabrics plus the record:
///
/// - no duplicate RemoveCell/AddCell ids (ledger multiplication);
/// - a RemoveCell id must exist before and be gone after (a forged
///   ledger on a surviving cell is a lie even though membership checks
///   pass);
/// - an AddCell id must be fresh (>= before.slab.len(); ids are
///   append-only, reuse is resurrection) and present after;
/// - a Retarget with from == to is a no-op edit inflating the ledger
///   and faking `advanced`;
/// - a RemoveCell summary must equal the removed cell as the pass saw
///   it (before + this record's retargets) — a lying summary launders
///   the value's identity into prose;
/// - the population must reconcile: after == before + adds - removes.
///
/// What this deliberately does NOT reject: adding fresh cells (a pass
/// may rematerialize) and remove+re-add of identical content under a
/// fresh id (clone laundering is within the letter of the conservation
/// law; the ledger records both halves — see COCAPN-CONSERVE.md).
#[derive(Debug)]
pub struct PopulationReport {
    pub before: usize,
    pub after: usize,
    pub added: usize,
    pub removed: usize,
}

pub fn population_audit(
    before: &Fabric,
    after: &Fabric,
    rec: &DiffRecord,
) -> Result<PopulationReport, String> {
    let mut removed_ids: std::collections::BTreeSet<CellId> = Default::default();
    let mut added_ids: std::collections::BTreeSet<CellId> = Default::default();
    for e in &rec.edits {
        match e {
            Edit::RemoveCell { id, .. } => {
                if !removed_ids.insert(*id) {
                    return Err(format!(
                        "ledger multiplication: {} removed more than once in one tick",
                        id
                    ));
                }
            }
            Edit::AddCell { id, .. } => {
                if !added_ids.insert(*id) {
                    return Err(format!("ledger multiplication: {} added twice in one tick", id));
                }
            }
            Edit::Retarget { cell, slot, from, to } => {
                if from == to {
                    return Err(format!(
                        "no-op retarget {}.{}: {} -> {} inflates the ledger",
                        cell, slot, from, to
                    ));
                }
            }
        }
    }
    for id in &removed_ids {
        if before.cell(*id).is_none() {
            return Err(format!("forged removal: {} was not present before the tick", id));
        }
        if after.cell(*id).is_some() {
            return Err(format!(
                "forged ledger: {} carries a RemoveCell entry but survived the tick",
                id
            ));
        }
    }
    for id in &added_ids {
        if (id.0 as usize) < before.slab.len() {
            return Err(format!(
                "id resurrection: {} was already assigned (ids are append-only)",
                id
            ));
        }
        if after.cell(*id).is_none() {
            return Err(format!("phantom add: {} recorded but absent after the tick", id));
        }
    }
    // Summary truth: reconstruct each removed cell as the pass saw it —
    // the before fabric plus this record's own retargets (constfold
    // retargets a user before folding it in the same tick).
    if !removed_ids.is_empty() {
        let mut ctx = before.clone();
        for e in &rec.edits {
            if let Edit::Retarget { cell, slot, from, to } = e {
                if let Some(c) = ctx.cell_mut(*cell) {
                    if let Some(op) = c.operands.get_mut(*slot as usize) {
                        if *op == *from {
                            *op = *to;
                        }
                    }
                }
            }
        }
        for e in &rec.edits {
            if let Edit::RemoveCell { id, summary, .. } = e {
                let truth = crate::text::render_cell(&ctx, *id);
                if *summary != truth {
                    return Err(format!(
                        "lying summary: {} recorded as {:?} but the cell was {:?}",
                        id, summary, truth
                    ));
                }
            }
        }
    }
    let before_n = before.cells().count();
    let after_n = after.cells().count();
    if after_n != before_n + added_ids.len() - removed_ids.len() {
        return Err(format!(
            "population does not reconcile: {} + {} adds - {} removes != {} cells after",
            before_n,
            added_ids.len(),
            removed_ids.len(),
            after_n
        ));
    }
    Ok(PopulationReport { before: before_n, after: after_n, added: added_ids.len(), removed: removed_ids.len() })
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

/// The lifecycle-coupling audit battery: every attack from
/// docs/phase/COCAPN-CONSERVE.md that previously escaped the whole
/// stack, plus the within-law shapes it must ALLOW.
#[cfg(test)]
mod population_audit_tests {
    use super::*;
    use crate::cell::{Cell, CellKind};
    use crate::text;
    use crate::ty::{ConstVal, Type};

    /// entry: %0 = param ; %1 = const 7 (dead) ; %2 = ret %0
    fn mix() -> Fabric {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(7) }));
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![CellId(0)];
        f.add_cell(e, r);
        f
    }

    fn really_remove_dead(f: &Fabric, edits: &mut Vec<Edit>) -> Fabric {
        let mut g = f.clone();
        let region = g.cell(CellId(1)).unwrap().region;
        g.regions[region.0 as usize].cells.retain(|&c| c != CellId(1));
        g.slab[1] = None;
        edits.push(Edit::RemoveCell {
            id: CellId(1),
            ledger: "dead: no path to a terminator".into(),
            summary: text::render_cell(f, CellId(1)),
        });
        g
    }

    // RED: A3 ledger multiplication — previously caught only by post-run
    // replay; now refused at the tick.
    #[test]
    fn audit_rejects_ledger_multiplication() {
        let f = mix();
        let mut edits = vec![];
        let g = really_remove_dead(&f, &mut edits);
        let mut rec = DiffRecord::new("a3");
        rec.edits = edits.clone();
        rec.edits.push(edits[0].clone());
        let err = population_audit(&f, &g, &rec).unwrap_err();
        assert!(err.contains("multiplication"), "{}", err);
    }

    // RED: A4 forged ledger on a surviving cell — conserve::check passes
    // (membership is satisfied); the audit's lifecycle coupling refuses.
    #[test]
    fn audit_rejects_forged_survivor_ledger() {
        let f = mix();
        let mut rec = DiffRecord::new("a4");
        rec.edits.push(Edit::RemoveCell {
            id: CellId(1),
            ledger: "dead: no path to a terminator".into(),
            summary: text::render_cell(&f, CellId(1)),
        });
        let err = population_audit(&f, &f, &rec).unwrap_err();
        assert!(err.contains("forged ledger"), "{}", err);
        // and conserve::check alone still passes it — the gap was real
        assert!(check(&f, &f, &rec).is_ok());
    }

    // RED: A5 lying summary — the removal is real, the ledger is honest,
    // but the summary misrenders the buried value. Previously invisible.
    #[test]
    fn audit_rejects_a_lying_summary() {
        let f = mix();
        let mut g = f.clone();
        let region = g.cell(CellId(1)).unwrap().region;
        g.regions[region.0 as usize].cells.retain(|&c| c != CellId(1));
        g.slab[1] = None;
        let mut rec = DiffRecord::new("a5");
        rec.edits.push(Edit::RemoveCell {
            id: CellId(1),
            ledger: "dead: no path to a terminator".into(),
            summary: "%1 = const i32 999".into(), // it was 7
        });
        let err = population_audit(&f, &g, &rec).unwrap_err();
        assert!(err.contains("lying summary"), "{}", err);
    }

    // RED: A6 no-op retarget — from == to inflates the edit count and
    // fakes `advanced` while changing nothing.
    #[test]
    fn audit_rejects_noop_retarget_inflation() {
        let f = mix();
        let mut rec = DiffRecord::new("a6");
        rec.edits.push(Edit::Retarget { cell: CellId(2), slot: 0, from: CellId(0), to: CellId(0) });
        let err = population_audit(&f, &f, &rec).unwrap_err();
        assert!(err.contains("no-op retarget"), "{}", err);
    }

    // RED-adjacent: A7a literal id reuse — previously caught only by
    // replay's forged-id check post-run; the audit refuses at the tick.
    #[test]
    fn audit_rejects_id_resurrection() {
        let f = mix();
        let mut edits = vec![];
        let g = really_remove_dead(&f, &mut edits);
        // next tick's before fabric is `g`; a pass re-adds id 1
        let mut g2 = g.clone();
        let e = g2.cell(CellId(0)).unwrap().region;
        let cell = Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(7) });
        g2.slab[1] = Some(cell.clone());
        g2.regions[e.0 as usize].cells.insert(1, CellId(1));
        let mut rec = DiffRecord::new("a7");
        rec.edits.push(Edit::AddCell { id: CellId(1), index: 1, cell });
        let err = population_audit(&g, &g2, &rec).unwrap_err();
        assert!(err.contains("resurrection"), "{}", err);
    }

    // GREEN (within the law): A9 a pass may add fresh cells —
    // rematerialization is not a conservation violation.
    #[test]
    fn audit_allows_a_fresh_add() {
        let f = mix();
        let mut g = f.clone();
        let e = g.cell(CellId(0)).unwrap().region;
        let cell = Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(9) });
        g.slab.push(Some(cell.clone()));
        g.regions[e.0 as usize].cells.insert(1, CellId(3));
        let mut rec = DiffRecord::new("a9");
        rec.edits.push(Edit::AddCell { id: CellId(3), index: 1, cell });
        let rep = population_audit(&f, &g, &rec).expect("fresh adds are legal");
        assert_eq!((rep.before, rep.added, rep.removed, rep.after), (3, 1, 0, 4));
    }

    // GREEN (within the law): A8 clone laundering — remove %1, add an
    // identical const under a fresh id, both halves ledgered. The law's
    // letter is satisfied; the ledger records the churn.
    #[test]
    fn audit_allows_clone_laundering_and_reports_it() {
        let f = mix();
        let mut edits = vec![];
        let mut g = really_remove_dead(&f, &mut edits);
        let e = g.cell(CellId(0)).unwrap().region;
        let cell = Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(7) });
        g.slab.push(Some(cell.clone()));
        g.regions[e.0 as usize].cells.insert(1, CellId(3));
        let mut rec = DiffRecord::new("a8");
        rec.edits = edits;
        rec.edits.push(Edit::AddCell { id: CellId(3), index: 1, cell });
        let rep = population_audit(&f, &g, &rec).expect("both halves are ledgered: within the law");
        assert_eq!((rep.added, rep.removed), (1, 1));
    }

    // GREEN: the real passes must pass the audit — summary truth holds
    // through the retarget reconstruction (constfold retargets a user
    // before folding it, in the same tick).
    #[test]
    fn audit_is_green_on_the_real_passes() {
        use crate::passes;
        let f = mix();
        let (g, rec) = passes::constfold::const_fold(&f).unwrap();
        population_audit(&f, &g, &rec).expect("constfold conserves population");
        let (g2, rec2) = passes::dce::dce(&g).unwrap();
        population_audit(&g, &g2, &rec2).expect("dce conserves population");
    }

    // RED: population must reconcile even when every edit is individually
    // well-formed — a fabric that drops a cell while the record claims a
    // balanced population is a silent vanish wearing a balanced ledger.
    #[test]
    fn audit_rejects_unreconciled_population() {
        let f = mix();
        let mut g = really_remove_dead(&f, &mut vec![]);
        // silently vanish %0's param too, without any edit for it
        let region = g.cell(CellId(0)).unwrap().region;
        g.regions[region.0 as usize].cells.retain(|&c| c != CellId(0));
        g.slab[0] = None;
        let mut rec = DiffRecord::new("a10");
        rec.edits.push(Edit::RemoveCell {
            id: CellId(1),
            ledger: "dead: no path to a terminator".into(),
            summary: text::render_cell(&f, CellId(1)),
        });
        let err = population_audit(&f, &g, &rec).unwrap_err();
        assert!(err.contains("does not reconcile"), "{}", err);
    }
}

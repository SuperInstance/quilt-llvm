//! Call inlining: the third pass (scout build order — "inlining hardest,
//! largest diffs — ships last"; ARCHITECTURE M5).
//!
//! Pure function `caller x funcs -> (caller', diff)`. Each eligible call
//! cell is replaced by a graft of the callee's body:
//!
//! - callee params are NOT grafted — every use of param i is rewired to
//!   the caller's argument i at graft time (the arch doc's "inlining =
//!   rewire param wires to caller args", not a special case);
//! - the callee's `ret %v` terminator is not grafted — uses of the call
//!   cell are retargeted to the graft of %v (or straight to the caller
//!   arg when the callee returns a param);
//! - every other callee cell is grafted in order, before the call site,
//!   under fresh ids (ids are never reused);
//! - the call cell is removed WITH a conservation ledger entry naming
//!   the graft.
//!
//! v1 scope, stated: only STRAIGHT-LINE callees (single region, entry
//! with no predecessors, `ret` with exactly one operand). CFG-grafting
//! needs the region-edit diff vocabulary v0 explicitly deferred. Calls
//! to multi-region callees, cyclic callees, or void returns are SKIPPED
//! with a recorded note — never silently.

use crate::cell::CellKind;
use crate::diff::{DiffRecord, Edit};
use crate::fabric::Fabric;
use crate::id::{CellId, RegionId};
use crate::verify::verify;
use std::collections::BTreeMap;

pub fn inline_calls(
    caller: &Fabric,
    funcs: &BTreeMap<String, Fabric>,
) -> Result<(Fabric, DiffRecord), String> {
    if let Err(e) = verify(caller) {
        return Err(format!("inline refuses unverified input: {}", e));
    }
    for (name, f) in funcs {
        if let Err(e) = verify(f) {
            return Err(format!("inline refuses unverified callee '{}': {}", name, e));
        }
    }
    let mut g = caller.clone();
    let mut rec = DiffRecord::new("inline");

    // Deterministic: region order, then cell order. ids collected first —
    // the fabric mutates underneath us as we graft.
    let mut call_sites: Vec<CellId> = vec![];
    for ri in 0..g.regions.len() as u32 {
        for &id in &g.regions[ri as usize].cells.clone() {
            if matches!(g.cell(id).map(|c| &c.kind), Some(CellKind::Call { .. })) {
                call_sites.push(id);
            }
        }
    }

    for call_id in call_sites {
        let (callee_name, _) = match g.cell(call_id).map(|c| c.kind.clone()) {
            Some(CellKind::Call { name, ret_ty }) => (name, ret_ty),
            _ => unreachable!("collected call ids are calls"),
        };
        let callee = match funcs.get(&callee_name) {
            Some(c) => c,
            None => {
                rec.notes.push(format!(
                    "skip {}: callee '{}' not provided to the inliner",
                    call_id, callee_name
                ));
                continue;
            }
        };
        // eligibility guards (each skip is noted)
        if callee.regions.len() != 1 {
            rec.notes.push(format!(
                "skip {}: callee '{}' has {} regions (CFG graft deferred; v1 inlines straight-line callees only)",
                call_id,
                callee_name,
                callee.regions.len()
            ));
            continue;
        }
        let entry = RegionId(0);
        if !callee.predecessors(entry).is_empty() {
            rec.notes.push(format!(
                "skip {}: callee '{}' is cyclic (entry has predecessors)",
                call_id, callee_name
            ));
            continue;
        }
        let term = *callee
            .region(entry)
            .and_then(|r| r.cells.last())
            .expect("verified fabric has a terminator");
        let ret_val = match callee.cell(term).map(|c| (&c.kind, c.operands.clone())) {
            Some((CellKind::Ret, ops)) if ops.len() == 1 => ops[0],
            _ => {
                rec.notes.push(format!(
                    "skip {}: callee '{}' does not return exactly one value (void returns are out of v1 scope)",
                    call_id, callee_name
                ));
                continue;
            }
        };
        // callee params, in entry order
        let params: Vec<CellId> = callee
            .region(entry)
            .map(|r| r.cells.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|&id| matches!(callee.cell(id).map(|c| &c.kind), Some(CellKind::Param { .. })))
            .collect();
        if params.len() != g.cell(call_id).unwrap().operands.len() {
            rec.notes.push(format!(
                "skip {}: arity mismatch with callee '{}'",
                call_id, callee_name
            ));
            continue;
        }

        // ---- graft ----
        let call_region = g.cell(call_id).unwrap().region;
        let args: Vec<CellId> = g.cell(call_id).unwrap().operands.clone();
        let mut map: BTreeMap<CellId, CellId> = BTreeMap::new(); // callee id -> caller id
        for (i, &p) in params.iter().enumerate() {
            map.insert(p, args[i]);
        }
        let mut insert_at = g
            .index_in_region(call_id)
            .expect("verified fabric lists its cells");
        let mut grafted = 0usize;
        for &cid in &callee.region(entry).unwrap().cells {
            let cc = callee.cell(cid).expect("present");
            match &cc.kind {
                CellKind::Param { .. } => continue, // bound to args
                CellKind::Ret => continue,          // handled below
                _ => {}
            }
            let mut mapped = cc.clone();
            mapped.region = call_region;
            mapped.operands = cc
                .operands
                .iter()
                .map(|&op| *map.get(&op).expect("callee is verified: operands resolve"))
                .collect();
            let new_id = g.insert_cell(call_region, insert_at, mapped.clone());
            map.insert(cid, new_id);
            rec.edits.push(Edit::AddCell { id: new_id, index: insert_at, cell: mapped });
            insert_at += 1;
            grafted += 1;
        }
        // retarget uses of the call to the mapped return value
        let ret_mapped = *map
            .get(&ret_val)
            .expect("ret operand is a param or a grafted cell");
        for (user, slot) in g.uses_of(call_id).to_vec() {
            let from = g.retarget(user, slot, ret_mapped).expect("present user");
            rec.edits.push(Edit::Retarget { cell: user, slot, from, to: ret_mapped });
        }
        // remove the call cell, with the conservation ledger entry
        let summary = crate::text::render_cell(&g, call_id);
        g.remove_cell(call_id).expect("call cell listed");
        rec.edits.push(Edit::RemoveCell {
            id: call_id,
            ledger: format!(
                "inlined '{}': {} cells grafted, {} params bound to caller args, ret -> {}",
                callee_name, grafted, params.len(), ret_mapped
            ),
            summary,
        });
    }

    if let Err(e) = verify(&g) {
        return Err(format!("inline produced an invalid fabric: {}", e));
    }
    Ok((g, rec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;
    use crate::conserve;
    use crate::diff::History;
    use crate::prov;
    use crate::replay;

    fn prog() -> (Fabric, BTreeMap<String, Fabric>) {
        let main_text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = const i32 2\n\
  %2 = call i32 add2 %0, %1\n\
  %3 = ret %2\n";
        let callee_text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = param i32\n\
  %2 = arith.add i32 %0, %1\n\
  %3 = ret %2\n";
        let mut funcs = BTreeMap::new();
        funcs.insert("add2".to_string(), crate::text::parse(callee_text).unwrap());
        (crate::text::parse(main_text).unwrap(), funcs)
    }

    #[test]
    fn green_inlines_straight_line_call() {
        let (f, funcs) = prog();
        assert!(crate::program::verify_program(&crate::program::Program {
            order: vec!["main".into(), "add2".into()],
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
        let (g, rec) = inline_calls(&f, &funcs).expect("inline");
        assert_ne!(g, f, "red without the pass: identity would fail everything below");
        assert!(g.cell(CellId(2)).is_none(), "the call cell must be gone");
        // ret now fed by the grafted add, fed by param and const
        let ret_op = g.cell(CellId(3)).unwrap().operands[0];
        let kind = g.cell(ret_op).unwrap().kind.clone();
        assert!(matches!(kind, CellKind::Arith { .. }), "grafted add feeds ret: {:?}", kind);
        let ops = g.cell(ret_op).unwrap().operands.clone();
        assert_eq!(ops, vec![CellId(0), CellId(1)], "params bound to caller args");
        assert!(verify(&g).is_ok());
        assert!(conserve::check(&f, &g, &rec).is_ok(), "conservation over the graft");
        let ledger = rec
            .edits
            .iter()
            .find_map(|e| match e {
                Edit::RemoveCell { ledger, .. } => Some(ledger.clone()),
                _ => None,
            })
            .unwrap();
        assert!(ledger.contains("inlined 'add2'"), "{}", ledger);
        assert!(ledger.contains("params bound"), "{}", ledger);
    }

    #[test]
    fn red_no_calls_is_identity() {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: crate::ty::Type::I32 }));
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![p];
        f.add_cell(e, r);
        let funcs = BTreeMap::new();
        let (g, rec) = inline_calls(&f, &funcs).expect("inline");
        assert_eq!(g, f);
        assert!(rec.is_empty());
    }

    /// THE payoff shot: before inline, the walk stops at the call; after,
    /// it walks THROUGH the grafted callee body into the caller's values.
    #[test]
    fn provenance_crosses_the_inline_boundary() {
        let (f, funcs) = prog();
        // before: the data walk ends at the call cell (a leaf that is
        // neither param nor const — the boundary v0 could not cross)
        let before = prov::render(&prov::provenance(&f, CellId(3)).unwrap());
        assert!(before.contains("call i32 add2"), "{}", before);
        // after: no call anywhere in the tree; callee body + caller roots
        let (g, _rec) = inline_calls(&f, &funcs).expect("inline");
        let ret_op = g.cell(CellId(3)).unwrap().operands[0];
        let after = prov::render(&prov::provenance(&g, ret_op).unwrap());
        assert!(after.contains("arith.add"), "callee body is in the walk: {}", after);
        assert!(after.contains("param i32"), "caller param reached through the graft: {}", after);
        assert!(after.contains("const i32 2"), "caller const reached: {}", after);
        assert!(!after.contains("call"), "no call leaves remain: {}", after);
        // and history tells the story of the removed call cell
        let mut h = History::new();
        let (_, rec) = inline_calls(&f, &funcs).unwrap();
        h.push(rec);
        let story = prov::prov_history(&h, CellId(2));
        assert!(story.len() == 1 && story[0].2.contains("inlined"), "{:?}", story);
    }

    #[test]
    fn multi_region_callee_is_skipped_with_a_note() {
        let (f, _) = prog();
        let callee_text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = const i1 true\n\
  %2 = br %1, t, e\n\
region t\n\
  %3 = jump e\n\
region e\n\
  %4 = ret %0\n";
        let mut funcs = BTreeMap::new();
        funcs.insert("add2".to_string(), crate::text::parse(callee_text).unwrap());
        let (g, rec) = inline_calls(&f, &funcs).expect("inline");
        assert_eq!(g, f, "skip must leave the fabric untouched");
        assert!(rec.edits.is_empty());
        assert!(
            rec.notes.iter().any(|n| n.contains("3 regions")),
            "skip note recorded: {:?}",
            rec.notes
        );
    }

    #[test]
    fn unknown_callee_is_skipped_with_a_note() {
        let (f, funcs) = prog();
        let mut funcs2 = BTreeMap::new();
        funcs2.insert("other".to_string(), funcs["add2"].clone());
        let (g, rec) = inline_calls(&f, &funcs2).expect("inline");
        assert_eq!(g, f);
        assert!(rec.notes.iter().any(|n| n.contains("not provided")), "{:?}", rec.notes);
    }

    #[test]
    fn nested_call_inlines_on_the_next_sweep() {
        // main -> add2(x, one) where add2's body calls one(x); one sweep
        // inlines add2 (its body's inner call grafts as a call cell);
        // a second sweep inlines that.
        let main_text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = call i32 add2 %0\n\
  %2 = ret %1\n";
        let add2_text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = call i32 one %0\n\
  %2 = ret %1\n";
        let one_text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = const i32 1\n\
  %2 = arith.add i32 %0, %1\n\
  %3 = ret %2\n";
        let mut funcs = BTreeMap::new();
        funcs.insert("add2".to_string(), crate::text::parse(add2_text).unwrap());
        funcs.insert("one".to_string(), crate::text::parse(one_text).unwrap());
        let f = crate::text::parse(main_text).unwrap();
        let (g1, rec1) = inline_calls(&f, &funcs).expect("sweep 1");
        assert!(g1.cell(CellId(1)).is_none(), "outer call gone");
        assert!(rec1.edits.iter().any(|e| matches!(e, Edit::AddCell { cell, .. } if matches!(&cell.kind, CellKind::Call { .. }))),
            "inner call grafted as a call cell");
        let (g2, rec2) = inline_calls(&g1, &funcs).expect("sweep 2");
        assert!(!rec2.is_empty(), "second sweep has work");
        assert!(verify(&g2).is_ok());
        let ret_op = g2.cell(CellId(2)).unwrap().operands[0];
        let walk = prov::render(&prov::provenance(&g2, ret_op).unwrap());
        assert!(walk.contains("const i32 1"), "fully flattened: {}", walk);
        assert!(!walk.contains("call"), "{}", walk);
    }

    #[test]
    fn replay_reproduces_inline_stages_bit_identically() {
        let (f, funcs) = prog();
        let (g, rec) = inline_calls(&f, &funcs).unwrap();
        let mut h = History::new();
        h.push(rec);
        let (stages, final_r) = replay::replay(&f, &h).unwrap();
        assert_eq!(stages.len(), 2);
        assert_eq!(stages[1], g);
        assert_eq!(stages[1], final_r);
        assert_eq!(crate::text::print(&stages[1]), crate::text::print(&g));
    }
}

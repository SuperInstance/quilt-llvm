//! COCAPN-CONSERVE probe: run each conservation-violation attack
//! through the CURRENT PassManager stack and report caught/missed
//! honestly. Temporary probe — results go into docs/phase/COCAPN-CONSERVE.md.

use llvm_fabric::cell::{Cell, CellKind};
use llvm_fabric::diff::{DiffRecord, Edit};
use llvm_fabric::fabric::Fabric;
use llvm_fabric::id::CellId;
use llvm_fabric::manager::{PassManager, TickCtx};
use llvm_fabric::ty::{ConstVal, Type};
use std::collections::BTreeMap;

/// entry: %0 = param i32 ; %1 = const i32 7 (dead) ; %2 = ret %0
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

type P = fn(&Fabric, &TickCtx, &BTreeMap<String, Fabric>) -> Result<(Fabric, DiffRecord), String>;

fn run(_label: &str, pipeline: &[&'static str], passes: &[(&'static str, P)]) -> (bool, String) {
    let mut m = PassManager::new();
    for (n, f) in passes {
        m.register(n, *f);
    }
    match m.run(&mix(), pipeline, &BTreeMap::new()) {
        Ok(run) => {
            let adv: Vec<&str> = run.audit.iter().map(|a| if a.advanced { "adv" } else { "fp" }).collect();
            (false, format!("MISSED (audit: {})", adv.join(",")))
        }
        Err(e) => (true, format!("caught: {}", e)),
    }
}

fn really_remove_dead(f: &Fabric, edits: &mut Vec<Edit>) -> Fabric {
    let mut g = f.clone();
    let region = g.cell(CellId(1)).unwrap().region;
    g.regions[region.0 as usize].cells.retain(|&c| c != CellId(1));
    g.slab[1] = None;
    edits.push(Edit::RemoveCell {
        id: CellId(1),
        ledger: "dead: no path to a terminator".into(),
        summary: llvm_fabric::text::render_cell(f, CellId(1)),
    });
    g
}

// A1 control: silent vanish (no ledger)
fn a1_silent_vanish(f: &Fabric, _c: &TickCtx, _g: &BTreeMap<String, Fabric>) -> Result<(Fabric, DiffRecord), String> {
    let mut g = f.clone();
    let region = g.cell(CellId(1)).unwrap().region;
    g.regions[region.0 as usize].cells.retain(|&c| c != CellId(1));
    g.slab[1] = None;
    Ok((g, DiffRecord::new("a1")))
}

// A3: ledger multiplication — one real removal, two RemoveCell edits
fn a3_duplicate_ledger(f: &Fabric, _c: &TickCtx, _g: &BTreeMap<String, Fabric>) -> Result<(Fabric, DiffRecord), String> {
    let mut edits = vec![];
    let g = really_remove_dead(f, &mut edits);
    let mut rec = DiffRecord::new("a3");
    rec.edits = edits.clone();
    rec.edits.push(edits[0].clone()); // the multiplication
    Ok((g, rec))
}

// A4: forged ledger on a surviving cell — fabric unchanged, edit claims a removal
fn a4_forged_survivor(f: &Fabric, _c: &TickCtx, _g: &BTreeMap<String, Fabric>) -> Result<(Fabric, DiffRecord), String> {
    let mut rec = DiffRecord::new("a4");
    rec.edits.push(Edit::RemoveCell {
        id: CellId(1),
        ledger: "dead: no path to a terminator".into(),
        summary: llvm_fabric::text::render_cell(f, CellId(1)),
    });
    Ok((f.clone(), rec))
}

// A5: lying summary — real removal, summary misrenders the cell
fn a5_lying_summary(f: &Fabric, _c: &TickCtx, _g: &BTreeMap<String, Fabric>) -> Result<(Fabric, DiffRecord), String> {
    let mut g = f.clone();
    let region = g.cell(CellId(1)).unwrap().region;
    g.regions[region.0 as usize].cells.retain(|&c| c != CellId(1));
    g.slab[1] = None;
    let mut rec = DiffRecord::new("a5");
    rec.edits.push(Edit::RemoveCell {
        id: CellId(1),
        ledger: "dead: no path to a terminator".into(),
        summary: "%1 = const i32 999".into(), // lie: it was 7
    });
    Ok((g, rec))
}

// A6: no-op retarget — fabric unchanged, edit count inflated, "advanced" faked
fn a6_noop_retarget(f: &Fabric, _c: &TickCtx, _g: &BTreeMap<String, Fabric>) -> Result<(Fabric, DiffRecord), String> {
    let mut rec = DiffRecord::new("a6");
    rec.edits.push(Edit::Retarget { cell: CellId(2), slot: 0, from: CellId(0), to: CellId(0) });
    Ok((f.clone(), rec))
}

// A7: resurrection — tick1 removes %1, tick2 adds a cell reusing id 1
fn a7_rm(f: &Fabric, _c: &TickCtx, _g: &BTreeMap<String, Fabric>) -> Result<(Fabric, DiffRecord), String> {
    let mut edits = vec![];
    let g = really_remove_dead(f, &mut edits);
    let mut rec = DiffRecord::new("a7rm");
    rec.edits = edits;
    Ok((g, rec))
}
fn a7_addback(f: &Fabric, _c: &TickCtx, _g: &BTreeMap<String, Fabric>) -> Result<(Fabric, DiffRecord), String> {
    let mut g = f.clone();
    let e = g.cell(CellId(0)).unwrap().region;
    let cell = Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(7) });
    g.slab.push(Some(cell.clone()));
    g.regions[e.0 as usize].cells.insert(1, CellId(3));
    let mut rec = DiffRecord::new("a7add");
    rec.edits.push(Edit::AddCell { id: CellId(3), index: 1, cell });
    Ok((g, rec))
}

fn a7a_reuse_id(f: &Fabric, _c: &TickCtx, _g: &BTreeMap<String, Fabric>) -> Result<(Fabric, DiffRecord), String> {
    let mut g = f.clone();
    let e = g.cell(CellId(0)).unwrap().region;
    let cell = Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(7) });
    g.slab[1] = Some(cell.clone()); // resurrect the REMOVED id, not a fresh one
    g.regions[e.0 as usize].cells.insert(1, CellId(1));
    let mut rec = DiffRecord::new("a7reuse");
    rec.edits.push(Edit::AddCell { id: CellId(1), index: 1, cell });
    Ok((g, rec))
}

// A8: clone laundering — remove %1, add a fresh identical const, all edits real
fn a8_clone_launder(f: &Fabric, _c: &TickCtx, _g: &BTreeMap<String, Fabric>) -> Result<(Fabric, DiffRecord), String> {
    let mut edits = vec![];
    let mut g = really_remove_dead(f, &mut edits);
    let e = g.cell(CellId(0)).unwrap().region;
    let cell = Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(7) });
    g.slab.push(Some(cell.clone()));
    g.regions[e.0 as usize].cells.insert(1, CellId(3));
    let mut rec = DiffRecord::new("a8");
    rec.edits = edits;
    rec.edits.push(Edit::AddCell { id: CellId(3), index: 1, cell });
    Ok((g, rec))
}

// A9: population inflation — a pass that only adds a fresh unused const
fn a9_inflate(f: &Fabric, _c: &TickCtx, _g: &BTreeMap<String, Fabric>) -> Result<(Fabric, DiffRecord), String> {
    let mut g = f.clone();
    let e = g.cell(CellId(0)).unwrap().region;
    let cell = Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(9) });
    g.slab.push(Some(cell.clone()));
    g.regions[e.0 as usize].cells.insert(1, CellId(3));
    let mut rec = DiffRecord::new("a9");
    rec.edits.push(Edit::AddCell { id: CellId(3), index: 1, cell });
    Ok((g, rec))
}

fn main() {
    let mut results: Vec<(&str, bool, String)> = vec![];
    macro_rules! probe {
        ($n:expr, $p:expr, $r:expr) => {{ let r = run($n, $p, $r); results.push(($n, r.0, r.1)); }};
    }
    probe!("A1 silent vanish", &["a1"], &[("a1", a1_silent_vanish)]);
    probe!("A3 ledger multiplication", &["a3"], &[("a3", a3_duplicate_ledger)]);
    probe!("A4 forged survivor ledger", &["a4"], &[("a4", a4_forged_survivor)]);
    probe!("A5 lying summary", &["a5"], &[("a5", a5_lying_summary)]);
    probe!("A6 no-op retarget inflation", &["a6"], &[("a6", a6_noop_retarget)]);
    probe!("A7 id resurrection (2 ticks)", &["a7rm", "a7add"], &[("a7rm", a7_rm), ("a7add", a7_addback)]);
    probe!("A7a literal id reuse (2 ticks)", &["a7rm", "a7reuse"], &[("a7rm", a7_rm), ("a7reuse", a7a_reuse_id)]);
    probe!("A8 clone laundering (within-law)", &["a8"], &[("a8", a8_clone_launder)]);
    probe!("A9 population inflation (within-law)", &["a9"], &[("a9", a9_inflate)]);
    for (name, caught, detail) in &results {
        println!("{:35} {:6} {}", name, if *caught { "CAUGHT" } else { "MISSED" }, detail);
    }
}

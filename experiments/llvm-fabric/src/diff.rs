//! N4 history: every transform appends a diff record; nothing rewrites.
//!
//! Edits are machine-applicable (not prose): `replay::replay` folds them
//! over the original fabric to reproduce every intermediate fabric.
//! `RemoveCell` edits carry a ledger entry — the conservation law's
//! paper trail (see conserve.rs).

use crate::cell::Cell;
use crate::id::CellId;

#[derive(Clone, PartialEq, Debug)]
pub enum Edit {
    /// Add a cell under an explicit (fresh) id at a position in its region.
    /// Invariant: id == fabric.slab.len() at apply time (ids append-only).
    AddCell { id: CellId, index: usize, cell: Cell },
    /// Remove a cell. `ledger` is the conservation-law entry: why this
    /// value was dropped. `summary` is the cell's rendered form for humans.
    RemoveCell { id: CellId, ledger: String, summary: String },
    /// Rewire one use: operands[slot] of `cell` goes from `from` to `to`.
    Retarget { cell: CellId, slot: u32, from: CellId, to: CellId },
}

#[derive(Clone, PartialEq, Debug)]
pub struct DiffRecord {
    pub pass: &'static str,
    pub epoch: u64,
    pub edits: Vec<Edit>,
    /// Human-readable pass notes that are NOT machine-applicable edits —
    /// e.g. why a candidate transform was skipped. A skip without a note
    /// would be the "silent no-op" the scout report bans (TILE-CONTRACT
    /// M3); notes make skips queryable.
    pub notes: Vec<String>,
}

impl DiffRecord {
    pub fn new(pass: &'static str) -> DiffRecord {
        DiffRecord { pass, epoch: 0, edits: vec![], notes: vec![] }
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub fn render(&self, f: &crate::fabric::Fabric) -> String {
        let mut out = format!("epoch {} pass {}\n", self.epoch, self.pass);
        for n in &self.notes {
            out.push_str(&format!("  # note: {}\n", n));
        }
        for e in &self.edits {
            match e {
                Edit::AddCell { id, index, cell } => {
                    let rname = f
                        .region(cell.region)
                        .map(|r| r.name.as_str())
                        .unwrap_or("<bad>");
                    // Render from the STORED cell: it may already be gone
                    // from the final fabric the history is rendered against.
                    out.push_str(&format!(
                        "  + {} @ {}[{}] :: {}\n",
                        id,
                        rname,
                        index,
                        render_standalone(f, *id, cell)
                    ));
                }
                Edit::RemoveCell { id, ledger, summary } => {
                    out.push_str(&format!("  - {} ({}) :: {}\n", id, ledger, summary));
                }
                Edit::Retarget { cell, slot, from, to } => {
                    out.push_str(&format!("  ~ {}.{}: {} -> {}\n", cell, slot, from, to));
                }
            }
        }
        out
    }
}

/// Render a cell stored inside an AddCell edit (it may no longer exist
/// in the fabric the history is rendered against).
fn render_standalone(f: &crate::fabric::Fabric, id: CellId, cell: &crate::cell::Cell) -> String {
    use crate::cell::CellKind;
    let o = |i: usize| -> String {
        cell.operands
            .get(i)
            .map(|x| x.to_string())
            .unwrap_or_else(|| "%<missing>".into())
    };
    match &cell.kind {
        CellKind::Param { ty } => format!("{} = param {}", id, ty.name()),
        CellKind::Const { ty, val } => format!("{} = const {} {}", id, ty.name(), val.render()),
        CellKind::Arith { op, ty } => format!("{} = {} {} {}, {}", id, op.name(), ty.name(), o(0), o(1)),
        CellKind::Cmp { op } => format!("{} = {} {}, {}", id, op.name(), o(0), o(1)),
        CellKind::Branch { then_r, else_r } => format!(
            "{} = br {}, {}, {}",
            id,
            o(0),
            f.region_name(*then_r),
            f.region_name(*else_r)
        ),
        CellKind::Jump { target } => format!("{} = jump {}", id, f.region_name(*target)),
        CellKind::Phi { joins } => {
            let parts: Vec<String> = joins
                .iter()
                .zip(cell.operands.iter())
                .map(|(r, v)| format!("[{}: {}]", f.region_name(*r), v))
                .collect();
            format!("{} = phi {}", id, parts.join(" "))
        }
        CellKind::Ret => {
            if cell.operands.is_empty() {
                format!("{} = ret", id)
            } else {
                format!("{} = ret {}", id, o(0))
            }
        }
        CellKind::Call { name, ret_ty } => {
            let args: Vec<String> = cell.operands.iter().map(|x| x.to_string()).collect();
            format!("{} = call {} {} {}", id, ret_ty.name(), name, args.join(", "))
        }
    }
}

/// Append-only history. Epochs are assigned on push, monotonically.
///
/// v1: the Weft — every tick recorded via `push_tick` lands a
/// hash-chained `TickSig` (fabric signature + chain + a MECHANICAL
/// progress entry). `push` is the v0 path (no Weft entry); histories are
/// all-or-nothing by law (`check_weft` rejects a partial Weft).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct History {
    pub records: Vec<DiffRecord>,
    pub weft: Vec<crate::sign::TickSig>,
}

impl History {
    pub fn new() -> History {
        History::default()
    }

    pub fn push(&mut self, mut rec: DiffRecord) -> u64 {
        rec.epoch = self.records.len() as u64;
        let epoch = rec.epoch;
        self.records.push(rec);
        epoch
    }

    /// v1 tick: append the diff record AND record its Weft entry —
    /// signature of `fabric_after`, chained to the previous tick, with
    /// the progress entry derived mechanically from the diff.
    pub fn push_tick(&mut self, mut rec: DiffRecord, fabric_after: &crate::fabric::Fabric) -> u64 {
        rec.epoch = self.records.len() as u64;
        let epoch = rec.epoch;
        let sig = crate::sign::fabric_sig(fabric_after);
        let prev = self.weft.last().map(|t| t.chain);
        let chain = crate::sign::TickSig::chain_step(prev, epoch, rec.pass, sig);
        let advanced = !rec.edits.is_empty();
        let note = if advanced {
            format!("advanced ({} edits)", rec.edits.len())
        } else {
            format!("fixed point — no edits fired")
        };
        self.weft.push(crate::sign::TickSig {
            epoch,
            pass: rec.pass,
            sig,
            chain,
            advanced,
            note,
        });
        self.records.push(rec);
        epoch
    }

    /// The progress law, made checkable: a history that records ANY
    /// Weft entries must record one per tick (gapless, in step with the
    /// records), every entry carries a non-empty progress note, and
    /// non-advancing ticks declare fixed point explicitly.
    pub fn check_weft(&self) -> Result<(), String> {
        if self.weft.is_empty() {
            return Ok(()); // v0-style history: pre-law, labeled, allowed
        }
        if self.weft.len() != self.records.len() {
            return Err(format!(
                "progress law violated: weft covers {}/{} ticks — partial recording is forbidden",
                self.weft.len(),
                self.records.len()
            ));
        }
        for (i, t) in self.weft.iter().enumerate() {
            if t.epoch != i as u64 {
                return Err(format!("progress law violated: weft[{}] has epoch {}", i, t.epoch));
            }
            if t.note.trim().is_empty() {
                return Err(format!("progress law violated: tick {} has a blank note", i));
            }
            if !t.advanced && !t.note.contains("fixed point") {
                return Err(format!(
                    "progress law violated: tick {} neither advanced nor declared fixed point",
                    i
                ));
            }
        }
        Ok(())
    }

    /// Chain verification against replayed stages: stages[0] is the
    /// pre-tick fabric, stages[i+1] the fabric after tick i. Every
    /// recorded signature must match the actual stage, and the chain
    /// must re-link. A tampered stage fails naming its tick.
    pub fn verify_chain(&self, stages: &[crate::fabric::Fabric]) -> Result<(), String> {
        if self.weft.is_empty() {
            return Ok(());
        }
        if stages.len() != self.weft.len() + 1 {
            return Err(format!(
                "chain verification: {} stages for {} weft entries",
                stages.len(),
                self.weft.len()
            ));
        }
        let mut prev: Option<u64> = None;
        for (i, t) in self.weft.iter().enumerate() {
            let actual = crate::sign::fabric_sig(&stages[i + 1]);
            if actual != t.sig {
                return Err(format!(
                    "chain verification: tick {} recorded sig {:016x} but the stage hashes {:016x} — history does not match the fabric",
                    i, t.sig, actual
                ));
            }
            let want = crate::sign::TickSig::chain_step(prev, t.epoch, t.pass, t.sig);
            if want != t.chain {
                return Err(format!("chain verification: tick {} does not re-link", i));
            }
            prev = Some(t.chain);
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn render(&self, f: &crate::fabric::Fabric) -> String {
        let mut out = String::new();
        for rec in &self.records {
            out.push_str(&rec.render(f));
        }
        out
    }

    pub fn bytes(&self, f: &crate::fabric::Fabric) -> usize {
        self.render(f).len()
    }

    /// Every edit that mentions a cell, in epoch order — the cell's
    /// transform provenance (works for dropped cells too).
    pub fn mentions_of(&self, id: CellId) -> Vec<(u64, &'static str, String)> {
        let mut out = vec![];
        for rec in &self.records {
            for e in &rec.edits {
                let m = match e {
                    Edit::AddCell { id: i, .. } if *i == id => {
                        Some(format!("added"))
                    }
                    Edit::RemoveCell { id: i, ledger, .. } if *i == id => {
                        Some(format!("removed ({})", ledger))
                    }
                    Edit::Retarget { cell: c, slot, from, to } if *c == id => {
                        Some(format!("retargeted use .{} {} -> {}", slot, from, to))
                    }
                    _ => None,
                };
                if let Some(what) = m {
                    out.push((rec.epoch, rec.pass, what));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epochs_assigned_monotonically() {
        let mut h = History::new();
        let e0 = h.push(DiffRecord::new("a"));
        let e1 = h.push(DiffRecord::new("b"));
        assert_eq!((e0, e1), (0, 1));
        assert_eq!(h.records[1].epoch, 1);
    }

    #[test]
    fn mentions_of_sees_all_roles() {
        let mut h = History::new();
        let mut r = DiffRecord::new("p");
        r.edits.push(Edit::Retarget { cell: CellId(3), slot: 0, from: CellId(1), to: CellId(9) });
        h.push(r);
        let m = h.mentions_of(CellId(3));
        assert_eq!(m.len(), 1);
        assert!(m[0].2.contains("retargeted"));
    }
}

#[cfg(test)]
mod weft_tests {
    use super::*;
    use crate::cell::{Cell, CellKind};
    use crate::fabric::Fabric;
    use crate::ty::{ConstVal, Type};

    fn mix() -> Fabric {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let c1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(20) }));
        let c2 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(22) }));
        let mut a = Cell::new(e, CellKind::Arith { op: crate::cell::ArithOp::Add, ty: Type::I32 });
        a.operands = vec![c1, c2];
        let a = f.add_cell(e, a);
        let dead = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I64, val: ConstVal::I64(7) }));
        let mut a2 = Cell::new(e, CellKind::Arith { op: crate::cell::ArithOp::Add, ty: Type::I32 });
        a2.operands = vec![p, a];
        let a2 = f.add_cell(e, a2);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![a2];
        f.add_cell(e, r);
        let _ = dead;
        f
    }

    fn run_pipeline(f: &Fabric) -> (Fabric, History, Vec<Fabric>) {
        crate::pipeline::run(f).expect("pipeline")
    }

    #[test]
    fn weft_records_every_tick_with_progress() {
        let f = mix();
        let (final_f, h, stages) = run_pipeline(&f);
        assert_eq!(h.weft.len(), h.records.len(), "one weft entry per tick");
        assert!(h.check_weft().is_ok());
        // ticks 0-1 advance; tick 2 (second constfold) is a fixed point
        assert!(h.weft[0].advanced && h.weft[0].note.contains("advanced"));
        assert!(!h.weft[2].advanced, "second constfold fires nothing: {:?}", h.weft[2].note);
        assert!(h.weft[2].note.contains("fixed point"), "{}", h.weft[2].note);
        // chain verifies against the actual stages
        assert!(h.verify_chain(&stages).is_ok());
        let _ = final_f;
    }

    #[test]
    fn tampered_stage_breaks_the_chain_with_the_tick_number() {
        let f = mix();
        let (_, h, mut stages) = run_pipeline(&f);
        // tamper with stage 1 (after constfold): flip the folded const
        let mut g = stages[1].clone();
        let ids: Vec<_> = g.cells().collect();
        for id in ids {
            if let Some(c) = g.cell_mut(id) {
                if let CellKind::Const { val, .. } = &mut c.kind {
                    *val = ConstVal::I32(999);
                }
            }
        }
        stages[1] = g;
        let err = h.verify_chain(&stages).unwrap_err();
        assert!(err.contains("tick 0"), "{}", err);
    }

    #[test]
    fn partial_weft_violates_the_law() {
        let f = mix();
        let (_, mut h, _) = run_pipeline(&f);
        h.weft.pop();
        let err = h.check_weft().unwrap_err();
        assert!(err.contains("progress law"), "{}", err);
        assert!(err.contains("3/4"), "{}", err);
    }

    #[test]
    fn blank_note_violates_the_law() {
        let f = mix();
        let (_, mut h, _) = run_pipeline(&f);
        h.weft[1].note = "  ".into();
        assert!(h.check_weft().is_err());
    }

    #[test]
    fn v0_push_only_history_is_labeled_pre_law_not_rejected() {
        let mut h = History::new();
        h.push(DiffRecord::new("old"));
        assert!(h.check_weft().is_ok(), "v0-style: empty weft is allowed (pre-law)");
    }
}

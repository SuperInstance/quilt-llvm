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
}

impl DiffRecord {
    pub fn new(pass: &'static str) -> DiffRecord {
        DiffRecord { pass, epoch: 0, edits: vec![] }
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub fn render(&self, f: &crate::fabric::Fabric) -> String {
        let mut out = format!("epoch {} pass {}\n", self.epoch, self.pass);
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
    }
}

/// Append-only history. Epochs are assigned on push, monotonically.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct History {
    pub records: Vec<DiffRecord>,
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

//! The fabric: regions of cells, wires as use edges, slab storage with
//! stable ids. Ids are never reused; removed cells leave `None` holes
//! (N4: history appends, ids persist).
//!
//! The graveyard (M4.1, tit-quilt retrofit): `tombstones` carries a
//! hash-only record of every FORGOTTEN cell — append-only, never
//! pruned, never deleted (the provenance-integrity law). A tombstone
//! and its cell never coexist: forgetting removes from the slab and
//! leaves the tombstone; the slab hole and the tombstone are the same
//! event seen from the two sides.

use crate::cell::{Cell, CellKind};
use crate::decay::Tombstone;
use crate::id::{CellId, RegionId};
use crate::usetables::{kind_succs, UseTables};

#[derive(Clone, PartialEq, Debug)]
pub struct Region {
    pub name: String,
    /// Cells in program order. The last one must be the terminator.
    pub cells: Vec<CellId>,
}

/// One use edge: value `from` (a def) flows into slot `slot` of `to`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Wire {
    pub from: CellId,
    pub to: CellId,
    pub slot: u32,
}

/// Fabric equality observes CONTENT, not derived indexes: regions,
/// slab, graveyard. The use/pred/succ tables (R2) are a maintained
/// derivative of the content and are excluded on purpose — a fabric
/// and its freshly-re-derived twin are the same fabric (the replay
/// law applied to indexes; see usetables.rs).
impl PartialEq for Fabric {
    fn eq(&self, other: &Self) -> bool {
        self.regions == other.regions && self.slab == other.slab && self.tombstones == other.tombstones
    }
}

#[derive(Clone, Debug, Default)]
pub struct Fabric {
    /// regions[0] is the entry region, when any.
    pub regions: Vec<Region>,
    /// Cell slab. `None` = removed (or not yet assigned). Index = CellId.
    pub slab: Vec<Option<Cell>>,
    /// Maintained use/pred/succ tables (R2 lane B). Derived, not
    /// content: excluded from PartialEq, from `text::print`, and from
    /// the fabric signature. Maintained in O(degree) by every
    /// sanctioned edit; re-derivable at any time via `rebuild_tables`.
    pub tables: UseTables,
    /// The graveyard: hash-only tombstones of FORGOTTEN cells
    /// (tit-quilt's provenance-integrity law retrofitted; see decay.rs).
    /// NOT part of `text::print` — the fabric signature observes the
    /// live fabric; tombstone integrity is audited by `verify_deaths`
    /// recomputation and the V18/V19 verifier laws.
    pub tombstones: Vec<Tombstone>,
}

impl Fabric {
    pub fn empty() -> Fabric {
        Fabric::default()
    }

    pub fn add_region(&mut self, name: impl Into<String>) -> RegionId {
        self.regions.push(Region { name: name.into(), cells: vec![] });
        self.tables.ensure_rows(self.regions.len());
        RegionId(self.regions.len() as u32 - 1)
    }

    /// Append a cell to the end of a region. Ids are assigned densely
    /// (id == slab.len()); never reused even after removals.
    pub fn add_cell(&mut self, region: RegionId, cell: Cell) -> CellId {
        let id = self.place_cell_at(region, self.slab.len(), cell);
        id
    }

    /// Insert a cell at a specific position in the region's cell order
    /// (still gets a fresh id at the end of the slab).
    pub fn insert_cell(&mut self, region: RegionId, index: usize, cell: Cell) -> CellId {
        self.place_cell_at(region, index, cell)
    }

    fn place_cell_at(&mut self, region: RegionId, index: usize, cell: Cell) -> CellId {
        assert_eq!(cell.region, region, "cell.region must match placement region");
        let id = CellId(self.slab.len() as u32);
        self.slab.push(Some(cell.clone()));
        let r = self
            .regions
            .get_mut(region.0 as usize)
            .unwrap_or_else(|| panic!("place in nonexistent region {:?}", region));
        let idx = index.min(r.cells.len());
        r.cells.insert(idx, id);
        self.register_cell(id, idx);
        id
    }

    /// Table maintenance for a freshly-placed cell: register its use
    /// edges, and — when it landed at the END of its region — recompute
    /// that region's successor row (the old scan derived succs from the
    /// last cell; any edit that changes which cell is last, or its
    /// kind, must refresh the row). O(degree of the placed cell).
    pub(crate) fn register_cell(&mut self, id: CellId, idx_in_region: usize) {
        // keep the users index in lockstep with the slab (holes included)
        while self.tables.users.len() < self.slab.len() {
            self.tables.users.push(vec![]);
        }
        let cell = match self.cell(id) {
            Some(c) => c.clone(),
            None => return,
        };
        for (slot, &op) in cell.operands.iter().enumerate() {
            self.tables.add_use(op, id, slot as u32);
        }
        let cells = &self.regions[cell.region.0 as usize].cells;
        if cells.len() == idx_in_region + 1 && cells.last() == Some(&id) {
            self.tables.set_succs(cell.region, kind_succs(&cell.kind));
        }
    }

    /// Place a cell under an explicit id (used by the parser and by
    /// history replay). Pads the slab with holes if the id skips ahead.
    /// Errors on double-assignment.
    pub fn place_cell(&mut self, id: CellId, cell: Cell) -> Result<(), String> {
        let region = cell.region;
        let idx = id.0 as usize;
        if idx < self.slab.len() {
            if self.slab[idx].is_some() {
                return Err(format!("cell {} assigned twice", id));
            }
            self.slab[idx] = Some(cell);
        } else {
            while self.slab.len() < idx {
                self.slab.push(None);
            }
            self.slab.push(Some(cell));
        }
        let r = self
            .regions
            .get_mut(region.0 as usize)
            .ok_or_else(|| format!("cell {} placed in nonexistent region {}", id, region))?;
        r.cells.push(id);
        let at = r.cells.len() - 1;
        self.register_cell(id, at);
        Ok(())
    }

    /// REMOVE a cell through the sanctioned vocabulary: slab hole,
    /// region-list removal, and use/succ table repair — all O(degree
    /// of the removed cell). No tombstone: for a ledgered death use
    /// `forget`; for pass-time removals pair this with an Edit.
    /// Returns the removed cell, or None if absent.
    pub fn remove_cell(&mut self, id: CellId) -> Option<Cell> {
        let cell = self.cell(id)?.clone();
        let cells = &mut self.regions[cell.region.0 as usize].cells;
        let pos = cells.iter().position(|&c| c == id)?;
        let was_last = pos + 1 == cells.len();
        cells.remove(pos);
        self.slab[id.0 as usize] = None;
        self.unregister_cell(id, &cell, was_last, cell.region);
        Some(cell)
    }

    /// Table maintenance for a removed cell: drop its outgoing use
    /// edges; if it was its region's last cell, recompute that
    /// region's successors from whatever is last now (possibly
    /// nothing — a region without a terminator has no succs).
    /// O(degree of the removed cell).
    fn unregister_cell(&mut self, id: CellId, cell: &Cell, was_last: bool, region: RegionId) {
        for (slot, &op) in cell.operands.iter().enumerate() {
            self.tables.remove_use(op, id, slot as u32);
        }
        if was_last {
            let new_succs = self
                .regions
                .get(region.0 as usize)
                .and_then(|r| r.cells.last().copied())
                .and_then(|last| self.cell(last))
                .map(|c| kind_succs(&c.kind))
                .unwrap_or_default();
            self.tables.set_succs(region, new_succs);
        }
    }

    /// REWIRE one operand slot (a "rewire"/edge edit): user.slot now
    /// reads `to`. O(1) table work plus the row moves. Returns the old
    /// operand (None when user/slot absent). Forging paths that need
    /// out-of-bounds operands may keep raw `cell_mut` — V01 catches
    /// them before any table query matters.
    pub fn retarget(&mut self, user: CellId, slot: u32, to: CellId) -> Option<CellId> {
        let from = *self.cell(user)?.operands.get(slot as usize)?;
        if from == to {
            return Some(from);
        }
        self.cell_mut(user)?.operands[slot as usize] = to;
        self.tables.move_use(from, to, user, slot);
        Some(from)
    }

    /// Swap a cell's kind through the sanctioned vocabulary. When the
    /// cell is its region's terminator-in-last-position, the succ/pred
    /// tables are repaired (Br→Jmp, Br→Ret, ...). Operand wires are
    /// untouched — pair with `set_operands`/`retarget` as needed.
    /// Returns the old kind.
    pub fn set_kind(&mut self, id: CellId, kind: CellKind) -> Option<CellKind> {
        let c = self.cell_mut(id)?;
        let old = std::mem::replace(&mut c.kind, kind);
        let region = c.region;
        let is_last = self
            .regions
            .get(region.0 as usize)
            .and_then(|r| r.cells.last())
            .map(|&l| l == id)
            .unwrap_or(false);
        if is_last {
            let new_succs = kind_succs(&self.cell(id).expect("present").kind);
            self.tables.set_succs(region, new_succs);
        }
        Some(old)
    }

    /// Replace a cell's whole operand list (bulk rewire — e.g. a phi
    /// dropping a join+operand pair). O(old + new degree).
    pub fn set_operands(&mut self, id: CellId, ops: &[CellId]) -> Option<()> {
        let c = self.cell_mut(id)?;
        let old = std::mem::replace(&mut c.operands, ops.to_vec());
        for (slot, &op) in old.iter().enumerate() {
            self.tables.remove_use(op, id, slot as u32);
        }
        for (slot, &op) in ops.iter().enumerate() {
            self.tables.add_use(op, id, slot as u32);
        }
        Some(())
    }

    /// Re-derive the tables from the slab in O(n) — the ground truth
    /// the maintained tables must equal (the replay law applied to
    /// indexes). The recovery path for raw tooling that pokes
    /// `slab`/`regions` directly.
    pub fn rebuild_tables(&mut self) {
        self.tables = UseTables::derive(self);
    }

    pub fn cell(&self, id: CellId) -> Option<&Cell> {
        self.slab.get(id.0 as usize).and_then(|c| c.as_ref())
    }

    /// The tombstone of a forgotten cell, if any. Like tit-quilt's
    /// `tombstone_by_id`: last record wins (idempotent forget means at
    /// most one exists — V19 enforces it).
    pub fn tombstone_of(&self, id: CellId) -> Option<&Tombstone> {
        self.tombstones.iter().rev().find(|t| t.cell == id)
    }

    /// FORGET — the tit-quilt law, retrofitted: never delete, tombstone.
    /// Removes `id` from the slab (leaving its hole) and appends the
    /// tombstone carried by `cert`. The certificate must MATCH the cell
    /// it forgets (content hash), a fresh forget must find the cell
    /// present, and re-forgetting is idempotent (the existing tombstone
    /// stands; no duplicate is appended). There is no delete path.
    pub fn forget(&mut self, cert: &crate::decay::DeathCert) -> Result<(), String> {
        if self.tombstone_of(cert.cell).is_some() {
            return Ok(()); // idempotent: the record stands, nothing appended
        }
        let cell = self
            .cell(cert.cell)
            .ok_or_else(|| format!("forget {}: no such cell (and no tombstone)", cert.cell))?
            .clone();
        let want = crate::sign::fnv1a64(crate::text::render_cell(self, cert.cell).as_bytes());
        if want != cert.vhash {
            return Err(format!(
                "forget {} rejected: certificate hash {:016x} does not match the cell ({:016x}) — the certificate does not describe this cell",
                cert.cell, cert.vhash, want
            ));
        }
        if cell.operands != cert.witness {
            return Err(format!(
                "forget {} rejected: certificate witness list does not match the cell's operands",
                cert.cell
            ));
        }
        let region = cell.region;
        let cells = &mut self.regions[region.0 as usize].cells;
        let pos = cells
            .iter()
            .position(|&c| c == cert.cell)
            .ok_or_else(|| format!("forget {}: not listed in its region", cert.cell))?;
        let was_last = pos + 1 == cells.len();
        cells.remove(pos);
        self.slab[cert.cell.0 as usize] = None;
        self.unregister_cell(cert.cell, &cell, was_last, region);
        self.tombstones.push(cert.tombstone());
        Ok(())
    }

    pub fn cell_mut(&mut self, id: CellId) -> Option<&mut Cell> {
        self.slab.get_mut(id.0 as usize).and_then(|c| c.as_mut())
    }

    pub fn region(&self, r: RegionId) -> Option<&Region> {
        self.regions.get(r.0 as usize)
    }

    pub fn region_mut(&mut self, r: RegionId) -> Option<&mut Region> {
        self.regions.get_mut(r.0 as usize)
    }

    pub fn entry(&self) -> Option<RegionId> {
        if self.regions.is_empty() {
            None
        } else {
            Some(RegionId(0))
        }
    }

    pub fn region_name(&self, r: RegionId) -> &str {
        self.region(r).map(|x| x.name.as_str()).unwrap_or("<bad-region>")
    }

    /// All present cells, in id order.
    pub fn cells(&self) -> impl Iterator<Item = CellId> + '_ {
        (0..self.slab.len() as u32).map(CellId).filter(|&id| self.cell(id).is_some())
    }

    /// Every use edge in the fabric, in deterministic order
    /// (user id asc, slot asc).
    pub fn wires(&self) -> Vec<Wire> {
        let mut out = vec![];
        for id in self.cells() {
            let c = self.cell(id).expect("cells() yields present ids");
            for (slot, &from) in c.operands.iter().enumerate() {
                out.push(Wire { from, to: id, slot: slot as u32 });
            }
        }
        out
    }

    /// Reverse of wires: every user of `id`, as (user, slot) — O(degree)
    /// table lookup (was: a scan over every cell and slot). Rows are
    /// ordered user-asc, slot-asc, exactly as the scan produced.
    pub fn uses_of(&self, id: CellId) -> &[(CellId, u32)] {
        self.tables
            .users
            .get(id.0 as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Successor regions of a region (from its terminator), deduplicated
    /// — O(degree) table lookup (was: read+match the last cell).
    pub fn successors(&self, r: RegionId) -> &[RegionId] {
        self.tables
            .succs
            .get(r.0 as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Predecessor regions of a region, deduplicated, ascending —
    /// O(degree) table lookup (was: a scan over every region × its
    /// successors).
    pub fn predecessors(&self, r: RegionId) -> &[RegionId] {
        self.tables
            .preds
            .get(r.0 as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Index of a cell within its region's cell order.
    pub fn index_in_region(&self, id: CellId) -> Option<usize> {
        let c = self.cell(id)?;
        self.region(c.region)?.cells.iter().position(|&x| x == id)
    }

    pub fn ty_of(&self, id: CellId) -> Option<crate::ty::Type> {
        self.cell(id).and_then(|c| c.ty_of(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ty::{ConstVal, Type};

    fn build_entry_only() -> Fabric {
        let mut f = Fabric::empty();
        let entry = f.add_region("entry");
        let p = f.add_cell(entry, Cell::new(entry, CellKind::Param { ty: Type::I32 }));
        let c = f.add_cell(
            entry,
            Cell::new(entry, CellKind::Const { ty: Type::I32, val: ConstVal::I32(42) }),
        );
        let mut a = Cell::new(entry, CellKind::Arith { op: crate::cell::ArithOp::Add, ty: Type::I32 });
        a.operands = vec![p, c];
        let a = f.add_cell(entry, a);
        let mut ret = Cell::new(entry, CellKind::Ret);
        ret.operands = vec![a];
        f.add_cell(entry, ret);
        f
    }

    #[test]
    fn slab_ids_dense_and_stable() {
        let f = build_entry_only();
        assert_eq!(f.slab.len(), 4);
        assert_eq!(f.cell(CellId(3)).unwrap().is_terminator(), true);
    }

    #[test]
    fn wires_follow_operand_order() {
        let f = build_entry_only();
        let w = f.wires();
        // 2 wires into the add, 1 wire into the ret.
        assert_eq!(w.len(), 3);
        assert!(w.contains(&Wire { from: CellId(0), to: CellId(2), slot: 0 }));
        assert!(w.contains(&Wire { from: CellId(1), to: CellId(2), slot: 1 }));
        assert!(w.contains(&Wire { from: CellId(2), to: CellId(3), slot: 0 }));
    }

    #[test]
    fn uses_of_reverse_of_wires() {
        let f = build_entry_only();
        assert_eq!(f.uses_of(CellId(2)), vec![(CellId(3), 0)]);
    }

    #[test]
    fn place_cell_allows_holes_for_parser() {
        let mut f = Fabric::empty();
        let entry = f.add_region("entry");
        f.place_cell(
            CellId(0),
            Cell::new(entry, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }),
        )
        .unwrap();
        // id 1 skipped: slab grows to len 2 with a hole; id 2 lands next.
        f.place_cell(
            CellId(2),
            Cell::new(entry, CellKind::Const { ty: Type::I1, val: ConstVal::I1(false) }),
        )
        .unwrap();
        assert_eq!(f.slab.len(), 3);
        assert!(f.cell(CellId(1)).is_none());
        assert_eq!(f.region(entry).unwrap().cells, vec![CellId(0), CellId(2)]);
        // double assignment must fail, not panic
        assert!(f.place_cell(CellId(0), Cell::new(entry, CellKind::Ret)).is_err());
    }
}

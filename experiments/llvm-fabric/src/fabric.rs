//! The fabric: regions of cells, wires as use edges, slab storage with
//! stable ids. Ids are never reused; removed cells leave `None` holes
//! (N4: history appends, ids persist).

use crate::cell::{Cell, CellKind};
use crate::id::{CellId, RegionId};

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

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Fabric {
    /// regions[0] is the entry region, when any.
    pub regions: Vec<Region>,
    /// Cell slab. `None` = removed (or not yet assigned). Index = CellId.
    pub slab: Vec<Option<Cell>>,
}

impl Fabric {
    pub fn empty() -> Fabric {
        Fabric::default()
    }

    pub fn add_region(&mut self, name: impl Into<String>) -> RegionId {
        self.regions.push(Region { name: name.into(), cells: vec![] });
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
        id
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
        Ok(())
    }

    pub fn cell(&self, id: CellId) -> Option<&Cell> {
        self.slab.get(id.0 as usize).and_then(|c| c.as_ref())
    }

    pub fn cell_mut(&mut self, id: CellId) -> Option<&mut Cell> {
        self.slab.get_mut(id.0 as usize).and_then(|c| c.as_mut())
    }

    pub fn region(&self, r: RegionId) -> Option<&Region> {
        self.regions.get(r.0 as usize)
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

    /// Reverse of wires: every user of `id`, as (user, slot).
    pub fn uses_of(&self, id: CellId) -> Vec<(CellId, u32)> {
        let mut out = vec![];
        for user in self.cells() {
            if let Some(c) = self.cell(user) {
                for (slot, &op) in c.operands.iter().enumerate() {
                    if op == id {
                        out.push((user, slot as u32));
                    }
                }
            }
        }
        out
    }

    /// Successor regions of a region (from its terminator), deduplicated.
    pub fn successors(&self, r: RegionId) -> Vec<RegionId> {
        let mut out = vec![];
        if let Some(region) = self.region(r) {
            if let Some(&last) = region.cells.last() {
                if let Some(c) = self.cell(last) {
                    match &c.kind {
                        CellKind::Branch { then_r, else_r } => {
                            for t in [*then_r, *else_r] {
                                if !out.contains(&t) {
                                    out.push(t);
                                }
                            }
                        }
                        CellKind::Jump { target } => out.push(*target),
                        CellKind::Ret => {}
                        _ => {}
                    }
                }
            }
        }
        out
    }

    /// Predecessor regions of a region, deduplicated, ascending.
    pub fn predecessors(&self, r: RegionId) -> Vec<RegionId> {
        let mut out = vec![];
        for (i, _) in self.regions.iter().enumerate() {
            let from = RegionId(i as u32);
            if self.successors(from).contains(&r) && !out.contains(&from) {
                out.push(from);
            }
        }
        out
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

//! Newtype ids. Cheap to copy, hard to confuse.

/// A cell's identity. Ids are slab indices; once assigned, never reused.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct CellId(pub u32);

/// A region's identity (index into `Fabric::regions`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct RegionId(pub u32);

impl CellId {
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl RegionId {
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for CellId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "%{}", self.0)
    }
}

impl std::fmt::Display for RegionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

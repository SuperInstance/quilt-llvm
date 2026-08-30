//! Cells: the unit of compilation and the unit of quilt.
//!
//! A cell is one inspectable step: it has a kind, zero or more operand
//! wires (use edges), and lives in exactly one region. Values flow along
//! wires from def to use; provenance is walking wires backwards.

use crate::fabric::Fabric;
use crate::id::CellId;
use crate::ty::{ConstVal, Type};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl ArithOp {
    pub fn name(self) -> &'static str {
        match self {
            ArithOp::Add => "arith.add",
            ArithOp::Sub => "arith.sub",
            ArithOp::Mul => "arith.mul",
            ArithOp::Div => "arith.div",
        }
    }

    pub fn parse(s: &str) -> Option<ArithOp> {
        match s {
            "arith.add" => Some(ArithOp::Add),
            "arith.sub" => Some(ArithOp::Sub),
            "arith.mul" => Some(ArithOp::Mul),
            "arith.div" => Some(ArithOp::Div),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    pub fn name(self) -> &'static str {
        match self {
            CmpOp::Eq => "cmp.eq",
            CmpOp::Ne => "cmp.ne",
            CmpOp::Lt => "cmp.lt",
            CmpOp::Le => "cmp.le",
            CmpOp::Gt => "cmp.gt",
            CmpOp::Ge => "cmp.ge",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum CellKind {
    /// Function parameter. Only meaningful in the entry region (v0).
    Param { ty: Type },
    /// A constant value.
    Const { ty: Type, val: ConstVal },
    /// Binary arithmetic on two operands of type `ty`.
    Arith { op: ArithOp, ty: Type },
    /// Comparison of two same-typed operands; produces i1.
    Cmp { op: CmpOp },
    /// Conditional terminator: operands[0] is the i1 condition.
    Branch { then_r: crate::id::RegionId, else_r: crate::id::RegionId },
    /// Unconditional terminator.
    Jump { target: crate::id::RegionId },
    /// Wire join (LLVM phi): joins[i] is the predecessor region from which
    /// operands[i] carries its value. joins.len() == operands.len().
    Phi { joins: Vec<crate::id::RegionId> },
    /// Return terminator. operands[0] is the returned value, if any.
    Ret,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Cell {
    /// The region this cell lives in (denormalized for O(1) lookup).
    pub region: crate::id::RegionId,
    pub kind: CellKind,
    /// Operand wires, in slot order. Slot indices are stable and are what
    /// Retarget edits address.
    pub operands: Vec<CellId>,
}

impl Cell {
    pub fn new(region: crate::id::RegionId, kind: CellKind) -> Cell {
        let operands = match &kind {
            CellKind::Param { .. } | CellKind::Const { .. } => vec![],
            CellKind::Arith { .. } => vec![],
            CellKind::Cmp { .. } => vec![],
            CellKind::Branch { .. } => vec![],
            CellKind::Jump { .. } => vec![],
            CellKind::Phi { joins } => vec![CellId(u32::MAX); joins.len()],
            CellKind::Ret => vec![],
        };
        Cell { region, kind, operands }
    }

    /// Terminators end a region; exactly one allowed, and it must be last.
    pub fn is_terminator(&self) -> bool {
        matches!(self.kind, CellKind::Branch { .. } | CellKind::Jump { .. } | CellKind::Ret)
    }

    /// Produces a value other cells can use as an operand.
    pub fn produces_value(&self) -> bool {
        !self.is_terminator()
    }

    /// The type this cell produces, if it produces a value. Phi types come
    /// from the first operand, so this needs the fabric; see Fabric::ty_of.
    pub fn ty_of(&self, f: &Fabric) -> Option<Type> {
        match &self.kind {
            CellKind::Param { ty } => Some(*ty),
            CellKind::Const { ty, .. } => Some(*ty),
            CellKind::Arith { ty, .. } => Some(*ty),
            CellKind::Cmp { .. } => Some(Type::I1),
            CellKind::Phi { .. } => {
                let first = self.operands.first()?;
                f.cell(*first).and_then(|c| c.ty_of(f))
            }
            CellKind::Branch { .. } | CellKind::Jump { .. } | CellKind::Ret => None,
        }
    }
}

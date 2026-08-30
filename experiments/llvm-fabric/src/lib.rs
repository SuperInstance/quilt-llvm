//! llvm-fabric v0 — the experimentation spike for quilt-llvm.
//!
//! A program is a fabric of cells; change travels on wires; every
//! transform appends to history (N4: append, never rewrite).
//!
//! Zero external dependencies on purpose: the spike must build fast,
//! deterministically, and be auditable line by line.

pub mod id;
pub mod ty;
pub mod cell;
pub mod fabric;
pub mod text;
pub mod verify;
pub mod fuzz;
pub mod diff;
pub mod conserve;
pub mod replay;
pub mod passes;
pub mod prov;
pub mod pipeline;

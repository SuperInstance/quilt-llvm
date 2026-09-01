//! Real passes over the fabric. Each pass is a pure function
//! `fabric -> (fabric, diff)`; the caller appends the diff to history.
//! Passes refuse unverified input and must leave verified, conserving
//! output (tested).

pub mod cfg_inline;
pub mod const_branch;
pub mod constfold;
pub mod dce;
pub mod inline;
pub mod region_dce;
pub mod verdict;

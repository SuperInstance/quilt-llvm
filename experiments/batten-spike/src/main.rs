//! BATTEN-SPIKE: verified-outcome routing inside a toy pass pipeline.
//!
//! Question (small, measurable): can verified pass outcomes act as
//! *battens* (anchor posts, per batten-spline's epistemology) for routing
//! a new fabric through candidate pass pipelines?
//!
//! Method:
//!   1. generate a corpus of random fabrics (llvm-fabric fuzz generator);
//!   2. run every candidate pipeline on every training fabric, measuring
//!      cost (cells processed) and benefit (size reduction, verify-clean);
//!      these measured outcomes are the battens;
//!   3. key each batten by cheap fabric features (cell count, op mix, depth);
//!   4. route NEW fabrics: pick the pipeline with the best interpolated
//!      expected score (Nadaraya-Watson over an RBF kernel) from nearby
//!      battens — one spline per (pipeline, metric);
//!   5. compare routed choice vs exhaustive-best (oracle) choice.
//!
//! The kernel is a minimal Rust reimplementation of batten-spline's
//! estimator (see README.md for the library-vs-reimplement call).

mod features;
mod kernel;
mod measure;
mod route;

fn main() {
    route::experiment();
}

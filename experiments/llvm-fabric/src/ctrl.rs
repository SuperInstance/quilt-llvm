//! Control wires: explicit control edges + the full provenance walk.
//!
//! v0 booked the gap (EXPERIMENTS.md §4(a) "Surprise" and §5 item 2):
//! data provenance does not cross control edges — a phi's def-chain walk
//! never reaches the branch condition that selected it, because the
//! branch is control, not a data wire.
//!
//! This module closes that gap:
//! - `ctrl_edges` — the explicit control edges of the fabric: one edge
//!   per (terminator T, successor region S). Terminators are the only
//!   source of ctrl-wires (ARCHITECTURE §1.1); a phi is a mux whose
//!   select lines are the incoming terminator wires ([K-r1]).
//! - `controlling_terminators(f, region)` — the backward closure over
//!   those edges: every terminator from which `region` is reachable.
//!   Those are the cells that gate whether ANY cell in `region` fires.
//! - `full_provenance(f, id)` — data walk (operands, as v0) PLUS control
//!   walk (the controlling terminators of the cell's region, each with
//!   its condition's data subtree). Cycle-safe: fabrics may be cyclic
//!   (loops); each region's closure is visited once.
//!
//! Honest scope (stated, not hidden): the full walk includes the control
//! closure of the cell's OWN region only. A phi additionally depends on
//! the mux inputs from its join regions — those are already its data
//! operands. The mux select lines (terminators of the join regions) are
//! inside the closure of the phi's region whenever the join regions are
//! predecessors, which V16 (phi covers all preds) now guarantees.

use crate::cell::CellKind;
use crate::fabric::Fabric;
use crate::id::{CellId, RegionId};
use crate::prov::ProvNode;

/// One control edge: terminator `term` (in some region R) routes control
/// into region `gated`. Terminators are the only ctrl-wire sources.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CtrlEdge {
    pub term: CellId,
    pub gated: RegionId,
}

/// All control edges, deterministic order: gated region index asc, then
/// terminator id asc.
pub fn ctrl_edges(f: &Fabric) -> Vec<CtrlEdge> {
    let mut out = vec![];
    for (ri, _) in f.regions.iter().enumerate() {
        let r = RegionId(ri as u32);
        for succ in f.successors(r) {
            // every terminator of r targets succ (br has two, jump one);
            // find r's terminator cell
            if let Some(region) = f.region(r) {
                if let Some(&last) = region.cells.last() {
                    if let Some(c) = f.cell(last) {
                        if c.is_terminator() {
                            out.push(CtrlEdge { term: last, gated: succ });
                        }
                    }
                }
            }
        }
    }
    out.sort_by_key(|e| (e.gated.0, e.term.0));
    out
}

/// Regions from which `target` is reachable (path length >= 1), i.e. the
/// regions whose terminators gate control flow into `target`. `target`
/// itself is included iff it lies on a cycle (its own re-entry is then
/// gated by its own terminator). The entry region appears whenever it
/// has a path to `target` — its terminator is a legitimate gate.
///
/// Cycle-safe by construction (visited set), deterministic order.
pub fn controlling_regions(f: &Fabric, target: RegionId) -> Vec<RegionId> {
    let mut seen: Vec<RegionId> = vec![];
    let mut work = vec![target];
    while let Some(r) = work.pop() {
        for p in f.predecessors(r) {
            if !seen.contains(&p) {
                seen.push(p);
                work.push(p);
            }
        }
    }
    seen.sort_by_key(|r| r.0);
    seen
}

/// The terminator cells gating control flow into `target`'s cells: the
/// terminators of every region in the backward closure.
pub fn controlling_terminators(f: &Fabric, target: RegionId) -> Vec<CellId> {
    let mut out = vec![];
    for r in controlling_regions(f, target) {
        if let Some(region) = f.region(r) {
            if let Some(&last) = region.cells.last() {
                if let Some(c) = f.cell(last) {
                    if c.is_terminator() {
                        out.push(last);
                    }
                }
            }
        }
    }
    out.sort_by_key(|&t| t.0);
    out
}

/// Full provenance: data walk (operands, exactly as v0) + control walk
/// (the controlling terminators of the queried cell's region, each with
/// its condition's data subtree). Cycle-safe: fabrics may be cyclic
/// (loops); genuine loop-carried self-influences are CUT with an
/// explicit `revisit:` marker.
///
/// Completeness note (why ctrl expansion happens ONLY at the root, and
/// why that loses nothing): for a verified fabric, every data ancestor
/// of the queried cell lives in (a) the same region — same closure — or
/// (b) the entry region — empty closure — or (c) a phi join region P,
/// which is a predecessor of the cell's region, and everything that
/// reaches P also reaches the cell's region, so closure(P) ⊆
/// closure(root). The root's closure covers them all.
pub fn full_provenance(f: &Fabric, target: CellId) -> Result<ProvNode, String> {
    let mut visiting: Vec<CellId> = vec![];
    full_walk(f, target, &mut visiting, 0, true)
}

fn full_walk(
    f: &Fabric,
    id: CellId,
    visiting: &mut Vec<CellId>,
    depth: u32,
    at_root: bool,
) -> Result<ProvNode, String> {
    let c = f.cell(id).ok_or_else(|| format!("no such cell {}", id))?;
    if depth > 100_000 {
        return Err(format!("full provenance depth guard at {}", id));
    }
    if visiting.contains(&id) {
        // A genuine loop-carried influence: in a cyclic fabric, a value's
        // iteration-k+1 self genuinely depends on its iteration-k self
        // (data -> control gate -> back edge -> data). We cut the cycle
        // with an explicit marker instead of diverging — or erroring:
        // the cycle is real information, not corruption (found by the
        // corpus, seed 1026845; kept as the regression test below).
        return Ok(ProvNode {
            id,
            line: format!("revisit: {}", crate::text::render_cell(f, id)),
            children: vec![],
        });
    }
    let mut children = vec![];
    let is_leaf_kind = matches!(c.kind, CellKind::Param { .. } | CellKind::Const { .. });
    // The node stays on `visiting` across BOTH its data and ctrl blocks:
    // a control gate whose condition feeds back into the gated cell is a
    // genuine loop-carried influence, and must surface as a revisit
    // marker (not a duplicated subtree). Markers can only appear when
    // the REGION graph is cyclic — in acyclic region graphs, a ctrl
    // condition's data chain cannot reach a node of a descendant region.
    if !is_leaf_kind {
        visiting.push(id);
    }
    // 1. data children (as in v0)
    if !is_leaf_kind {
        for &op in &c.operands {
            children.push(full_walk(f, op, visiting, depth + 1, false)?);
        }
    }
    // 2. control children (root only — see completeness note above)
    if at_root {
        let region = c.region;
        if Some(region) != f.entry() {
            for term in controlling_terminators(f, region) {
                let line = format!("ctrl: {}", crate::text::render_cell(f, term));
                let mut ctrl_children = vec![];
                if let Some(tc) = f.cell(term) {
                    visiting.push(term);
                    for &op in &tc.operands {
                        // the branch condition's data subtree rides along
                        ctrl_children.push(full_walk(f, op, visiting, depth + 1, false)?);
                    }
                    visiting.pop();
                }
                children.push(ProvNode { id: term, line, children: ctrl_children });
            }
        }
    }
    if !is_leaf_kind {
        visiting.pop();
    }
    let line = crate::text::render_cell(f, id);
    Ok(ProvNode { id, line, children })
}

/// Correctness check used by the corpus: the full walk terminates, every
/// data leaf is a root (param/const), every ctrl child is a real
/// terminator, no cell is invented, and — at the ROOT — every controlling
/// terminator of the root's region is shown. Below the root, data nodes
/// may sit in other regions (phi operands live in pred regions), so each
/// ctrl child is checked to be a real terminator of the walk, not against
/// the root's closure.
pub fn check_full_prov(f: &Fabric, id: CellId) -> Result<(), String> {
    let node = full_provenance(f, id)?;
    let region = f
        .cell(id)
        .ok_or_else(|| format!("no such cell {}", id))?
        .region;
    if f.cell(node.id).is_none() {
        return Err(format!("full provenance invented cell {}", node.id));
    }
    // root completeness: the root's own closure is fully shown
    let allowed_ctrl = controlling_terminators(f, region);
    if Some(region) != f.entry() {
        let shown: Vec<CellId> = node
            .children
            .iter()
            .filter(|c| c.line.starts_with("ctrl: "))
            .map(|c| c.id)
            .collect();
        for &t in &allowed_ctrl {
            if !shown.contains(&t) {
                return Err(format!(
                    "full provenance of {} misses controlling terminator {}",
                    id, t
                ));
            }
        }
    }
    check_node(f, &node, true)
}

fn check_node(f: &Fabric, n: &ProvNode, at_root: bool) -> Result<(), String> {
    for ch in &n.children {
        if f.cell(ch.id).is_none() {
            return Err(format!("full provenance invented cell {}", ch.id));
        }
        if ch.line.starts_with("ctrl: ") {
            if !at_root {
                return Err(format!("ctrl child {} below the root (structure)", ch.id));
            }
            let is_term = f.cell(ch.id).map(|c| c.is_terminator()).unwrap_or(false);
            if !is_term {
                return Err(format!("ctrl child {} is not a terminator", ch.id));
            }
        } else if ch.line.starts_with("revisit: ") {
            // loop-carried cut point; existence checked above
        } else if ch.children.is_empty() {
            let is_root = f
                .cell(ch.id)
                .map(|c| matches!(c.kind, CellKind::Param { .. } | CellKind::Const { .. }))
                .unwrap_or(false);
            if !is_root {
                return Err(format!("full provenance has non-root leaf {}", ch.id));
            }
        } else {
            if f.cell(ch.id).map(|c| c.produces_value()) != Some(true) {
                return Err(format!("non-value cell {} inside data provenance tree", ch.id));
            }
        }
        check_node(f, ch, false)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prov::render;

    /// The diamond from v0's docs: the exact shape whose data walk could
    /// not cross the control edge.
    fn diamond() -> Fabric {
        let text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = const i32 42\n\
  %2 = arith.add i32 %0, %1\n\
  %3 = cmp.lt %2, %1\n\
  %5 = br %3, then, else\n\
region then\n\
  %4 = const i64 1i64\n\
  %6 = jump join\n\
region else\n\
  %7 = const i64 2i64\n\
  %8 = jump join\n\
region join\n\
  %9 = phi [then: %4] [else: %7]\n\
  %10 = ret %9\n";
        crate::text::parse(text).expect("sample parses")
    }

    #[test]
    fn ctrl_edges_are_the_terminator_wires() {
        let f = diamond();
        let edges = ctrl_edges(&f);
        // entry's br gates then and else; then's jump and else's jump gate join
        assert_eq!(edges.len(), 4, "edges: {:?}", edges);
        assert!(edges.contains(&CtrlEdge { term: CellId(5), gated: RegionId(1) })); // entry->then
        assert!(edges.contains(&CtrlEdge { term: CellId(5), gated: RegionId(2) })); // entry->else
        assert!(edges.contains(&CtrlEdge { term: CellId(6), gated: RegionId(3) })); // then->join
        assert!(edges.contains(&CtrlEdge { term: CellId(8), gated: RegionId(3) })); // else->join
    }

    #[test]
    fn closure_of_join_is_all_three_regions_terminators() {
        let f = diamond();
        let ts = controlling_terminators(&f, RegionId(3)); // join
        assert_eq!(ts, vec![CellId(5), CellId(6), CellId(8)], "br + both jumps");
        let entry_ts = controlling_terminators(&f, RegionId(0));
        assert!(entry_ts.is_empty(), "entry has no in-fabric control origin");
    }

    /// THE test: v0's booked gap, closed. The phi's FULL provenance now
    /// reaches the branch condition AND, through it, the param that v0's
    /// data walk could never reach (prov::tests::
    /// provenance_of_phi_reaches_all_roots asserts the opposite for the
    /// data-only walk — both tests stay, as the red/green pair).
    #[test]
    fn full_provenance_of_phi_crosses_the_control_edge() {
        let f = diamond();
        let node = full_provenance(&f, CellId(9)).expect("walk");
        let r = render(&node);
        assert!(r.contains("ctrl: %5 = br %3, then, else"), "{}", r);
        assert!(r.contains("%3 = cmp.lt %2, %1"), "{}", r);
        assert!(r.contains("%2 = arith.add i32 %0, %1"), "{}", r);
        // the payoff line: the param feeding the branch condition
        assert!(r.contains("%0 = param i32"), "full walk must reach the branch cond's roots: {}", r);
        // the mux select lines: the jump terminators of the join preds
        assert!(r.contains("ctrl: %6 = jump join"), "{}", r);
        assert!(r.contains("ctrl: %8 = jump join"), "{}", r);
        // data side unchanged: both arm consts still present
        assert!(r.contains("const i64 1i64") && r.contains("const i64 2i64"), "{}", r);
        assert!(check_full_prov(&f, CellId(9)).is_ok());
    }

    #[test]
    fn entry_cells_have_no_ctrl_children() {
        let f = diamond();
        let node = full_provenance(&f, CellId(2)).expect("walk");
        let r = render(&node);
        assert!(!r.contains("ctrl:"), "entry data walk must stay data-only: {}", r);
        assert!(r.contains("param i32"), "{}", r);
    }

    #[test]
    fn loops_do_not_hang_the_closure() {
        // self-loop: body branches back to itself and to exit
        let text = "fabric v0\n\
region entry\n\
  %0 = const i1 true\n\
  %1 = br %0, body, exit\n\
region body\n\
  %2 = const i32 7\n\
  %3 = const i1 false\n\
  %4 = br %3, body, exit\n\
region exit\n\
  %5 = ret\n";
        let f = crate::text::parse(text).expect("loop fabric parses");
        assert!(crate::verify::verify(&f).is_ok());
        // body is in a cycle with itself; its own terminator gates re-entry
        let ts = controlling_terminators(&f, RegionId(1));
        assert_eq!(ts, vec![CellId(1), CellId(4)], "entry br + own br (self-loop)");
        let node = full_provenance(&f, CellId(2)).expect("walk over loop closes");
        let r = render(&node);
        assert!(r.contains("ctrl: %1 = br"), "{}", r);
        assert!(r.contains("ctrl: %4 = br"), "{}", r);
        assert!(check_full_prov(&f, CellId(2)).is_ok());
    }

    #[test]
    fn missing_cell_is_still_an_error() {
        let f = diamond();
        assert!(full_provenance(&f, CellId(99)).is_err());
    }

    /// Regression: the corpus (seed 1026845) found a genuine loop-carried
    /// influence cycle — add -> (ctrl gate of its own region) -> branch
    /// cond -> back to the add. The walk must CUT with a marker, not error
    /// or diverge.
    #[test]
    fn loop_carried_influence_is_cut_with_a_marker() {
        // condensed from the corpus fabric: r1 loops to itself; %8 feeds
        // the cond that gates r1; %8 also depends on r1-gated cells
        let text = "fabric v0\n\
region r0\n\
%0 = param i32\n\
%5 = const i32 -405\n\
%16 = jump r1\n\
region r1\n\
%8 = arith.add i32 %5, %0\n\
%9 = cmp.gt %8, %5\n\
%14 = phi [r0: %5] [r1: %8]\n\
%17 = br %9, r1, r1\n";
        let f = crate::text::parse(text).expect("loops parse");
        assert!(crate::verify::verify(&f).is_ok());
        let node = full_provenance(&f, CellId(8)).expect("loop fabric walks");
        let r = render(&node);
        assert!(
            r.contains("revisit: %8"),
            "the loop-carried self-dependence must be marked, not hidden: {}",
            r
        );
        assert!(check_full_prov(&f, CellId(8)).is_ok());
        // and the same fabric's NON-cyclic cell still walks clean
        let r2 = render(&full_provenance(&f, CellId(9)).unwrap());
        assert!(r2.contains("ctrl: %16 = jump"), "{}", r2);
        assert!(r2.contains("ctrl: %17 = br"), "{}", r2);
        assert!(check_full_prov(&f, CellId(9)).is_ok());
    }
}

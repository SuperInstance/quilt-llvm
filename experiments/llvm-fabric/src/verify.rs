//! The fabric verifier. Random fabrics must either verify or fail with a
//! precise reason — never a panic (the fuzzer enforces this).
//!
//! v0 scope rules (conservative on purpose, documented in EXPERIMENTS.md):
//! - a non-phi use must see its operand defined earlier in the SAME region,
//!   or anywhere in the entry region (entry is the unique root);
//! - a phi operand for predecessor P must be defined in P or in entry;
//! - reachability is NOT required (unreachable regions are legal fabric).

use crate::cell::{Cell, CellKind};
use crate::fabric::Fabric;
use crate::id::{CellId, RegionId};
use crate::ty::Type;
use std::collections::BTreeMap;

pub type Code = &'static str;

#[derive(Debug, PartialEq, Clone)]
pub struct VerifyError {
    pub code: Code,
    pub detail: String,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

fn fail(code: Code, detail: impl Into<String>) -> VerifyError {
    VerifyError { code, detail: detail.into() }
}

/// Verify a fabric. Returns the first error found (codes are stable and
/// quoted in docs/tests), or Ok(()) when the fabric is well formed.
pub fn verify(f: &Fabric) -> Result<(), VerifyError> {
    if f.regions.is_empty() {
        return Err(fail("V00", "fabric has no regions"));
    }

    // V01: region cell lists point at present cells; operands point at
    // present cells (in-bounds and Some).
    for (ri, region) in f.regions.iter().enumerate() {
        for &cid in &region.cells {
            match f.cell(cid) {
                None => {
                    return Err(fail(
                        "V01",
                        format!("dangling cell {} listed in region '{}'", cid, region.name),
                    ))
                }
                Some(c) => {
                    if c.region.0 as usize != ri {
                        return Err(fail(
                            "V01",
                            format!(
                                "cell {} lives in region {} but is listed in '{}'",
                                cid,
                                f.region_name(c.region),
                                region.name
                            ),
                        ));
                    }
                }
            }
        }
    }
    for id in f.cells() {
        let c = f.cell(id).expect("cells() yields present ids");
        for (slot, &op) in c.operands.iter().enumerate() {
            if f.cell(op).is_none() {
                return Err(fail(
                    "V01",
                    format!("dangling operand: {} slot {} -> {}", id, slot, op),
                ));
            }
        }
    }

    // V17: the operand graph must be acyclic. Cycles are only reachable
    // through phis (V12 blocks same-region use-before-def for non-phi
    // cells, but phis are exempt — a phi may join a value defined in a
    // later region, and that value may use entry cells, closing a loop
    // through the entry phi). A cycle also makes `Cell::ty_of` recurse
    // forever (phi type = first operand's type) — found by the corpus
    // (seed 1032071, mutation made a phi its own operand); v0 never saw
    // it because the generator never emitted phis (booked in
    // EXPERIMENTS.md). Checked HERE, before any ty_of call.
    {
        // 0 = unvisited, 1 = on stack, 2 = done
        let n = f.slab.len();
        let mut color = vec![0u8; n];
        for start in f.cells() {
            if color[start.0 as usize] != 0 {
                continue;
            }
            // iterative DFS with explicit stack of (cell, next-slot)
            let mut stack: Vec<(CellId, usize)> = vec![(start, 0)];
            color[start.0 as usize] = 1;
            while let Some(&mut (id, ref mut slot)) = stack.last_mut() {
                let ops = match f.cell(id) {
                    Some(c) => &c.operands,
                    None => {
                        stack.pop();
                        color[id.0 as usize] = 2;
                        continue;
                    }
                };
                if *slot >= ops.len() {
                    stack.pop();
                    color[id.0 as usize] = 2;
                    continue;
                }
                let next = ops[*slot];
                *slot += 1;
                match color.get(next.0 as usize).copied().unwrap_or(2) {
                    0 => {
                        color[next.0 as usize] = 1;
                        stack.push((next, 0));
                    }
                    1 => {
                        return Err(fail(
                            "V17",
                            format!("operand cycle: {} is its own (transitive) operand via {}", next, id),
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    // V02: region references are in bounds.
    for id in f.cells() {
        let c = f.cell(id).expect("present");
        match &c.kind {
            CellKind::Branch { then_r, else_r } => {
                for r in [*then_r, *else_r] {
                    if f.region(r).is_none() {
                        return Err(fail("V02", format!("{} branches to nonexistent region {}", id, r)));
                    }
                }
            }
            CellKind::Jump { target } => {
                if f.region(*target).is_none() {
                    return Err(fail("V02", format!("{} jumps to nonexistent region {}", id, target)));
                }
            }
            CellKind::Phi { joins } => {
                for &r in joins {
                    if f.region(r).is_none() {
                        return Err(fail("V02", format!("{} joins nonexistent region {}", id, r)));
                    }
                }
            }
            _ => {}
        }
    }

    // V03/V04: exactly one terminator per region, in final position.
    for (ri, region) in f.regions.iter().enumerate() {
        let rid = RegionId(ri as u32);
        if region.cells.is_empty() {
            return Err(fail("V03", format!("region '{}' has no terminator (empty)", region.name)));
        }
        for (pos, &cid) in region.cells.iter().enumerate() {
            let c = f.cell(cid).expect("checked in V01");
            if c.is_terminator() && pos + 1 != region.cells.len() {
                return Err(fail(
                    "V04",
                    format!("terminator {} is not last in '{}'", cid, region.name),
                ));
            }
        }
        let last = f.cell(*region.cells.last().expect("nonempty")).expect("checked");
        if !last.is_terminator() {
            return Err(fail(
                "V03",
                format!("region '{}' does not end in a terminator", region.name),
            ));
        }
        let _ = rid;
    }

    let entry = f.entry().expect("regions nonempty");

    // V05: phi arity — joins and operands aligned, at least one join.
    for id in f.cells() {
        let c = f.cell(id).expect("present");
        if let CellKind::Phi { joins } = &c.kind {
            if joins.is_empty() || joins.len() != c.operands.len() {
                return Err(fail(
                    "V05",
                    format!("phi {} has {} joins but {} operands", id, joins.len(), c.operands.len()),
                ));
            }
        }
    }

    // V06/V14: every phi join is a real predecessor edge, no duplicates.
    for id in f.cells() {
        let c = f.cell(id).expect("present");
        if let CellKind::Phi { joins } = &c.kind {
            let preds = f.predecessors(c.region);
            for (i, &r) in joins.iter().enumerate() {
                if !preds.contains(&r) {
                    return Err(fail(
                        "V06",
                        format!(
                            "phi {} join {} is '{}' but that region never branches to '{}'",
                            id,
                            i,
                            f.region_name(r),
                            f.region_name(c.region)
                        ),
                    ));
                }
                if joins[..i].contains(&r) {
                    return Err(fail("V14", format!("phi {} joins '{}' twice", id, f.region_name(r))));
                }
            }
        }
    }

    // V16: control well-formedness — a phi is a mux over its region's
    // incoming ctrl edges; every real predecessor must carry exactly one
    // join (V06/V14 checked realness/uniqueness above; this checks
    // COMPLETENESS). A predecessor edge without a join entry would be a
    // value arriving on an unselected path — the "silent no-op wire"
    // quilt-scratch's TILE-CONTRACT names as the worst failure mode.
    // Cited in EXPERIMENTS.md §v1; the book closing the v0 control-gap.
    for id in f.cells() {
        let c = f.cell(id).expect("present");
        if let CellKind::Phi { joins } = &c.kind {
            let preds = f.predecessors(c.region);
            for &p in preds {
                if !joins.contains(&p) {
                    return Err(fail(
                        "V16",
                        format!(
                            "phi {} in '{}' has no join for predecessor '{}' — control edge without a mux input",
                            id,
                            f.region_name(c.region),
                            f.region_name(p)
                        ),
                    ));
                }
            }
        }
    }

    // Scope + type checks per cell.
    for id in f.cells() {
        let c = f.cell(id).expect("present");
        match &c.kind {
            CellKind::Param { .. } => {
                if c.region != entry {
                    return Err(fail(
                        "V12",
                        format!("param {} outside entry region '{}'", id, f.region_name(c.region)),
                    ));
                }
            }
            CellKind::Const { ty, val } => {
                if val.ty() != *ty {
                    return Err(fail(
                        "V11",
                        format!("const {} declares {} but value is {}", id, ty.name(), val.ty().name()),
                    ));
                }
            }
            CellKind::Arith { ty, .. } => {
                check_operand_type(f, id, 0, *ty, "V08")?;
                check_operand_type(f, id, 1, *ty, "V08")?;
            }
            CellKind::Cmp { .. } => {
                let a = f
                    .ty_of(*c.operands.first().ok_or_else(|| fail("V01", format!("cmp {} has no operands", id)))?)
                    .ok_or_else(|| fail("V09", format!("cmp {} operand has no type", id)))?;
                check_operand_type(f, id, 1, a, "V09")?;
            }
            CellKind::Branch { .. } => {
                check_operand_type(f, id, 0, Type::I1, "V10")?;
            }
            CellKind::Phi { joins } => {
                let first_ty = f.ty_of(*c.operands.first().expect("V05 checked arity"));
                for (i, &op) in c.operands.iter().enumerate() {
                    let t = f.ty_of(op).ok_or_else(|| {
                        fail("V13", format!("phi {} operand {} has no type", id, i))
                    })?;
                    if let Some(ft) = first_ty {
                        if t != ft {
                            return Err(fail(
                                "V13",
                                format!(
                                    "phi {} mixes types: {} from '{}' but {} elsewhere",
                                    id,
                                    t.name(),
                                    f.region_name(joins[i]),
                                    ft.name()
                                ),
                            ));
                        }
                    }
                    // V07: value must be defined in the join region (or entry).
                    let op_region = f.cell(op).expect("V01 checked").region;
                    if op_region != joins[i] && op_region != entry {
                        return Err(fail(
                            "V07",
                            format!(
                                "phi {} takes {} from '{}' but it is defined in '{}'",
                                id,
                                op,
                                f.region_name(joins[i]),
                                f.region_name(op_region)
                            ),
                        ));
                    }
                }
            }
            CellKind::Jump { .. } => {}
            CellKind::Call { name, ret_ty } => {
                // V18: call operands must be value cells (arity and types
                // against the actual callee are checked at the PROGRAM
                // level — a lone fabric cannot resolve the name).
                for (slot, &op) in c.operands.iter().enumerate() {
                    let op_cell = f.cell(op).expect("V01 checked");
                    if !op_cell.produces_value() {
                        return Err(fail(
                            "V18",
                            format!("call {} ({}) operand {} is not a value cell: {}", id, name, slot, op),
                        ));
                    }
                }
                let _ = ret_ty;
            }
            CellKind::Ret => {
                if let Some(&op) = c.operands.first() {
                    let op_cell = f.cell(op).expect("V01 checked");
                    if !op_cell.produces_value() {
                        return Err(fail(
                            "V15",
                            format!("ret {} returns non-value cell {}", id, op),
                        ));
                    }
                }
            }
        }

        // V12: use-before-def for non-phi uses.
        if !matches!(c.kind, CellKind::Phi { .. }) {
            let my_pos = f
                .index_in_region(id)
                .ok_or_else(|| fail("V01", format!("cell {} not listed in its region", id)))?;
            for (slot, &op) in c.operands.iter().enumerate() {
                let def = f.cell(op).expect("V01 checked");
                if def.region == c.region {
                    let def_pos = f
                        .index_in_region(op)
                        .ok_or_else(|| fail("V01", format!("cell {} not listed in its region", op)))?;
                    if def_pos >= my_pos {
                        return Err(fail(
                            "V12",
                            format!(
                                "use-before-def: {} slot {} uses {} later in same region",
                                id, slot, op
                            ),
                        ));
                    }
                } else if def.region != entry {
                    return Err(fail(
                        "V12",
                        format!(
                            "{} uses {} across regions (must go through phi)",
                            id,
                            op
                        ),
                    ));
                }
            }
        }
    }

    // V18/V19: the tombstone laws (M4.1, tit-quilt retrofit — the
    // provenance-integrity law as verifier codes). A forget REMOVES:
    // a tombstone for a cell still in the slab is a forged FORGET of a
    // live-or-present cell. And forget is idempotent: two tombstones
    // for one cell is a forged graveyard.
    let mut tombed: std::collections::BTreeSet<CellId> = std::collections::BTreeSet::new();
    for tb in &f.tombstones {
        if f.cell(tb.cell).is_some() {
            return Err(fail(
                "V18",
                format!(
                    "tombstone {} exists but the cell is still present — a forged FORGET (forgetting removes; it never deletes)",
                    tb.cell
                ),
            ));
        }
        if !tombed.insert(tb.cell) {
            return Err(fail(
                "V19",
                format!("duplicate tombstone {} — forget is idempotent; one cell, one tombstone", tb.cell),
            ));
        }
    }

    Ok(())
}

fn check_operand_type(
    f: &Fabric,
    user: CellId,
    slot: usize,
    want: Type,
    code: Code,
) -> Result<(), VerifyError> {
    let c = f.cell(user).expect("present");
    let op = *c
        .operands
        .get(slot)
        .ok_or_else(|| fail("V01", format!("{} missing operand slot {}", user, slot)))?;
    let got = f
        .ty_of(op)
        .ok_or_else(|| fail(code, format!("{} operand {} produces no value", user, op)))?;
    if got != want {
        return Err(fail(
            code,
            format!(
                "{} slot {} wants {} but {} is {}",
                user,
                slot,
                want.name(),
                op,
                got.name()
            ),
        ));
    }
    Ok(())
}

/// Histogram of rejection codes over a set of (already run) results.
pub fn reason_histogram(errs: &[VerifyError]) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for e in errs {
        *m.entry(e.code.to_string()).or_insert(0) += 1;
    }
    m
}

/// Convenience used by tests and the fuzzer: is this cell a present,
/// value-producing cell?
pub fn is_value_cell(f: &Fabric, id: CellId) -> bool {
    matches!(f.cell(id), Some(c) if c.produces_value())
}

#[allow(dead_code)]
fn _unused(_c: &Cell) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{ArithOp, CmpOp};
    use crate::ty::ConstVal;

    /// entry: %0=const i32 1 ; %1=arith.add %0,%0 ; %2=ret %1
    fn good() -> Fabric {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let c = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let mut a = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a.operands = vec![c, c];
        let a = f.add_cell(e, a);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![a];
        f.add_cell(e, r);
        f
    }

    fn code_of(f: &Fabric) -> Code {
        verify(f).expect_err("test fabric must fail").code
    }

    /// Remove `region`'s terminator (leaving a None hole), then let `add`
    /// append the replacement cells (appends land at the terminator spot).
    fn swap_term(f: &mut Fabric, region: RegionId, add: impl FnOnce(&mut Fabric)) {
        let last = f.regions[region.0 as usize].cells.pop().expect("region has cells");
        f.slab[last.0 as usize] = None;
        add(f);
    }

    #[test]
    fn good_fabric_verifies() {
        assert!(verify(&good()).is_ok());
    }

    #[test]
    fn v00_no_regions() {
        let f = Fabric::empty();
        assert_eq!(code_of(&f), "V00");
    }

    #[test]
    fn v01_dangling_operand_and_hole() {
        let mut f = good();
        f.slab[0] = None; // punch hole under the add's operands
        assert_eq!(code_of(&f), "V01");
    }

    #[test]
    fn v01_dangling_region_list() {
        let mut f = good();
        f.regions[0].cells.push(CellId(99));
        assert_eq!(code_of(&f), "V01");
    }

    #[test]
    fn v02_jump_to_nonexistent_region() {
        let mut f = good();
        let e = f.entry().unwrap();
        swap_term(&mut f, e, |f| {
            f.add_cell(e, Cell::new(e, CellKind::Jump { target: RegionId(7) }));
        });
        assert_eq!(code_of(&f), "V02");
    }

    #[test]
    fn v03_no_terminator() {
        let mut f = good();
        let e = f.entry().unwrap();
        let last = f.regions[0].cells.pop().unwrap();
        f.slab[last.0 as usize] = None;
        let _ = e;
        assert_eq!(code_of(&f), "V03");
    }

    #[test]
    fn v04_terminator_not_last() {
        let mut f = good();
        let e = f.entry().unwrap();
        let j = Cell::new(e, CellKind::Jump { target: e });
        let pos = f.regions[0].cells.len() - 1;
        f.insert_cell(e, pos, j); // jump before ret
        assert_eq!(code_of(&f), "V04");
    }

    #[test]
    fn v05_phi_operand_count_mismatch() {
        let mut f = good();
        let e = f.entry().unwrap();
        swap_term(&mut f, e, |f| {
            let mut phi = Cell::new(e, CellKind::Phi { joins: vec![e] });
            phi.operands = vec![]; // 1 join, 0 operands
            f.add_cell(e, phi);
            f.add_cell(e, Cell::new(e, CellKind::Ret));
        });
        assert_eq!(code_of(&f), "V05");
    }

    #[test]
    fn v06_phi_join_is_not_a_pred() {
        let mut f = good();
        let e = f.entry().unwrap();
        let b = f.add_region("b");
        let c = f.add_region("c");
        f.add_cell(c, Cell::new(c, CellKind::Ret)); // c must be well-formed too
        swap_term(&mut f, e, |f| {
            f.add_cell(e, Cell::new(e, CellKind::Jump { target: b }));
        });
        // phi in b claims join from c, but c never branches to b
        let mut phi = Cell::new(b, CellKind::Phi { joins: vec![c] });
        phi.operands = vec![CellId(0)];
        f.add_cell(b, phi);
        f.add_cell(b, Cell::new(b, CellKind::Ret));
        assert_eq!(code_of(&f), "V06");
    }

    #[test]
    fn v07_phi_value_from_wrong_region() {
        let mut f = good();
        let e = f.entry().unwrap();
        let b = f.add_region("b");
        let c = f.add_region("c");
        swap_term(&mut f, e, |f| {
            f.add_cell(e, Cell::new(e, CellKind::Jump { target: b }));
        });
        let cv = f.add_cell(c, Cell::new(c, CellKind::Const { ty: Type::I32, val: ConstVal::I32(5) }));
        f.add_cell(c, Cell::new(c, CellKind::Ret));
        let mut phi = Cell::new(b, CellKind::Phi { joins: vec![e] });
        phi.operands = vec![cv]; // defined in c, joined from entry
        f.add_cell(b, phi);
        f.add_cell(b, Cell::new(b, CellKind::Ret));
        assert_eq!(code_of(&f), "V07");
    }

    #[test]
    fn v08_arith_type_mismatch() {
        let mut f = good();
        if let Some(c) = f.cell_mut(CellId(1)) {
            c.kind = CellKind::Arith { op: ArithOp::Add, ty: Type::I64 };
        }
        assert_eq!(code_of(&f), "V08");
    }

    #[test]
    fn v09_cmp_mixed_types() {
        let mut f = good();
        let e = f.entry().unwrap();
        swap_term(&mut f, e, |f| {
            let l = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I64, val: ConstVal::I64(1) }));
            let mut cmp = Cell::new(e, CellKind::Cmp { op: CmpOp::Lt });
            cmp.operands = vec![l, CellId(1)]; // i64 vs i32
            f.add_cell(e, cmp);
            f.add_cell(e, Cell::new(e, CellKind::Ret));
        });
        assert_eq!(code_of(&f), "V09");
    }

    #[test]
    fn v10_branch_cond_not_i1() {
        let mut f = good();
        let e = f.entry().unwrap();
        let b = f.add_region("b");
        swap_term(&mut f, e, |f| {
            let mut br = Cell::new(e, CellKind::Branch { then_r: b, else_r: b });
            br.operands = vec![CellId(0)]; // i32 const as cond
            f.add_cell(e, br);
        });
        f.add_cell(b, Cell::new(b, CellKind::Ret));
        assert_eq!(code_of(&f), "V10");
    }

    #[test]
    fn v11_const_value_type_mismatch() {
        let mut f = good();
        if let Some(c) = f.cell_mut(CellId(0)) {
            c.kind = CellKind::Const { ty: Type::I64, val: ConstVal::I32(1) };
        }
        assert_eq!(code_of(&f), "V11");
    }

    #[test]
    fn v12_use_before_def_same_region() {
        let mut f = good();
        // v1 note: the original fixture made %1 its own operand (a cycle);
        // V17 (operand-acyclicity) now fires first, so the fixture uses a
        // LATER cell instead — still exactly the use-before-def case V12
        // exists for, now without accidentally testing V17.
        let mut later = Cell::new(f.entry().unwrap(), CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        later.operands = vec![CellId(0), CellId(0)];
        let later_id = f.add_cell(f.entry().unwrap(), later);
        // splice it right AFTER %1 so %1 sees a later-defined cell
        let cells = &mut f.regions[0].cells;
        let later_pos = cells.iter().position(|&c| c == later_id).unwrap();
        cells.remove(later_pos); // take it off the end first
        let pos = cells.iter().position(|&c| c == CellId(1)).unwrap();
        cells.insert(pos + 1, later_id);
        if let Some(c) = f.cell_mut(CellId(1)) {
            c.operands = vec![later_id, CellId(0)];
        }
        assert_eq!(code_of(&f), "V12");
    }

    #[test]
    fn v12_cross_region_use_without_phi() {
        let mut f = good();
        let e = f.entry().unwrap();
        let b = f.add_region("b");
        let c = f.add_region("c");
        swap_term(&mut f, e, |f| {
            f.add_cell(e, Cell::new(e, CellKind::Jump { target: b }));
        });
        let cv = f.add_cell(c, Cell::new(c, CellKind::Const { ty: Type::I32, val: ConstVal::I32(9) }));
        f.add_cell(c, Cell::new(c, CellKind::Ret));
        let mut a2 = Cell::new(b, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a2.operands = vec![cv, cv];
        f.add_cell(b, a2);
        f.add_cell(b, Cell::new(b, CellKind::Ret));
        assert_eq!(code_of(&f), "V12");
    }

    #[test]
    fn v13_phi_mixed_types_two_preds() {
        let mut f = good();
        let e = f.entry().unwrap();
        let b = f.add_region("b");
        let c = f.add_region("c");
        let j = f.add_region("join");
        swap_term(&mut f, e, |f| {
            let bi1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }));
            let mut br = Cell::new(e, CellKind::Branch { then_r: b, else_r: c });
            br.operands = vec![bi1];
            f.add_cell(e, br);
        });
        let vb = f.add_cell(b, Cell::new(b, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        f.add_cell(b, Cell::new(b, CellKind::Jump { target: j }));
        let vc = f.add_cell(c, Cell::new(c, CellKind::Const { ty: Type::I64, val: ConstVal::I64(1) }));
        f.add_cell(c, Cell::new(c, CellKind::Jump { target: j }));
        let mut phi = Cell::new(j, CellKind::Phi { joins: vec![b, c] });
        phi.operands = vec![vb, vc];
        f.add_cell(j, phi);
        f.add_cell(j, Cell::new(j, CellKind::Ret));
        assert_eq!(code_of(&f), "V13");
    }

    #[test]
    fn v14_duplicate_phi_join() {
        let mut f = good();
        let e = f.entry().unwrap();
        let b = f.add_region("b");
        swap_term(&mut f, e, |f| {
            let bi1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I1, val: ConstVal::I1(false) }));
            let mut br = Cell::new(e, CellKind::Branch { then_r: b, else_r: b });
            br.operands = vec![bi1];
            f.add_cell(e, br);
        });
        let v1 = f.add_cell(b, Cell::new(b, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        let mut phi = Cell::new(b, CellKind::Phi { joins: vec![e, e] });
        phi.operands = vec![v1, v1];
        f.add_cell(b, phi);
        f.add_cell(b, Cell::new(b, CellKind::Ret));
        assert_eq!(code_of(&f), "V14");
    }

    #[test]
    fn v15_ret_of_non_value() {
        let mut f = good();
        let e = f.entry().unwrap();
        let b = f.add_region("b");
        // entry branches to b (twice); b's ret returns ENTRY'S BRANCH —
        // a cross-region use of an entry cell, legal for V12, nonsense
        // for V15
        let mut br_id_holder = CellId(0);
        swap_term(&mut f, e, |f| {
            let bi1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }));
            let mut br = Cell::new(e, CellKind::Branch { then_r: b, else_r: b });
            br.operands = vec![bi1];
            br_id_holder = f.add_cell(e, br);
        });
        let mut r = Cell::new(b, CellKind::Ret);
        r.operands = vec![br_id_holder];
        f.add_cell(b, r);
        assert_eq!(code_of(&f), "V15");
    }

    #[test]
    fn v12_param_outside_entry() {
        let mut f = good();
        let b = f.add_region("b");
        let p = Cell::new(b, CellKind::Param { ty: Type::I32 });
        f.add_cell(b, p);
        f.add_cell(b, Cell::new(b, CellKind::Ret));
        assert_eq!(code_of(&f), "V12");
    }
}

#[cfg(test)]
mod v16_tests {
    use super::*;
    use crate::cell::{Cell, CellKind};
    use crate::id::CellId;
    use crate::ty::ConstVal;

    fn code_of(f: &Fabric) -> Code {
        verify(f).expect_err("test fabric must fail").code
    }

    /// Two preds, phi joins only one of them: a control edge without a
    /// mux input — V16. (v1 rule; see EXPERIMENTS.md.)
    #[test]
    fn v16_phi_missing_a_predecessor_join() {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let t = f.add_region("t");
        let el = f.add_region("el");
        let j = f.add_region("j");
        let bi1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }));
        let mut br = Cell::new(e, CellKind::Branch { then_r: t, else_r: el });
        br.operands = vec![bi1];
        f.add_cell(e, br);
        let vt = f.add_cell(t, Cell::new(t, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        f.add_cell(t, Cell::new(t, CellKind::Jump { target: j }));
        f.add_cell(el, Cell::new(el, CellKind::Jump { target: j }));
        // phi joins ONLY 't' — 'el' is a real predecessor with no mux input
        let mut phi = Cell::new(j, CellKind::Phi { joins: vec![t] });
        phi.operands = vec![vt];
        let phi_id = f.add_cell(j, phi);
        let mut r = Cell::new(j, CellKind::Ret);
        r.operands = vec![phi_id];
        f.add_cell(j, r);
        assert_eq!(code_of(&f), "V16");
    }

    /// The same diamond with BOTH joins verifies: V16 accepts completeness.
    #[test]
    fn v16_complete_phi_verifies() {
        let text = "fabric v0\n\
region entry\n\
  %0 = param i32\n\
  %1 = const i1 true\n\
  %2 = br %1, t, el\n\
region t\n\
  %3 = const i32 1\n\
  %4 = jump j\n\
region el\n\
  %5 = const i32 2\n\
  %6 = jump j\n\
region j\n\
  %7 = phi [t: %3] [el: %5]\n\
  %8 = ret %7\n";
        let f = crate::text::parse(text).expect("diamond parses");
        assert!(verify(&f).is_ok());
    }

    /// A region with preds but NO phi at all is fine — V16 constrains
    /// phis, not regions.
    #[test]
    fn v16_region_without_phi_is_legal() {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let t = f.add_region("t");
        let el = f.add_region("el");
        let j = f.add_region("j");
        let bi1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I1, val: ConstVal::I1(true) }));
        let mut br = Cell::new(e, CellKind::Branch { then_r: t, else_r: el });
        br.operands = vec![bi1];
        f.add_cell(e, br);
        f.add_cell(t, Cell::new(t, CellKind::Jump { target: j }));
        f.add_cell(el, Cell::new(el, CellKind::Jump { target: j }));
        f.add_cell(j, Cell::new(j, CellKind::Ret));
        assert!(verify(&f).is_ok());
        let _ = CellId(0);
    }
}

#[cfg(test)]
mod v17_tests {
    use super::*;
    use crate::cell::{Cell, CellKind};
    use crate::id::CellId;
    use crate::ty::{ConstVal, Type};

    fn code_of(f: &Fabric) -> Code {
        verify(f).expect_err("test fabric must fail").code
    }

    /// Regression (corpus seed 1032071): a phi that joins ITSELF. v0's
    /// ty_of recursed forever on this; V17 must reject it first — and
    /// fast, not via stack overflow.
    #[test]
    fn v17_phi_self_cycle_is_rejected_not_a_hang() {
        let mut f = Fabric::empty();
        let r0 = f.add_region("r0");
        let r1 = f.add_region("r1");
        let v = f.add_cell(r0, Cell::new(r0, CellKind::Const { ty: Type::I32, val: ConstVal::I32(1) }));
        // %1 in r0: phi [r1: %1] — joins r1 (r1 jumps to r0), operand is
        // itself, defined in entry => passes V05/V06/V07/V16, only V17
        // catches the cycle
        let mut phi = Cell::new(r0, CellKind::Phi { joins: vec![r1] });
        phi.operands = vec![CellId(1)];
        let phi_id = f.add_cell(r0, phi);
        let _ = v;
        f.add_cell(r0, Cell::new(r0, CellKind::Jump { target: r1 }));
        f.add_cell(r1, Cell::new(r1, CellKind::Jump { target: r0 }));
        assert_eq!(code_of(&f), "V17");
        assert!(f.cell(phi_id).is_some());
    }

    /// Indirect cycle: entry phi A joins value from r1; that value uses
    /// entry phi B; B's join operand is that same r1 value. The loop
    /// passes through NON-phi cells — V17 must still catch it.
    #[test]
    fn v17_indirect_cycle_through_arith() {
        let mut f = Fabric::empty();
        let r0 = f.add_region("r0");
        let r1 = f.add_region("r1");
        // %0 = phi [r1: %3]   (entry phi, operand in r1)
        let mut a = Cell::new(r0, CellKind::Phi { joins: vec![r1] });
        a.operands = vec![CellId(3)];
        f.add_cell(r0, a);
        // %1 = phi [r1: %3]   (second entry phi, same operand)
        let mut b = Cell::new(r0, CellKind::Phi { joins: vec![r1] });
        b.operands = vec![CellId(3)];
        f.add_cell(r0, b);
        f.add_cell(r0, Cell::new(r0, CellKind::Jump { target: r1 }));
        // r1: %2 = arith.add %0(entry), %1(entry) ; %3 = phi [r0: %2]
        let mut ar = Cell::new(r1, CellKind::Arith { op: crate::cell::ArithOp::Add, ty: Type::I32 });
        ar.operands = vec![CellId(0), CellId(1)];
        f.add_cell(r1, ar);
        let mut c = Cell::new(r1, CellKind::Phi { joins: vec![r0] });
        c.operands = vec![CellId(2)];
        f.add_cell(r1, c);
        f.add_cell(r1, Cell::new(r1, CellKind::Jump { target: r0 }));
        assert_eq!(code_of(&f), "V17");
    }
}

#[cfg(test)]
mod tombstone_tests {
    //! V18/V19 — the graveyard laws (M4.1, tit-quilt retrofit).

    use super::*;
    use crate::cell::{ArithOp, Cell, CellKind};
    use crate::decay::{DeathCert, Tombstone};
    use crate::id::CellId;
    use crate::ty::{ConstVal, Type};

    /// entry: %0=param ; %1=const i64 7 (dead) ; %2=arith.add %0,%0 ; %3=ret %2
    fn with_dead() -> (Fabric, CellId) {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let dead = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I64, val: ConstVal::I64(7) }));
        let mut a = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
        a.operands = vec![p, p];
        let a = f.add_cell(e, a);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![a];
        f.add_cell(e, r);
        (f, dead)
    }

    #[test]
    fn red_v18_tombstone_for_a_present_cell_is_a_forged_forget() {
        let (mut f, dead) = with_dead();
        assert!(verify(&f).is_ok());
        // forge: tombstone the dead const WITHOUT removing it — the
        // graveyard claims a forget the fabric never performed
        let cert = DeathCert::measure(&f, dead, "dce-decay", 0).expect("present");
        f.tombstones.push(Tombstone {
            cell: dead,
            kind: "const".into(),
            killer: "dce-decay".into(),
            tick: 0,
            vhash: cert.vhash,
            witness: vec![],
        });
        let err = verify(&f).expect_err("a tombstone for a present cell must fail");
        assert_eq!(err.code, "V18");
        assert!(err.detail.contains("forged FORGET"), "{}", err);
        // the honest state — forget via the law path — verifies
        let mut g = f;
        g.tombstones.clear();
        g.forget(&cert).expect("lawful forget");
        assert!(verify(&g).is_ok());
        assert!(g.cell(dead).is_none());
    }

    #[test]
    fn red_v19_duplicate_tombstones_are_a_forged_graveyard() {
        let (f, dead) = with_dead();
        let cert = DeathCert::measure(&f, dead, "dce-decay", 0).expect("present");
        let mut g = f;
        g.forget(&cert).expect("lawful forget");
        assert!(verify(&g).is_ok());
        // forge: append a second tombstone for the same cell
        g.tombstones.push(cert.tombstone());
        let err = verify(&g).expect_err("two tombstones for one cell must fail");
        assert_eq!(err.code, "V19");
        assert!(err.detail.contains("idempotent"), "{}", err);
    }

    #[test]
    fn green_an_empty_graveyard_changes_nothing() {
        // every pre-M4.1 fabric (all corpus inputs, all v0 shapes)
        // carries an empty graveyard and must keep verifying
        let (f, _dead) = with_dead();
        assert_eq!(f.tombstones.len(), 0);
        assert!(verify(&f).is_ok());
    }
}

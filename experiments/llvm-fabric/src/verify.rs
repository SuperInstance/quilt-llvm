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
            CellKind::Jump { .. } | CellKind::Ret => {}
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
        if let Some(c) = f.cell_mut(CellId(1)) {
            c.operands = vec![CellId(1), CellId(0)];
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
    fn v12_param_outside_entry() {
        let mut f = good();
        let b = f.add_region("b");
        let p = Cell::new(b, CellKind::Param { ty: Type::I32 });
        f.add_cell(b, p);
        f.add_cell(b, Cell::new(b, CellKind::Ret));
        assert_eq!(code_of(&f), "V12");
    }
}

//! M4/M4.1 proof-of-rejection: forge a COMPLETE tombstone-bearing
//! death certificate for a LIVE cell — every field measured and
//! correct — and watch the verifier reject it anyway, naming the
//! demand chain. The tombstone body (tit-quilt retrofit) does not
//! launder a forged FORGET.
//!
//! Run: cargo run --release --example bogus_kill
//! Exit code 0 = the forged FORGET WAS rejected (expected); 1 = it
//! slipped through (would be an M4 failure).

use llvm_fabric::cell::{ArithOp, Cell, CellKind};
use llvm_fabric::decay::{dce_decay, verify_deaths, DeathCert};
use llvm_fabric::diff::Edit;
use llvm_fabric::fabric::Fabric;
use llvm_fabric::manager::TickCtx;
use llvm_fabric::ty::{ConstVal, Type};

fn main() {
    // %0 param ; %1 const 20 ; %2 add(%0,%1) ; %3 ret(%2) — %1 is LIVE
    let mut f = Fabric::empty();
    let e = f.add_region("entry");
    let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
    let c1 = f.add_cell(e, Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(20) }));
    let mut a = Cell::new(e, CellKind::Arith { op: ArithOp::Add, ty: Type::I32 });
    a.operands = vec![p, c1];
    let a = f.add_cell(e, a);
    let mut r = Cell::new(e, CellKind::Ret);
    r.operands = vec![a];
    f.add_cell(e, r);

    // run a real decay tick (fixed point — nothing dead), then FORGE a
    // complete tombstone for the live const %1: hash, kind, witness,
    // users — all measured, all correct. Only the death is a lie.
    let (_, mut rec) = dce_decay(&f, &TickCtx { tick: 0 }).expect("decay tick");
    let forged = DeathCert::measure(&f, c1, "dce-decay", 0).expect("live cell is measurable");
    rec.edits.push(Edit::RemoveCell {
        id: c1,
        ledger: forged.render(),
        summary: "%1 = const i32 20".into(),
    });

    println!("forged FORGET ledger entry: {}", rec.edits[0].match_ledger());
    match verify_deaths(&f, &rec) {
        Ok(()) => {
            println!("NOT REJECTED — M4 FAILURE: a forged FORGET passed as a certified death");
            std::process::exit(1);
        }
        Err(e) => {
            println!("verifier says: {}", e);
            println!("REJECTED (exit 0)");
        }
    }
}

trait MatchLedger {
    fn match_ledger(&self) -> String;
}
impl MatchLedger for Edit {
    fn match_ledger(&self) -> String {
        match self {
            Edit::RemoveCell { ledger, .. } => ledger.clone(),
            _ => String::new(),
        }
    }
}

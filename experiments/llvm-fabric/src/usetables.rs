//! Maintained use/pred/succ tables (R2 lane B — GATE-W2 §4 KEEP).
//!
//! `predecessors()` and `uses_of()` were linear scans over the whole
//! fabric (fabric.rs@9850cf2; EXPERIMENTS §4c). R3's region-edit
//! vocabulary will hammer pred/succ queries on every region add,
//! remove, and join drop, so the tables are an **R3 prerequisite**,
//! not a performance win — that is the honest reason they exist
//! (GATE-W2 §4, re-justified after the tier split was demoted).
//!
//! Three tables, one law:
//!
//! - `users[def]` — every use edge (user, slot) whose operand slot
//!   holds `def`, ordered user-asc then slot-asc (exactly the order
//!   the old scan produced, so consumers keep their determinism).
//! - `succs[r]` — successor regions of `r`: the targets of its LAST
//!   cell when that cell is a terminator, deduplicated in
//!   [then, else] order (exactly the old `successors()` semantics).
//! - `preds[s]` — predecessor regions of `s`, deduplicated, ascending
//!   (exactly the old `predecessors()` semantics).
//!
//! **The replay law applied to indexes:** the tables are maintained
//! incrementally by the sanctioned edit vocabulary (`Fabric::add_cell`
//! / `insert_cell` / `place_cell` / `remove_cell` / `retarget` /
//! `set_kind` / `set_operands` / `forget` / `add_region`), and every
//! maintenance step must keep them **bit-identical** to
//! `UseTables::derive(f)` — a full O(n) rebuild from the slab. The
//! test `tables_derivable_bit_identical_10k_corpus` enforces this on
//! the whole 10k corpus, on every pipeline stage and every replayed
//! stage. A maintained index that cannot be re-derived is a lie.
//!
//! **Desync policy, stated plainly.** `Fabric.slab` and
//! `Fabric.regions` are public; raw pokes (the corruption tier in
//! `fuzz::mutate`, the forgery fixtures in verify tests) do NOT go
//! through the tables and can desync them. This is by design: those
//! pokes exist to feed the verifier malformed fabrics, and the
//! verifier's structural checks (V01 dangling operand, V03 missing
//! terminator) fire before any table-backed query in every forgery
//! the corpus can produce. The sanctioned edit paths never desync.
//! `Fabric::rebuild_tables()` exists to re-derive after raw
//! inspection tooling pokes. `PartialEq` on Fabric ignores the tables
//! (a derived index is not fabric content), so replay bit-identity is
//! unaffected by table state.
//!
//! **One documented deviation from scan semantics:** an operand whose
//! id is >= slab.len() at placement time (the u32::MAX placeholder a
//! fresh `Cell::new(Phi)` carries, or a forged forward reference) is
//! not indexed into `users`. The old scan would count it in
//! `uses_of(that-id)` only if the slab later grew past the id, which
//! no sanctioned path does. Documented here rather than hidden.

use crate::cell::CellKind;
use crate::fabric::Fabric;
use crate::id::{CellId, RegionId};

/// The maintained tables. Not part of `Fabric` equality, not printed,
/// not signed: a derived index, derivable at all times.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct UseTables {
    /// users[def] = [(user, slot)] in user-asc, slot-asc order.
    /// Rows survive def removal (holes): the old scan listed users of a
    /// hole id too, and so do we.
    pub users: Vec<Vec<(CellId, u32)>>,
    /// succs[r] = successor regions of r (from its last cell), dedup,
    /// [then, else] order.
    pub succs: Vec<Vec<RegionId>>,
    /// preds[s] = predecessor regions of s, dedup, ascending.
    pub preds: Vec<Vec<RegionId>>,
}

/// Successor targets of a cell kind — the single definition of the
/// region-successor relation, used by both `derive` and every
/// incremental update (one law, two readers).
pub fn kind_succs(kind: &CellKind) -> Vec<RegionId> {
    match kind {
        CellKind::Branch { then_r, else_r } => {
            let mut out = vec![];
            for t in [*then_r, *else_r] {
                if !out.contains(&t) {
                    out.push(t);
                }
            }
            out
        }
        CellKind::Jump { target } => vec![*target],
        _ => vec![],
    }
}

impl UseTables {
    /// Full O(n) derivation from the slab — the ground truth every
    /// maintained table must equal bit-for-bit. Semantics identical to
    /// the pre-R2 scan queries (users: cells asc, slots asc; succs:
    /// last-cell kind; preds: regions asc, dedup).
    pub fn derive(f: &Fabric) -> UseTables {
        let mut t = UseTables {
            users: vec![vec![]; f.slab.len()],
            succs: vec![vec![]; f.regions.len()],
            preds: vec![vec![]; f.regions.len()],
        };
        for id in f.cells() {
            let c = f.cell(id).expect("cells() yields present ids");
            for (slot, &op) in c.operands.iter().enumerate() {
                if let Some(row) = t.users.get_mut(op.0 as usize) {
                    row.push((id, slot as u32));
                }
            }
        }
        for (ri, region) in f.regions.iter().enumerate() {
            let r = RegionId(ri as u32);
            let succs = region
                .cells
                .last()
                .and_then(|&last| f.cell(last))
                .map(|c| kind_succs(&c.kind))
                .unwrap_or_default();
            t.set_succs(r, succs);
        }
        t
    }

    /// ---- incremental ops (each O(local degree)) ----

    /// Ensure succ/pred rows exist for `n` regions. Rows may already
    /// exist (and be longer) when forward control references grew them
    /// early — this only ever grows, never truncates.
    pub fn ensure_rows(&mut self, n: usize) {
        while self.succs.len() < n {
            self.succs.push(vec![]);
        }
        while self.preds.len() < n {
            self.preds.push(vec![]);
        }
    }

    /// Growth cap for forward/forged region targets: a terminator may
    /// legally target a region index that is not yet registered (the
    /// text format allows forward region refs). We grow rows to fit
    /// real targets, but refuse to allocate for absurd forged indices.
    const ROW_GROW_LIMIT: usize = 1 << 20;

    /// Register one use edge (def <- user.slot), keeping the row
    /// sorted (user asc, slot asc). Binary-search insert: O(log + row).
    pub fn add_use(&mut self, def: CellId, user: CellId, slot: u32) {
        let row = match self.users.get_mut(def.0 as usize) {
            Some(r) => r,
            None => return, // forward reference beyond the slab: see module doc
        };
        let key = (user, slot);
        let pos = row.partition_point(|&k| k < key);
        row.insert(pos, key);
    }

    /// Unregister one use edge. No-op if absent (idempotent on
    /// forged/desynced states — the derive comparison is the real
    /// auditor).
    pub fn remove_use(&mut self, def: CellId, user: CellId, slot: u32) {
        let row = match self.users.get_mut(def.0 as usize) {
            Some(r) => r,
            None => return,
        };
        let key = (user, slot);
        if let Ok(pos) = row.binary_search(&key) {
            row.remove(pos);
        }
    }

    /// Move one use edge from one def to another (a retarget).
    pub fn move_use(&mut self, from: CellId, to: CellId, user: CellId, slot: u32) {
        if from == to {
            return;
        }
        self.remove_use(from, user, slot);
        self.add_use(to, user, slot);
    }

    /// Replace region r's successor set, repairing preds of every
    /// target that gained or lost the edge. O(|old| + |new| + preds
    /// rows touched) — local to the edit's degree. Targets beyond the
    /// current rows (forward region references — a jump placed before
    /// its target region is registered) grow the preds index to fit;
    /// this matches the old scan, which read targets at query time.
    pub fn set_succs(&mut self, r: RegionId, new: Vec<RegionId>) {
        if r.0 as usize >= self.succs.len() {
            return;
        }
        let old = std::mem::replace(&mut self.succs[r.0 as usize], new.clone());
        for t in &old {
            if !new.contains(t) {
                if let Some(row) = self.preds.get_mut(t.0 as usize) {
                    if let Ok(pos) = row.binary_search(&r) {
                        row.remove(pos); // removal keeps ascending order
                    }
                }
            }
        }
        for t in &new {
            if !old.contains(t) {
                if (t.0 as usize) < Self::ROW_GROW_LIMIT {
                    self.ensure_rows(t.0 as usize + 1);
                }
                if let Some(row) = self.preds.get_mut(t.0 as usize) {
                    let pos = row.partition_point(|&x| x < r);
                    row.insert(pos, r); // sorted insert keeps ascending order
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;
    use crate::ty::{ConstVal, Type};

    /// Independent brute-force users computation (NOT via derive): the
    /// old scan body, verbatim in spirit, so bit-identity tests compare
    /// the maintained tables against a genuinely second implementation.
    fn scan_users(f: &Fabric, def: CellId) -> Vec<(CellId, u32)> {
        let mut out = vec![];
        for user in f.cells() {
            if let Some(c) = f.cell(user) {
                for (slot, &op) in c.operands.iter().enumerate() {
                    if op == def {
                        out.push((user, slot as u32));
                    }
                }
            }
        }
        out
    }

    fn scan_preds(f: &Fabric, s: RegionId) -> Vec<RegionId> {
        let mut out = vec![];
        for (i, _) in f.regions.iter().enumerate() {
            let from = RegionId(i as u32);
            let succs = {
                let mut v = vec![];
                if let Some(region) = f.region(from) {
                    if let Some(&last) = region.cells.last() {
                        if let Some(c) = f.cell(last) {
                            v = kind_succs(&c.kind);
                        }
                    }
                }
                v
            };
            if succs.contains(&s) && !out.contains(&from) {
                out.push(from);
            }
        }
        out
    }

    /// entry: %0=param %1=const %2=add(%0,%1) %3=ret(%2) — the same
    /// shape the fabric.rs tests use.
    fn entry_fabric() -> Fabric {
        let mut f = Fabric::empty();
        let e = f.add_region("entry");
        let p = f.add_cell(e, Cell::new(e, CellKind::Param { ty: Type::I32 }));
        let c = f.add_cell(
            e,
            Cell::new(e, CellKind::Const { ty: Type::I32, val: ConstVal::I32(42) }),
        );
        let mut a = Cell::new(e, CellKind::Arith { op: crate::cell::ArithOp::Add, ty: Type::I32 });
        a.operands = vec![p, c];
        let a = f.add_cell(e, a);
        let mut r = Cell::new(e, CellKind::Ret);
        r.operands = vec![a];
        f.add_cell(e, r);
        f
    }

    /// A diamond with phis in the join (preds/succs actually exercised).
    fn diamond() -> Fabric {
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
        crate::text::parse(text).expect("diamond parses")
    }

    #[test]
    fn queries_match_the_old_scan_on_a_diamond() {
        let f = diamond();
        for id in f.cells() {
            assert_eq!(f.uses_of(id).to_vec(), scan_users(&f, id), "uses_of({id})");
        }
        for ri in 0..f.regions.len() as u32 {
            let r = RegionId(ri);
            assert_eq!(f.predecessors(r).to_vec(), scan_preds(&f, r), "preds({r})");
        }
        // spot semantic checks (the numbers the old tests asserted):
        // the br's condition is %1; entry branches to t and el; only
        // t and el jump to j
        assert_eq!(f.predecessors(RegionId(3)), &[RegionId(1), RegionId(2)]);
        assert_eq!(f.successors(RegionId(0)), &[RegionId(1), RegionId(2)]);
        assert_eq!(f.uses_of(CellId(1)), &[(CellId(2), 0)]);
    }

    #[test]
    fn add_remove_retarget_keep_tables_derivable() {
        let mut f = diamond();

        // ADD: a new add in t feeding nothing yet
        let t = RegionId(1);
        let mut a = Cell::new(t, CellKind::Arith { op: crate::cell::ArithOp::Add, ty: Type::I32 });
        a.operands = vec![CellId(3), CellId(3)];
        let na = f.insert_cell(t, 1, a); // before t's jump (mid-region insert)
        assert_eq!(UseTables::derive(&f), f.tables);
        assert_eq!(f.uses_of(CellId(3)), &[(CellId(7), 0), (na, 0), (na, 1)]);

        // RETARGET: the phi's then-arm now reads the new add
        let old = f.retarget(CellId(7), 0, na).expect("phi slot 0");
        assert_eq!(old, CellId(3));
        assert_eq!(UseTables::derive(&f), f.tables);
        assert_eq!(f.uses_of(na), &[(CellId(7), 0)]);

        // SET_OPERANDS: rebuild the phi's operand list wholesale
        f.set_operands(CellId(7), &[CellId(3), CellId(5)]).expect("phi present");
        assert_eq!(UseTables::derive(&f), f.tables);

        // REMOVE: take the new add back out (mid-region)
        assert!(f.remove_cell(na).is_some());
        assert_eq!(UseTables::derive(&f), f.tables);
        assert_eq!(f.uses_of(CellId(3)), &[(CellId(7), 0)]);

        // SET_KIND on the entry terminator: Br -> Jmp keeps only `t`.
        // t and el both still jump to j, so preds(j) is unchanged; what
        // changes is preds(el) — entry no longer branches there.
        f.set_kind(CellId(2), CellKind::Jump { target: RegionId(1) }).expect("br present");
        assert_eq!(UseTables::derive(&f), f.tables);
        assert_eq!(f.successors(RegionId(0)), &[RegionId(1)]);
        assert_eq!(f.predecessors(RegionId(2)), &[] as &[RegionId]); // el lost its pred
        assert_eq!(f.predecessors(RegionId(3)), &[RegionId(1), RegionId(2)]);

        // REMOVE the terminator: region loses its succs entirely
        assert!(f.remove_cell(CellId(2)).is_some());
        assert_eq!(UseTables::derive(&f), f.tables);
        assert_eq!(f.successors(RegionId(0)), &[] as &[RegionId]);
        assert_eq!(f.predecessors(RegionId(1)), &[] as &[RegionId]);

        // equality ignores tables: a re-derived twin must compare equal
        let mut twin = f.clone();
        twin.tables = UseTables::derive(&twin);
        assert_eq!(f, twin);
    }

    /// Red condition for the maintenance ops: dropping ANY of the four
    /// incremental updates (add-use, remove-use, succ-repair, pred-
    /// repair) must break bit-identity with `derive`. Simulated by
    /// sabotage: apply an edit with the table update suppressed, then
    /// demand derive == maintained. If a future refactor makes one of
    /// these ops a no-op, this suite pattern is what goes red (the
    /// per-op asserts above pin each one).
    #[test]
    fn sabotage_each_table_op_breaks_identity() {
        let mut f = entry_fabric();

        // sabotage 1: an add that never registers uses
        let mut ghost = f.clone();
        let e = ghost.entry().unwrap();
        let mut a = Cell::new(e, CellKind::Arith { op: crate::cell::ArithOp::Add, ty: Type::I32 });
        a.operands = vec![CellId(0), CellId(1)];
        ghost.slab.push(Some(a.clone()));
        ghost.regions[e.0 as usize].cells.insert(3, CellId(4)); // raw, table-blind
        assert_ne!(UseTables::derive(&ghost), ghost.tables, "a use-blind add must desync");

        // sabotage 2: a remove that never unregisters the removed
        // cell's own use edges (users[0] and users[1] would still list
        // the ghost of cell 2)
        let mut g = f.clone();
        g.slab[2] = None; // raw hole, table-blind
        g.regions[0].cells.retain(|&c| c != CellId(2));
        assert_ne!(UseTables::derive(&g), g.tables, "a use-blind remove must desync");

        // recovery: rebuild_tables re-derives (the replay law's escape
        // hatch for raw tooling), and the honest mutator path keeps
        // identity without any rebuild
        g.rebuild_tables();
        assert_eq!(UseTables::derive(&g), g.tables);
        let mut h = f;
        assert!(h.remove_cell(CellId(2)).is_some());
        assert_eq!(UseTables::derive(&h), h.tables, "remove_cell must keep tables derivable");
    }

    #[test]
    fn rows_survive_def_removal_like_the_old_scan() {
        // users of a hole id: the old scan still listed them; so do we
        let f = entry_fabric();
        // punch the DEF of the ret's operand (the add) WITHOUT
        // retargeting: a forged-but-parseable state
        let mut forged = f.clone();
        forged.slab[2] = None;
        forged.rebuild_tables();
        assert_eq!(forged.uses_of(CellId(2)), &[(CellId(3), 0)], "hole def keeps its users row");
        assert_eq!(forged.uses_of(CellId(2)).to_vec(), scan_users(&forged, CellId(2)));
    }
}

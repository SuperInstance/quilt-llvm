#!/usr/bin/env python3
"""R1 lane 3 (tier S2) — the code-level semantic sabotage battery.

Applies one-line, TYPE-CORRECT, semantically wrong source mutations
(the NEXT-PHASE §2 sabotage set, systematized) to the crate, runs the
full judge set after each, restores the file, and tallies kill rates:

  J1  cargo test           — the existing suite (123 tests at battery time)
  J2  cargo test + oracle  — suite with the property-oracle fixture
                             (scripts/sem_oracle_fixture.rs) appended
                             to src/passes/constfold.rs

A mutant is KILLED by a judge if the run fails (a failing test names
the corruption) or the tree stops compiling. Restore is `git checkout`
per mutant + a final clean-tree assertion.

House rules honored: exact-anchor replacement (count must be 1),
subprocess in list form (no shell strings), deterministic order,
results written to scripts/sem-mutants-results.tsv.
"""

import pathlib
import re
import subprocess
import sys

CRATE = pathlib.Path(__file__).resolve().parent.parent
TRACKED = [
    "src/passes/constfold.rs",
    "src/passes/inline.rs",
    "src/decay.rs",
]
FIXTURE = (CRATE / "scripts" / "sem_oracle_fixture.rs").read_text()
TSV = CRATE / "scripts" / "sem-mutants-results.tsv"

# ---------------------------------------------------------------------------
# The manifest. §2 = the nine sabotages from NEXT-PHASE §2 (baseline to
# reproduce: 5/7 fold corruptions survive J1). EXT = extensions.
# Every anchor must occur EXACTLY ONCE in its file.
# ---------------------------------------------------------------------------
CF = "src/passes/constfold.rs"
IN = "src/passes/inline.rs"
DC = "src/decay.rs"

MUTANTS = [
    # --- §2 baseline set (7 fold corruptions + inline + decay) ---
    dict(id="fold-add-i32-xy1", kind="fold-result-flip", file=CF, sec="S2",
         anchor="(ArithOp::Add, I32(x), I32(y)) => x.checked_add(y).map(I32),",
         repl="(ArithOp::Add, I32(x), I32(y)) => x.checked_mul(y).and_then(|v| v.checked_add(1)).map(I32),"),
    dict(id="fold-add-i64-xy1", kind="fold-result-flip", file=CF, sec="S2",
         anchor="(ArithOp::Add, I64(x), I64(y)) => x.checked_add(y).map(I64),",
         repl="(ArithOp::Add, I64(x), I64(y)) => x.checked_mul(y).and_then(|v| v.checked_add(1)).map(I64),"),
    dict(id="fold-sub-i32-swap", kind="operand-swap", file=CF, sec="S2",
         anchor="(ArithOp::Sub, I32(x), I32(y)) => x.checked_sub(y).map(I32),",
         repl="(ArithOp::Sub, I32(x), I32(y)) => y.checked_sub(x).map(I32),"),
    dict(id="fold-mul-i32-add", kind="fold-result-flip", file=CF, sec="S2",
         anchor="(ArithOp::Mul, I32(x), I32(y)) => x.checked_mul(y).map(I32),",
         repl="(ArithOp::Mul, I32(x), I32(y)) => x.checked_add(y).map(I32),"),
    dict(id="fold-div-i32-mul", kind="fold-result-flip", file=CF, sec="S2",
         anchor="(ArithOp::Div, I32(x), I32(y)) => x.checked_div(y).map(I32),",
         repl="(ArithOp::Div, I32(x), I32(y)) => x.checked_mul(y).map(I32),"),
    dict(id="fold-cmp-lt-i32-le", kind="fold-result-flip", file=CF, sec="S2",
         anchor="(CmpOp::Lt, I32(x), I32(y)) => x < y,",
         repl="(CmpOp::Lt, I32(x), I32(y)) => x <= y,"),
    dict(id="fold-cmp-ge-i64-gt", kind="fold-result-flip", file=CF, sec="S2",
         anchor="(CmpOp::Ge, I64(x), I64(y)) => x >= y,",
         repl="(CmpOp::Ge, I64(x), I64(y)) => x > y,"),
    dict(id="inline-args-reversed", kind="arg-binding-swap", file=IN, sec="S2",
         anchor="            map.insert(p, args[i]);",
         repl="            map.insert(p, args[params.len() - 1 - i]);"),
    dict(id="decay-liveness-skip-first", kind="liveness-skip", file=DC, sec="S2",
         anchor="        if let Some(c) = f.cell(id) {\n            for &op in &c.operands {",
         repl="        if let Some(c) = f.cell(id) {\n            for &op in c.operands.iter().skip(1) {"),
    # --- extensions: wider type coverage of the same families ---
    dict(id="fold-add-f64-xy1", kind="fold-result-flip", file=CF, sec="EXT",
         anchor="(ArithOp::Add, F64(x), F64(y)) => {\n            let r = x + y;",
         repl="(ArithOp::Add, F64(x), F64(y)) => {\n            let r = x * y + 1.0;"),
    dict(id="fold-sub-i64-swap", kind="operand-swap", file=CF, sec="EXT",
         anchor="(ArithOp::Sub, I64(x), I64(y)) => x.checked_sub(y).map(I64),",
         repl="(ArithOp::Sub, I64(x), I64(y)) => y.checked_sub(x).map(I64),"),
    dict(id="fold-mul-i64-add", kind="fold-result-flip", file=CF, sec="EXT",
         anchor="(ArithOp::Mul, I64(x), I64(y)) => x.checked_mul(y).map(I64),",
         repl="(ArithOp::Mul, I64(x), I64(y)) => x.checked_add(y).map(I64),"),
    dict(id="fold-div-i64-mul", kind="fold-result-flip", file=CF, sec="EXT",
         anchor="(ArithOp::Div, I64(x), I64(y)) => x.checked_div(y).map(I64),",
         repl="(ArithOp::Div, I64(x), I64(y)) => x.checked_mul(y).map(I64),"),
    dict(id="fold-div-f64-mul", kind="fold-result-flip", file=CF, sec="EXT",
         anchor="(ArithOp::Div, F64(x), F64(y)) => {\n            let r = x / y;",
         repl="(ArithOp::Div, F64(x), F64(y)) => {\n            let r = x * y;"),
    dict(id="fold-cmp-lt-f64-le", kind="fold-result-flip", file=CF, sec="EXT",
         anchor="(CmpOp::Lt, F64(x), F64(y)) => x < y,",
         repl="(CmpOp::Lt, F64(x), F64(y)) => x <= y,"),
    dict(id="fold-cmp-eq-i32-flip", kind="fold-result-flip", file=CF, sec="EXT",
         anchor="(CmpOp::Eq, I32(x), I32(y)) => x == y,",
         repl="(CmpOp::Eq, I32(x), I32(y)) => x != y,"),
    # --- extensions: the named kinds from the lane brief ---
    dict(id="fold-add-i32-offbyone", kind="off-by-one", file=CF, sec="EXT",
         anchor="(ArithOp::Add, I32(x), I32(y)) => x.checked_add(y).map(I32),",
         repl="(ArithOp::Add, I32(x), I32(y)) => x.checked_add(y).and_then(|v| v.checked_add(1)).map(I32),"),
    dict(id="fold-add-i64-offbyone", kind="off-by-one", file=CF, sec="EXT",
         anchor="(ArithOp::Add, I64(x), I64(y)) => x.checked_add(y).map(I64),",
         repl="(ArithOp::Add, I64(x), I64(y)) => x.checked_add(y).and_then(|v| v.checked_add(1)).map(I64),"),
    dict(id="fold-div-i32-offbyone", kind="off-by-one", file=CF, sec="EXT",
         anchor="(ArithOp::Div, I32(x), I32(y)) => x.checked_div(y).map(I32),",
         repl="(ArithOp::Div, I32(x), I32(y)) => x.checked_div(y).and_then(|v| v.checked_add(1)).map(I32),"),
    dict(id="inline-noncomm-swap", kind="noncomm-reorder", file=IN, sec="EXT",
         anchor=("            mapped.operands = cc\n"
                 "                .operands\n"
                 "                .iter()\n"
                 "                .map(|&op| *map.get(&op).expect(\"callee is verified: operands resolve\"))\n"
                 "                .collect();"),
         repl=("            mapped.operands = cc\n"
               "                .operands\n"
               "                .iter()\n"
               "                .map(|&op| *map.get(&op).expect(\"callee is verified: operands resolve\"))\n"
               "                .collect::<Vec<_>>();\n"
               "            if matches!(\n"
               "                &cc.kind,\n"
               "                CellKind::Arith { op: crate::cell::ArithOp::Sub | crate::cell::ArithOp::Div, .. }\n"
               "            ) {\n"
               "                mapped.operands.reverse();\n"
               "            }")),
]

# the §2 seven: fold-table corruptions only (baseline arithmetic)
S2_FOLD_IDS = {
    "fold-add-i32-xy1", "fold-add-i64-xy1", "fold-sub-i32-swap",
    "fold-mul-i32-add", "fold-div-i32-mul", "fold-cmp-lt-i32-le",
    "fold-cmp-ge-i64-gt",
}


def sh(args):
    return subprocess.run(args, cwd=CRATE, capture_output=True, text=True)


def cargo_test():
    p = sh(["cargo", "test", "-q"])
    out = p.stdout + p.stderr
    if p.returncode == 0:
        return ("SURVIVE", 0, [], out)
    # failing tests: "test result: FAILED. 5 passed; 116 failed; ..."
    fails = re.findall(r"test result: FAILED\. \d+ passed; (\d+) failed", out)
    if fails:
        names = re.findall(r"^failures:\n((?:    \S[^\n]*\n)+)", out, re.M)
        killed_by = [n.strip() for n in names[0].splitlines()] if names else []
        return ("KILL", sum(int(x) for x in fails), killed_by, out)
    if re.search(r"^error(\[|:)", out, re.M):
        return ("COMPILE-ERR", 0, [], out)
    # nonzero exit, format unrecognized — honest default: a kill
    return ("KILL", 0, [], out)


def apply_patch(mut):
    path = CRATE / mut["file"]
    src = path.read_text()
    n = src.count(mut["anchor"])
    if n != 1:
        raise SystemExit(f"anchor for {mut['id']} occurs {n}x in {mut['file']} (need 1) — aborting, tree restored")
    path.write_text(src.replace(mut["anchor"], mut["repl"], 1))


def restore():
    sh(["git", "checkout", "--"] + TRACKED)


def append_fixture():
    path = CRATE / "src/passes/constfold.rs"
    path.write_text(path.read_text() + "\n" + FIXTURE)


def main():
    # sanity: tree clean in the mutated paths, suite green
    st = sh(["git", "status", "--porcelain", "--"] + TRACKED)
    if st.stdout.strip():
        raise SystemExit(f"dirty tracked files before battery:\n{st.stdout}")
    base = cargo_test()
    if base[0] != "SURVIVE":
        raise SystemExit(f"baseline suite is not green: {base[0]}\n{base[3][-2000:]}")

    rows = []
    try:
        for mut in MUTANTS:
            restore()
            apply_patch(mut)
            j1, n1, names1, _ = cargo_test()
            append_fixture()
            j2, n2, names2, _ = cargo_test()
            restore()
            rows.append((mut, j1, n1, names1, j2, n2, names2))
            mark1 = "x" if j1 == "KILL" else " "
            mark2 = "x" if j2 == "KILL" else " "
            print(f"[{mark1}][{mark2}] {mut['id']:<28} suite={j1:<10}({n1} tests)  suite+oracle={j2:<10}({n2} tests)")
            for nm in names1[:3]:
                print(f"         killed by: {nm}")
    finally:
        restore()

    # final clean-tree assertion
    st = sh(["git", "status", "--porcelain", "--"] + TRACKED)
    if st.stdout.strip():
        raise SystemExit(f"BATTERY LEFT THE TREE DIRTY:\n{st.stdout}")

    # TSV artifact
    with TSV.open("w") as f:
        f.write("id\tkind\tsection\tfile\tjudge_suite\ttests_failed_suite\tkilled_by_suite\tjudge_suite_oracle\ttests_failed_oracle\toracle_marginal\n")
        for mut, j1, n1, names1, j2, n2, _ in rows:
            marginal = "yes" if (j2 == "KILL" and j1 != "KILL") else "no"
            f.write(f"{mut['id']}\t{mut['kind']}\t{mut['sec']}\t{mut['file']}\t{j1}\t{n1}\t{'|'.join(names1)}\t{j2}\t{n2}\t{marginal}\n")

    # summary
    total = len(rows)
    k1 = sum(1 for r in rows if r[1] == "KILL")
    k2 = sum(1 for r in rows if r[4] == "KILL")
    print()
    print(f"S2 tier: {total} code-level semantic mutants")
    print(f"  J1 suite kill rate:          {k1}/{total} ({100.0 * k1 / total:.1f}%)")
    print(f"  J2 suite+oracle kill rate:   {k2}/{total} ({100.0 * k2 / total:.1f}%)")
    print()
    fold = [r for r in rows if r[0]["id"] in S2_FOLD_IDS]
    fk1 = sum(1 for r in fold if r[1] == "KILL")
    fk2 = sum(1 for r in fold if r[4] == "KILL")
    print(f"NEXT-PHASE §2 baseline: {len(fold) - fk1}/{len(fold)} fold corruptions survive J1 (documented: 5/7)")
    print(f"                          {len(fold) - fk2}/{len(fold)} survive J2 (property oracle kills the residue)")
    print()
    kinds = {}
    for mut, j1, n1, names1, j2, n2, names2 in rows:
        a, b, c = kinds.setdefault(mut["kind"], [0, 0, 0])
        kinds[mut["kind"]] = [a + 1, b + (1 if j1 == "KILL" else 0), c + (1 if j2 == "KILL" else 0)]
    print(f"{'kind':<20} {'n':>3} {'J1':>5} {'J2':>5}")
    for kind, (n, a, b) in sorted(kinds.items()):
        print(f"{kind:<20} {n:>3} {a:>5} {b:>5}")
    print(f"\nresults: {TSV}")


if __name__ == "__main__":
    sys.exit(main())

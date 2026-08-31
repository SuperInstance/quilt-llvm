//! llvm-fabric CLI: print / verify / fuzz / pipeline / prov / replay / bench.

use llvm_fabric::fabric::Fabric;
use llvm_fabric::id::CellId;
use std::env;
use std::fs;
use std::process::exit;

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");
    match cmd {
        "version" => println!("llvm-fabric 0.1.0 (spike)"),
        "print" => cmd_print(&args[2..]),
        "verify" => cmd_verify(&args[2..]),
        "fuzz" => cmd_fuzz(&args[2..]),
        "pipeline" => cmd_pipeline(&args[2..]),
        "prov" => cmd_prov(&args[2..]),
        "replay" => cmd_replay(&args[2..]),
        "inline" => cmd_inline(&args[2..]),
        "bench" => {
            println!("{}", llvm_fabric::bench::bench());
        }
        _ => {
            eprintln!(
                "usage: llvm-fabric <version|print FILE|verify FILE|fuzz [--iters N] [--seed S]|pipeline FILE|prov FILE CELL|replay FILE|bench>"
            );
            exit(2);
        }
    }
}

fn load(path: &str) -> Fabric {
    let text = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("cannot read {}: {}", path, e);
        exit(1);
    });
    match llvm_fabric::text::parse(&text) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("parse error: {}", e);
            exit(1);
        }
    }
}

fn cmd_print(args: &[String]) {
    let f = load(args.first().expect("print needs FILE"));
    print!("{}", llvm_fabric::text::print(&f));
}

fn cmd_verify(args: &[String]) {
    let f = load(args.first().expect("verify needs FILE"));
    match llvm_fabric::verify::verify(&f) {
        Ok(()) => println!("OK"),
        Err(e) => {
            println!("FAIL {}", e);
            exit(1);
        }
    }
}

fn cmd_fuzz(args: &[String]) {
    let mut iters: u64 = 10_000;
    let mut seed: u64 = 0xFAB1C;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--iters" => {
                i += 1;
                iters = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| panic!("--iters needs a number"));
            }
            "--seed" => {
                i += 1;
                seed = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| panic!("--seed needs a number"));
            }
            other => panic!("unknown fuzz arg {}", other),
        }
        i += 1;
    }
    match llvm_fabric::fuzz::corpus_run(iters, seed) {
        Ok(st) => {
            println!("iters:                 {}", st.iters);
            println!("valid (by generator):  {}", st.valid);
            println!("phis generated:        {}", st.phis);
            println!("cells provenance-walked: {}", st.cells_walked);
            println!("roundtrip failures:    {}", st.roundtrip_fail);
            println!("prov failures:         {}", st.prov_fail);
            println!("ctrl-prov failures:    {}", st.ctrl_fail);
            println!("weft failures:         {}", st.weft_fail);
            println!("replay failures:       {}", st.replay_fail);
            println!(
                "panics:                {}  (a panic crashes this process; 0 here means none)",
                st.panics
            );
            println!("mutated:               {}", st.mutated);
            println!("  still valid:         {}", st.mutated_still_valid);
            println!("  rejected:            {} (by code)", st.rejected_total());
            for (code, n) in &st.rejected {
                println!("    {} {}", code, n);
            }
        }
        Err(e) => {
            eprintln!("corpus invariant violated: {}", e);
            exit(1);
        }
    }
}

fn cmd_pipeline(args: &[String]) {
    let f = load(args.first().expect("pipeline needs FILE"));
    if let Err(e) = llvm_fabric::verify::verify(&f) {
        eprintln!("input does not verify: {}", e);
        exit(1);
    }
    match llvm_fabric::pipeline::run(&f) {
        Ok((final_f, history, _)) => {
            println!("== history ==" );
            print!("{}", history.render(&final_f));
            println!("== weft (signature chain + progress law) ==");
            for t in &history.weft {
                println!(
                    "  tick {} {} sig={:016x} chain={:016x} :: {}",
                    t.epoch, t.pass, t.sig, t.chain, t.note
                );
            }
            if let Err(e) = history.check_weft() {
                println!("  WEFT LAW VIOLATED: {}", e);
                exit(1);
            }
            println!("== final fabric ==");
            print!("{}", llvm_fabric::text::print(&final_f));
            match llvm_fabric::verify::verify(&final_f) {
                Ok(()) => println!("final verifies: yes"),
                Err(e) => {
                    println!("final verifies: NO — {}", e);
                    exit(1);
                }
            }
            match llvm_fabric::conserve::check_pipeline(&f, &final_f, &history) {
                Ok(()) => println!("conservation: holds"),
                Err(e) => {
                    println!("conservation: VIOLATED — {}", e);
                    exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("pipeline failed: {}", e);
            exit(1);
        }
    }
}

fn cmd_inline(args: &[String]) {
    let path = args.first().expect("inline needs FILE (a fabric v1 program)");
    let text = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("cannot read {}: {}", path, e);
        exit(1);
    });
    let prog = match llvm_fabric::program::parse(&text) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("program parse error: {}", e);
            exit(1);
        }
    };
    if let Err(e) = llvm_fabric::program::verify_program(&prog) {
        eprintln!("program does not verify: {}", e);
        exit(1);
    }
    let main = prog.get("main").expect("program needs a 'main'").clone();
    let mut funcs = std::collections::BTreeMap::new();
    for name in &prog.order {
        if name != "main" {
            funcs.insert(name.clone(), prog.get(name).unwrap().clone());
        }
    }
    // remember what fed ret BEFORE, for the payoff shot
    let call_ids: Vec<llvm_fabric::id::CellId> = main
        .cells()
        .filter(|&id| matches!(main.cell(id).map(|c| &c.kind), Some(llvm_fabric::cell::CellKind::Call { .. })))
        .collect();
    match llvm_fabric::pipeline::run_v1(&main, &funcs) {
        Ok((final_f, history, _)) => {
            println!("== history ==");
            print!("{}", history.render(&final_f));
            println!("== weft (signature chain + progress law) ==");
            for t in &history.weft {
                println!(
                    "  tick {} {} sig={:016x} chain={:016x} :: {}",
                    t.epoch, t.pass, t.sig, t.chain, t.note
                );
            }
            println!("== final main ==");
            print!("{}", llvm_fabric::text::print(&final_f));
            match llvm_fabric::verify::verify(&final_f) {
                Ok(()) => println!("final verifies: yes"),
                Err(e) => {
                    println!("final verifies: NO — {}", e);
                    exit(1);
                }
            }
            match llvm_fabric::conserve::check_pipeline(&main, &final_f, &history) {
                Ok(()) => println!("conservation: holds"),
                Err(e) => {
                    println!("conservation: VIOLATED — {}", e);
                    exit(1);
                }
            }
            // payoff shot: the value that WAS each call result, walked
            // back through the graft into caller values
            for cid in call_ids {
                println!("== full provenance of the former call result {} (through the graft) ==", cid);
                let ret_id = final_f
                    .cells()
                    .find(|&id| matches!(final_f.cell(id).map(|c| &c.kind), Some(llvm_fabric::cell::CellKind::Ret)))
                    .unwrap();
                let _ = ret_id;
                let story = llvm_fabric::prov::prov_history(&history, cid);
                for (epoch, pass, what) in story {
                    println!("  tick {} {}: {}", epoch, pass, what);
                }
            }
            let ret_id = final_f
                .cells()
                .find(|&id| matches!(final_f.cell(id).map(|c| &c.kind), Some(llvm_fabric::cell::CellKind::Ret)))
                .unwrap();
            if let Some(&fed) = final_f.cell(ret_id).unwrap().operands.first() {
                println!("== provenance(ret) after inline ==");
                match llvm_fabric::prov::provenance(&final_f, fed) {
                    Ok(node) => print!("{}", llvm_fabric::prov::render(&node)),
                    Err(e) => println!("  prov failed: {}", e),
                }
            }
        }
        Err(e) => {
            eprintln!("pipeline failed: {}", e);
            exit(1);
        }
    }
}

fn cmd_prov(args: &[String]) {
    let f = load(args.first().expect("prov needs FILE"));
    let cell: u32 = args
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("prov needs CELL (a number, e.g. 9)"));
    match llvm_fabric::prov::provenance(&f, CellId(cell)) {
        Ok(node) => print!("{}", llvm_fabric::prov::render(&node)),
        Err(e) => {
            eprintln!("provenance failed: {}", e);
            exit(1);
        }
    }
}

fn cmd_replay(args: &[String]) {
    let f = load(args.first().expect("replay needs FILE"));
    let (final_f, history, stages) = llvm_fabric::pipeline::run(&f).expect("pipeline");
    let (replayed, final_r) = llvm_fabric::replay::replay(&f, &history).expect("replay");
    let mut ok = true;
    for (i, (a, b)) in stages.iter().zip(replayed.iter()).enumerate() {
        if a != b || llvm_fabric::text::print(a) != llvm_fabric::text::print(b) {
            println!("stage {}: DIVERGED", i);
            ok = false;
        }
    }
    if final_f != final_r {
        println!("final: DIVERGED");
        ok = false;
    }
    if ok {
        println!(
            "replay: {} stages reproduced bit-identically (structural + canonical text)",
            replayed.len()
        );
    } else {
        exit(1);
    }
}

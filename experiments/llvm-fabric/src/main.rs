//! llvm-fabric CLI.

use std::env;
use std::process::exit;

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");
    match cmd {
        "version" => println!("llvm-fabric 0.1.0 (spike)"),
        "fuzz" => cmd_fuzz(&args[2..]),
        _ => {
            eprintln!("usage: llvm-fabric <version|fuzz [--iters N] [--seed S]>");
            exit(2);
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
            println!("roundtrip failures:    {}", st.roundtrip_fail);
            println!("panics:                {}  (a panic crashes this process; 0 here means none)", st.panics);
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

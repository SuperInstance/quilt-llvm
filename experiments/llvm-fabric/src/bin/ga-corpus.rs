//! GA-CORPUS spike runner — see docs/phase/GA-CORPUS-SPIKE.md.
//!
//! cargo run --release --bin ga-corpus [--pop 200 --gens 50 --seed N]
//!
//! Prints per-C-item measured exits: first generation covered, max
//! population count over the run, final-population count, final-pop
//! verify pass rate, plus a per-generation trace of the best fabric's
//! coverage (as a stable bitstring, C1..C11, `.` = uncovered).

use llvm_fabric::fuzz::Rng;
use llvm_fabric::ga::{self, C_ITEMS, GaConfig};

fn main() {
    let mut cfg = GaConfig::default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0usize;
    while i < args.len() {
        let val = |i: &mut usize| -> u64 {
            *i += 1;
            args.get(*i).expect("flag needs a value").parse().expect("numeric")
        };
        match args[i].as_str() {
            "--pop" => cfg.population = val(&mut i) as usize,
            "--gens" => cfg.generations = val(&mut i) as usize,
            "--elite" => cfg.elite = val(&mut i) as usize,
            "--seed" => cfg.seed = val(&mut i),
            other => panic!("unknown flag {}", other),
        }
        i += 1;
    }
    eprintln!(
        "ga-corpus: pop {} x {} gens, elite {}, seed {:#x} (mud-arena engine structure)",
        cfg.population, cfg.generations, cfg.elite, cfg.seed
    );

    let t0 = std::time::Instant::now();
    let rep = ga::run(&cfg);
    let dt = t0.elapsed();

    // per-generation coverage trace of population item-counts
    println!("gen  avg      best   verify  C1..C11 population counts");
    for g in &rep.gens {
        let counts: String = g
            .item_counts
            .iter()
            .map(|c| format!("{:4}", c))
            .collect::<Vec<_>>()
            .join("");
        println!(
            "{:3}  {:7.3}  {:6.1}  {:5}   {}",
            g.gen, g.avg_fitness, g.best_fitness, g.verify_pass, counts
        );
    }

    println!("\n== measured exit: per-C-item, best-of-run ==");
    println!("item  construct                              first-gen  max-in-pop");
    let names = [
        "call cells",
        "non-i32 phis",
        ">1 phi/region",
        "phi at spine head",
        "partial phis (V16)",
        "phi feeds computation",
        "params outside entry (V12)",
        "nested regions (no IR)",
        "boundary consts",
        "size caps exceeded",
        "non-latest operand head",
    ];
    for i in 0..11 {
        let first = if rep.first_covered[i] == usize::MAX {
            "never".to_string()
        } else {
            rep.first_covered[i].to_string()
        };
        println!("{:4}  {:38}  {:>9}  {:11}", C_ITEMS[i], names[i], first, rep.max_item_counts[i]);
    }

    let last = rep.gens.last().expect("at least one gen");
    println!("\nfinal population: {}/{} verify green ({:.1}%)", last.verify_pass, cfg.population,
        100.0 * last.verify_pass as f64 / cfg.population as f64);
    println!("best coverage: {} items, fitness {:.1}", rep.best_coverage.n_items(), rep.best_fitness);
    println!("total evaluations: {}  wall time: {:.2}s", rep.total_evals, dt.as_secs_f64());

    // honesty check: breed a fresh population and count text round-trips
    // (the corpus harness demands print/parse/print stability; bred
    // fabrics are not corpus fabrics until they round-trip too)
    let mut rng = Rng::new(cfg.seed ^ 0x5EED);
    let mut rt_ok = 0usize;
    let mut rt_total = 0usize;
    for i in 0..cfg.population {
        let mut f = llvm_fabric::fuzz::gen_fabric(&mut Rng::new(cfg.seed.wrapping_add(i as u64 + 1).max(1)));
        for _ in 0..(1 + rng.below(6)) {
            f = ga::mutate_breed(&f, &mut rng);
        }
        if llvm_fabric::verify::verify(&f).is_ok() {
            rt_total += 1;
            let once = llvm_fabric::text::print(&f);
            if let Ok(f2) = llvm_fabric::text::parse(&once) {
                if llvm_fabric::text::print(&f2) == once {
                    rt_ok += 1;
                }
            }
        }
    }
    println!(
        "mutated-only sanity: {}/{} verifying, {}/{} of those round-trip text",
        rt_total, cfg.population, rt_ok, rt_total.max(1)
    );
    let _ = &mut cfg;
}

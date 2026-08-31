//! The experiment driver: build battens from verified training outcomes,
//! route held-out fabrics, compare against the oracle.

use crate::features::{raw_features, Standardizer};
use crate::kernel::Spline;
use crate::measure::{run_pipeline, Outcome, LAMBDA, PIPELINES};
use llvm_fabric::fabric::Fabric;
use llvm_fabric::fuzz::{gen_fabric, Rng};

const TRAIN_SEED0: u64 = 1;
const TRAIN_N: usize = 800;
const TEST_SEED0: u64 = 100_000;
const TEST_N: usize = 200;

struct Fab {
    fabric: Fabric,
    feats: Vec<f64>, // standardized
}

fn corpus(seed0: u64, n: usize, st: &Standardizer) -> Vec<Fab> {
    (0..n)
        .map(|i| {
            let fabric = gen_fabric(&mut Rng::new(seed0 + i as u64));
            let feats = st.transform(&raw_features(&fabric));
            Fab { fabric, feats }
        })
        .collect()
}

struct Routed {
    pipeline: String,
    outcome: Outcome,
}

struct SplineSet {
    util: Vec<Spline>, // per pipeline
    cost: Vec<Spline>, // per pipeline
}

impl SplineSet {
    fn fit(train: &[Fab], fog_scale: f64) -> SplineSet {
        let mut set = SplineSet { util: Vec::new(), cost: Vec::new() };
        for p in PIPELINES {
            let mut su = Spline::new(fog_scale);
            let mut sc = Spline::new(fog_scale);
            for t in train {
                let (_, o) = run_pipeline(&t.fabric, p).expect("train pipeline run");
                su.add(t.feats.clone(), o.utility());
                sc.add(t.feats.clone(), o.rel_cost());
            }
            set.util.push(su);
            set.cost.push(sc);
        }
        set
    }

    fn route(&self, x: &[f64]) -> (usize, f64, f64) {
        // (pipeline idx, est score, fog at that pipeline's battens)
        let mut best = (0usize, f64::NEG_INFINITY, f64::INFINITY);
        for i in 0..PIPELINES.len() {
            let est = self.util[i].estimate(x) - LAMBDA * self.cost[i].estimate(x);
            if est > best.1 {
                best = (i, est, self.util[i].fog(x));
            }
        }
        best
    }
}

fn oracle(f: &Fabric) -> Routed {
    let mut best: Option<(usize, Outcome)> = None;
    for (i, p) in PIPELINES.iter().enumerate() {
        let (_, o) = run_pipeline(f, p).expect("oracle pipeline run");
        // argmax score, tie-break cheaper cost
        let better = match best {
            None => true,
            Some((_, bo)) => {
                o.score() > bo.score() + 1e-12
                    || ((o.score() - bo.score()).abs() <= 1e-12 && o.cost < bo.cost)
            }
        };
        if better {
            best = Some((i, o));
        }
    }
    let (i, o) = best.unwrap();
    Routed { pipeline: PIPELINES[i].to_string(), outcome: o }
}

pub fn experiment() {
    println!("batten-spike: verified-outcome routing over a toy pass pipeline");
    println!("pipelines: {:?}", PIPELINES);
    println!("lambda (utility/cost tradeoff): {}", LAMBDA);

    // ---- corpus (features standardized on train stats only) ----
    let raw_train: Vec<Vec<f64>> = (0..TRAIN_N)
        .map(|i| {
            let f = gen_fabric(&mut Rng::new(TRAIN_SEED0 + i as u64));
            raw_features(&f)
        })
        .collect();
    let st = Standardizer::fit(&raw_train);
    let train = corpus(TRAIN_SEED0, TRAIN_N, &st);
    let test = corpus(TEST_SEED0, TEST_N, &st);

    // ---- fog-scale sweep on training self-consistency, then final run ----
    for fog_scale in [0.25, 0.5, 1.0, 2.0] {
        let set = SplineSet::fit(&train, fog_scale);
        run_and_report(&set, &train, &test, fog_scale);
    }
}

fn run_and_report(set: &SplineSet, train: &[Fab], test: &[Fab], fog_scale: f64) {
    let (mut match_n, mut regret, mut majority_hits) = (0usize, 0.0f64, 0usize);
    let (mut routed_cost, mut oracle_cost, mut full_cost) = (0usize, 0usize, 0usize);
    let mut routed_util = 0.0f64;
    let mut oracle_util = 0.0f64;
    // failure analysis
    let (mut fog_ok_sum, mut fog_ok_n, mut fog_miss_sum, mut fog_miss_n) =
        (0.0f64, 0usize, 0.0f64, 0usize);
    let mut miss_hist: std::collections::BTreeMap<(String, String), usize> = Default::default();
    let mut misses: Vec<(f64, f64)> = Vec::new(); // (fog, score gap)

    let mut oracle_picks: std::collections::BTreeMap<String, usize> = Default::default();
    // majority pipeline measured on a train-time oracle pass (trivial baseline)
    let mut train_picks: std::collections::BTreeMap<String, usize> = Default::default();
    for t in train {
        let o = oracle(&t.fabric);
        *train_picks.entry(o.pipeline).or_insert(0) += 1;
    }
    let majority = train_picks.iter().max_by_key(|(_, c)| **c).map(|(k, _)| k.clone()).unwrap();

    for t in test {
        let (ri, _est, fog) = set.route(&t.feats);
        let ro = {
            let (_, o) = run_pipeline(&t.fabric, PIPELINES[ri]).unwrap();
            o
        };
        let orc = oracle(&t.fabric);
        *oracle_picks.entry(orc.pipeline.clone()).or_insert(0) += 1;
        if orc.pipeline == majority {
            majority_hits += 1;
        }
        let (_, full_o) = run_pipeline(&t.fabric, "full").unwrap();

        if PIPELINES[ri] == orc.pipeline {
            match_n += 1;
            fog_ok_sum += fog;
            fog_ok_n += 1;
        } else {
            regret += orc.outcome.score() - ro.score();
            fog_miss_sum += fog;
            fog_miss_n += 1;
            *miss_hist
                .entry((PIPELINES[ri].to_string(), orc.pipeline.clone()))
                .or_insert(0) += 1;
            misses.push((fog, orc.outcome.score() - ro.score()));
        }
        routed_cost += ro.cost;
        oracle_cost += orc.outcome.cost;
        full_cost += full_o.cost;
        routed_util += ro.utility();
        oracle_util += orc.outcome.utility();
    }

    let n = test.len() as f64;
    println!("\n=== fog_scale = {:.2} ===", fog_scale);
    println!(
        "train battens: {} fabrics x {} pipelines = {} verified outcomes",
        train.len(),
        PIPELINES.len(),
        train.len() * PIPELINES.len()
    );
    println!("test fabrics: {}", test.len());
    println!(
        "routing accuracy vs oracle: {}/{} = {:.1}%",
        match_n,
        test.len(),
        100.0 * match_n as f64 / n
    );
    println!(
        "mean score regret (oracle - routed): {:.5}",
        regret / n
    );
    println!(
        "cost: routed {} cells vs full {} cells -> {:.1}% saved (vs always-full)",
        routed_cost,
        full_cost,
        100.0 * (1.0 - routed_cost as f64 / full_cost as f64)
    );
    println!(
        "cost vs oracle: routed {}, oracle-cheapest {} ({:.1}% overhead)",
        routed_cost,
        oracle_cost,
        100.0 * (routed_cost as f64 / oracle_cost as f64 - 1.0)
    );
    println!(
        "utility: routed mean {:.4} vs oracle mean {:.4}",
        routed_util / n,
        oracle_util / n
    );
    println!(
        "fog at correct routes: mean {:.3} (n={}) | fog at misroutes: mean {:.3} (n={})",
        if fog_ok_n > 0 { fog_ok_sum / fog_ok_n as f64 } else { f64::NAN },
        fog_ok_n,
        if fog_miss_n > 0 { fog_miss_sum / fog_miss_n as f64 } else { f64::NAN },
        fog_miss_n
    );
    if !miss_hist.is_empty() {
        println!("misroute histogram (routed -> oracle):");
        let mut v: Vec<_> = miss_hist.iter().collect();
        v.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
        for ((a, b), c) in v {
            println!("  {} -> {} : {}", a, b, c);
        }
    }
    println!(
        "oracle pick distribution: {:?}",
        oracle_picks
    );
    println!(
        "trivial baseline (always {:?}, train majority): {}/{} = {:.1}%",
        majority,
        majority_hits,
        test.len(),
        100.0 * majority_hits as f64 / n
    );
    let _ = &mut misses; // retained for future per-case dumps
}

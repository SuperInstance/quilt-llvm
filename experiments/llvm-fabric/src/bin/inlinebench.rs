fn main() {
    use std::collections::BTreeMap;
    // a caller with N call sites to a callee with B body cells:
    // measure orig/final/history bytes across the v1 pipeline
    for (nsites, bbody) in [(1usize, 1usize), (4, 4), (8, 8), (16, 16)] {
        let mut main_text = String::from("fabric v0\nregion entry\n  %0 = param i32\n  %999 = const i32 3\n");
        let mut id = 1usize;
        let mut prev = "%0".to_string();
        for s in 0..nsites {
            let c = if s % 2 == 0 { "%999".to_string() } else { "%0".to_string() };
            main_text.push_str(&format!("  %{} = call i32 add2 {}, {}\n", id, prev, c));
            prev = format!("%{}", id);
            id += 1;
        }
        main_text.push_str(&format!("  %{} = ret {}\n", id, prev));
        let mut callee = String::from("fabric v0\nregion entry\n  %0 = param i32\n  %1 = param i32\n");
        let mut cid = 2usize;
        let mut cprev = "%0".to_string();
        for _ in 0..bbody {
            callee.push_str(&format!("  %{} = arith.add i32 {}, %1\n", cid, cprev));
            cprev = format!("%{}", cid);
            cid += 1;
        }
        callee.push_str(&format!("  %{} = ret {}\n", cid, cprev));
        let mut funcs = BTreeMap::new();
        funcs.insert("add2".to_string(), llvm_fabric::text::parse(&callee).unwrap());
        let f = llvm_fabric::text::parse(&main_text).unwrap();
        let (fin, h, stages) = llvm_fabric::pipeline::run_v1(&f, &funcs).unwrap();
        let ob = llvm_fabric::text::print(&f).len();
        let fb = llvm_fabric::text::print(&fin).len();
        let hb = h.render(&fin).len();
        println!(
            "callsites={} calleebody={} cells {}->{} orig-B {} final-B {} history-B {} hist/final {:.1}",
            nsites,
            bbody,
            f.cells().count(),
            fin.cells().count(),
            ob,
            fb,
            hb,
            hb as f64 / fb.max(1) as f64
        );
        let _ = stages;
        // sanity: still verifies + conserves + weft holds
        llvm_fabric::verify::verify(&fin).unwrap();
        llvm_fabric::conserve::check_pipeline(&f, &fin, &h).unwrap();
        h.check_weft().unwrap();
    }
}

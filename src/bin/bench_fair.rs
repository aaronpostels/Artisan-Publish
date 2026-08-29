use artisan::bench::{self, harness::BenchCfg};
use std::io::Write;

#[derive(serde::Serialize)]
struct FairRun {
    framework: &'static str,
    schema_version: u32,
    suite: &'static str,

    threading: &'static str,
    env: bench::BenchEnv,
    cfg: BenchCfg,
    results: Vec<bench::fair::FairResult>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg = BenchCfg { warmup: 3, reps: 15, parallel: false, work_scale: 1.0 };
    let mut out_path = "results/fair_artisan.json".to_string();
    let mut only: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--reps" => { i += 1; cfg.reps = args[i].parse().expect("--reps expects a number"); }
            "--warmup" => { i += 1; cfg.warmup = args[i].parse().expect("--warmup expects a number"); }
            "--scale" => { i += 1; cfg.work_scale = args[i].parse().expect("--scale expects a number"); }
            "--only" => { i += 1; only = Some(args[i].clone()); }
            "--out" => { i += 1; out_path = args[i].clone(); }
            "--list" => {
                for c in bench::fair::fair_cases() {
                    println!("{:>3}  {:<16} {}", c.spec.id, c.spec.group, c.spec.label);
                }
                return;
            }
            other => { eprintln!("unknown argument: {other}"); std::process::exit(2); }
        }
        i += 1;
    }

    let env = bench::BenchEnv::capture();
    println!("\n=== ARTISAN — NEUTRAL ECS SUITE (schema v3, single-threaded) ===");
    println!("  cpu         : {}", env.cpu_brand);
    println!("  os          : {}", env.os_description);
    println!("  rustc       : {}", env.rustc_version);
    println!("  commit      : {}{}", env.git_commit, if env.git_dirty { " (dirty)" } else { "" });
    println!("  warmup/reps : {}/{}", cfg.warmup, cfg.reps);
    println!();

    let mut group = String::new();
    let results = bench::fair::run_fair_suite(&cfg, only.as_deref(), |r| {
        if r.spec.group != group {
            group = r.spec.group.to_string();
            println!("  {}", group.to_uppercase());
        }
        println!(
            "    {:<32} {:>10.3} ms   ci95 [{:>8.3}, {:>8.3}]   rsd {:>5.1} %{}",
            r.spec.label,
            r.stats.median,
            r.stats.ci95_median[0],
            r.stats.ci95_median[1],
            r.stats.rsd * 100.0,
            if r.checksum_stable { "" } else { "   [!] UNSTABLE" }
        );
    });

    let run = FairRun {
        framework: "artisan",
        schema_version: 3,
        suite: "neutral-ecs",
        threading: "single",
        env,
        cfg,
        results,
    };

    if let Some(dir) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::File::create(&out_path)
        .expect("creating result file")
        .write_all(serde_json::to_string_pretty(&run).unwrap().as_bytes())
        .expect("writing result file");
    println!("\nWrote {out_path}");
}

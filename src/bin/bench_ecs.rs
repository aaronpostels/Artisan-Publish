use artisan::bench::{self, BenchCfg};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut cfg = BenchCfg::default();
    let mut ids: Vec<u32> = Vec::new();
    let mut out_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--serial" => cfg.parallel = false,
            "--parallel" => cfg.parallel = true,
            "--reps" => {
                i += 1;
                cfg.reps = args[i].parse().expect("--reps expects a number");
            }
            "--warmup" => {
                i += 1;
                cfg.warmup = args[i].parse().expect("--warmup expects a number");
            }
            "--scale" => {
                i += 1;
                cfg.work_scale = args[i].parse().expect("--scale expects a number");
            }
            "--out" => {
                i += 1;
                out_path = Some(args[i].clone());
            }
            "--list" => {
                for s in bench::manifest() {
                    println!(
                        "{:>3}  {:<28} n={:<9} iters={:<4} {:?}/{:?}",
                        s.id, s.name, s.entity_count, s.inner_iters, s.group, s.subquestion
                    );
                }
                return;
            }
            other => match other.parse::<u32>() {
                Ok(id) => ids.push(id),
                Err(_) => {
                    eprintln!("unknown argument: {other}");
                    std::process::exit(2);
                }
            },
        }
        i += 1;
    }

    let mode = if cfg.parallel { "parallel" } else { "serial" };
    let out = out_path.unwrap_or_else(|| format!("results/artisan_ecs_{mode}.json"));

    let env = bench::BenchEnv::capture();
    println!("\n=== ARTISAN ECS BENCHMARK (schema v2) ===");
    println!("  cpu           : {}", env.cpu_brand);
    println!("  os            : {}", env.os_description);
    println!("  rustc         : {}", env.rustc_version);
    println!("  profile       : {} (opt-level {})", env.profile, env.opt_level);
    println!("  commit        : {}{}", env.git_commit, if env.git_dirty { " (dirty)" } else { "" });
    println!("  rayon threads : {}", env.rayon_threads);
    println!("  mode          : {mode}");
    println!("  warmup/reps   : {}/{}", cfg.warmup, cfg.reps);
    println!();

    let run = bench::run_suite(&ids, &cfg, |r| {
        let s = &r.stats;
        println!(
            "  {:<28} median {:>10.3} ms   ci95 [{:>9.3}, {:>9.3}]   rsd {:>5.1} %   min {:>10.3} ms{}",
            r.spec.name,
            s.median,
            s.ci95_median[0],
            s.ci95_median[1],
            s.rsd * 100.0,
            s.min,
            if r.checksum_stable { "" } else { "   [!] UNSTABLE CHECKSUM" }
        );
    });

    let unstable: Vec<&str> =
        run.results.iter().filter(|r| !r.checksum_stable).map(|r| r.spec.name).collect();
    if !unstable.is_empty() {
        eprintln!(
            "\n[!] {} categor(y/ies) produced differing checksums across repetitions: {:?}",
            unstable.len(),
            unstable
        );
        eprintln!("    Those samples do not describe identical work and must not be reported.");
    }

    let noisy: Vec<&str> =
        run.results.iter().filter(|r| r.stats.rsd > 0.10).map(|r| r.spec.name).collect();
    if !noisy.is_empty() {
        eprintln!("\n[!] Relative standard deviation above 10 % in: {noisy:?}");
        eprintln!("    Re-run on an idle machine before reporting these values.");
    }

    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let json = serde_json::to_string_pretty(&run).expect("serialising results");
    let mut f = std::fs::File::create(&out).expect("creating result file");
    f.write_all(json.as_bytes()).expect("writing result file");
    println!("\nWrote {out}");
}

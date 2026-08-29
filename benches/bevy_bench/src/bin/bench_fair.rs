use bevy::prelude::*;
use std::io::Write;

#[path = "../harness.rs"]
mod harness;
use harness::{BenchCfg, Measured, Stats, Timer as BenchTimer, scaled};

macro_rules! payload {
    ($($name:ident),*) => {
        $(
            #[derive(Component, Clone, Copy, Default)]
            #[repr(C)]
            struct $name { x: f32, y: f32, z: f32, w: f32 }
        )*
    };
}
payload!(C0, C1, C2, C3, C4, C5, C6, C7);

macro_rules! markers {
    ($($name:ident),*) => { $(#[allow(dead_code)] #[derive(Component)] struct $name;)* };
}
markers!(T0, T1, T2, T3, T4, T5, T6, T7, T8);

#[derive(Component)]
struct Tag;

fn shuffled_indices(n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    for i in (1..n).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

fn checksum_c0(world: &mut World) -> u64 {
    let mut q = world.query::<&C0>();
    let mut sum = 0.0f64;
    for c in q.iter(world) {
        sum += c.x as f64;
    }
    sum.to_bits()
}

fn live_entities(world: &mut World) -> u64 {
    let mut q = world.query::<Entity>();
    q.iter(world).count() as u64
}

fn spawn_a(world: &mut World, n: usize, width: usize) {
    for i in 0..n {
        let base = i as f32;
        let mut e = world.spawn(C0 { x: base, y: 0., z: 0., w: 0. });
        if width >= 2 { e.insert(C1 { x: 1., y: 1., z: 1., w: 1. }); }
        if width >= 4 {
            e.insert(C2 { x: 1., y: 1., z: 1., w: 1. });
            e.insert(C3 { x: 1., y: 1., z: 1., w: 1. });
        }
        if width >= 8 {
            e.insert(C4 { x: 1., y: 1., z: 1., w: 1. });
            e.insert(C5 { x: 1., y: 1., z: 1., w: 1. });
            e.insert(C6 { x: 1., y: 1., z: 1., w: 1. });
            e.insert(C7 { x: 1., y: 1., z: 1., w: 1. });
        }
    }
}

const ITER_PASSES: usize = 20;

fn case_iter_write_1(n: usize, cfg: &BenchCfg) -> Measured {
    let passes = scaled(ITER_PASSES, cfg);
    let mut world = World::new();
    spawn_a(&mut world, n, 1);
    let mut q = world.query::<&mut C0>();
    let t = BenchTimer::start();
    for _ in 0..passes {
        q.iter_mut(&mut world).for_each(|mut c| c.x += 1.0);
        world.increment_change_tick();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_c0(&mut world) }
}

fn case_iter_rw_2(n: usize, cfg: &BenchCfg) -> Measured {
    let passes = scaled(ITER_PASSES, cfg);
    let mut world = World::new();
    spawn_a(&mut world, n, 2);
    let mut q = world.query::<(&mut C0, &C1)>();
    let t = BenchTimer::start();
    for _ in 0..passes {
        q.iter_mut(&mut world).for_each(|(mut a, b)| a.x += b.x);
        world.increment_change_tick();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_c0(&mut world) }
}

fn case_iter_rw_4(n: usize, cfg: &BenchCfg) -> Measured {
    let passes = scaled(ITER_PASSES, cfg);
    let mut world = World::new();
    spawn_a(&mut world, n, 4);
    let mut q = world.query::<(&mut C0, &C1, &C2, &C3)>();
    let t = BenchTimer::start();
    for _ in 0..passes {
        q.iter_mut(&mut world).for_each(|(mut a, b, c, d)| a.x += b.x + c.x + d.x);
        world.increment_change_tick();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_c0(&mut world) }
}

fn case_iter_rw_8(n: usize, cfg: &BenchCfg) -> Measured {
    let passes = scaled(ITER_PASSES, cfg);
    let mut world = World::new();
    spawn_a(&mut world, n, 8);
    let mut q = world.query::<(&mut C0, &C1, &C2, &C3, &C4, &C5, &C6, &C7)>();
    let t = BenchTimer::start();
    for _ in 0..passes {
        q.iter_mut(&mut world)
            .for_each(|(mut a, b, c, d, e, f, g, h)| a.x += b.x + c.x + d.x + e.x + f.x + g.x + h.x);
        world.increment_change_tick();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_c0(&mut world) }
}

fn case_iter_read_2(n: usize, cfg: &BenchCfg) -> Measured {
    let passes = scaled(ITER_PASSES, cfg);
    let mut world = World::new();
    spawn_a(&mut world, n, 2);
    let mut q = world.query::<(&C0, &C1)>();
    let mut acc = 0.0f64;
    let t = BenchTimer::start();
    for _ in 0..passes {
        for (a, b) in q.iter(&world) {
            acc += (a.x + b.x) as f64;
        }
        world.increment_change_tick();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: acc.to_bits() }
}

fn spawn_topology(world: &mut World, n: usize, k: usize) {
    for i in 0..n {
        let mut e = world.spawn((
            C0 { x: i as f32, y: 0., z: 0., w: 0. },
            C1 { x: 1., y: 1., z: 1., w: 1. },
        ));
        let bits = i % k;
        if bits & 1 != 0 { e.insert(T0); }
        if bits & 2 != 0 { e.insert(T1); }
        if bits & 4 != 0 { e.insert(T2); }
        if bits & 8 != 0 { e.insert(T3); }
        if bits & 16 != 0 { e.insert(T4); }
        if bits & 32 != 0 { e.insert(T5); }
        if bits & 64 != 0 { e.insert(T6); }
        if bits & 128 != 0 { e.insert(T7); }
        if bits & 256 != 0 { e.insert(T8); }
    }
}

fn case_topology(k: usize, cfg: &BenchCfg) -> Measured {
    let passes = scaled(ITER_PASSES, cfg);
    let mut world = World::new();
    spawn_topology(&mut world, 100_000, k);
    let mut q = world.query::<(&mut C0, &C1)>();
    let t = BenchTimer::start();
    for _ in 0..passes {
        q.iter_mut(&mut world).for_each(|(mut a, b)| a.x += b.x);
        world.increment_change_tick();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_c0(&mut world) }
}

const LIFECYCLE_N: usize = 200_000;

fn case_spawn_empty(cfg: &BenchCfg) -> Measured {
    let n = scaled(LIFECYCLE_N, cfg);
    let mut world = World::new();
    let t = BenchTimer::start();
    for _ in 0..n {
        world.spawn_empty();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: live_entities(&mut world) }
}

fn case_spawn_2comp(cfg: &BenchCfg) -> Measured {
    let n = scaled(LIFECYCLE_N, cfg);
    let mut world = World::new();
    let t = BenchTimer::start();
    for i in 0..n {
        world.spawn((
            C0 { x: i as f32, y: 0., z: 0., w: 0. },
            C1 { x: 1., y: 1., z: 1., w: 1. },
        ));
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: live_entities(&mut world) }
}

fn case_despawn(cfg: &BenchCfg) -> Measured {
    let n = scaled(LIFECYCLE_N, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..n)
        .map(|i| {
            world
                .spawn((
                    C0 { x: i as f32, y: 0., z: 0., w: 0. },
                    C1 { x: 1., y: 1., z: 1., w: 1. },
                ))
                .id()
        })
        .collect();
    let t = BenchTimer::start();
    for &e in &ents {
        world.despawn(e);
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: live_entities(&mut world) }
}

const STRUCT_N: usize = 100_000;

fn case_add_component(cfg: &BenchCfg) -> Measured {
    let n = scaled(STRUCT_N, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..n)
        .map(|i| world.spawn(C0 { x: i as f32, y: 0., z: 0., w: 0. }).id())
        .collect();
    let t = BenchTimer::start();
    for &e in &ents {
        world.entity_mut(e).insert(Tag);
    }
    let ms = t.elapsed_ms();
    let mut q = world.query_filtered::<Entity, With<Tag>>();
    let tagged = q.iter(&world).count() as u64;
    Measured { ms, checksum: tagged }
}

fn case_remove_component(cfg: &BenchCfg) -> Measured {
    let n = scaled(STRUCT_N, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..n)
        .map(|i| world.spawn((C0 { x: i as f32, y: 0., z: 0., w: 0. }, Tag)).id())
        .collect();
    let t = BenchTimer::start();
    for &e in &ents {
        world.entity_mut(e).remove::<Tag>();
    }
    let ms = t.elapsed_ms();
    let mut q = world.query_filtered::<Entity, With<Tag>>();
    let tagged = q.iter(&world).count() as u64;
    Measured { ms, checksum: live_entities(&mut world) << 32 | tagged }
}

fn case_add_remove_cycle(cfg: &BenchCfg) -> Measured {
    let cycles = scaled(20, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..20_000)
        .map(|i| world.spawn(C0 { x: i as f32, y: 0., z: 0., w: 0. }).id())
        .collect();
    let t = BenchTimer::start();
    for _ in 0..cycles {
        for &e in &ents {
            world.entity_mut(e).insert(Tag);
        }
        for &e in &ents {
            world.entity_mut(e).remove::<Tag>();
        }
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: live_entities(&mut world) }
}

const RANDOM_N: usize = 100_000;

fn case_random_get(cfg: &BenchCfg) -> Measured {
    let passes = scaled(10, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..RANDOM_N)
        .map(|i| {
            world
                .spawn((
                    C0 { x: i as f32, y: 0., z: 0., w: 0. },
                    C1 { x: 1., y: 1., z: 1., w: 1. },
                ))
                .id()
        })
        .collect();
    let order = shuffled_indices(RANDOM_N);
    let mut acc = 0.0f64;
    let t = BenchTimer::start();
    for _ in 0..passes {
        for &idx in &order {
            if let Some(c) = world.get::<C0>(ents[idx]) {
                acc += c.x as f64;
            }
        }
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: acc.to_bits() }
}

fn case_random_write(cfg: &BenchCfg) -> Measured {
    let passes = scaled(10, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..RANDOM_N)
        .map(|i| {
            world
                .spawn((
                    C0 { x: i as f32, y: 0., z: 0., w: 0. },
                    C1 { x: 1., y: 1., z: 1., w: 1. },
                ))
                .id()
        })
        .collect();
    let order = shuffled_indices(RANDOM_N);
    let t = BenchTimer::start();
    for _ in 0..passes {
        for &idx in &order {
            if let Some(mut c) = world.get_mut::<C0>(ents[idx]) {
                c.x += 1.0;
            }
        }
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_c0(&mut world) }
}

fn case_changed_sparse(cfg: &BenchCfg) -> Measured {
    let passes = scaled(20, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..200_000)
        .map(|i| {
            world
                .spawn((
                    C0 { x: i as f32, y: 0., z: 0., w: 0. },
                    C1 { x: 1., y: 1., z: 1., w: 1. },
                ))
                .id()
        })
        .collect();
    let touched: Vec<Entity> = ents.iter().copied().step_by(100).collect();

    let mut q_changed = world.query_filtered::<&C0, Changed<C0>>();
    let mut observed = 0u64;
    let t = BenchTimer::start();
    for _ in 0..passes {
        for &e in &touched {
            if let Some(mut c) = world.get_mut::<C0>(e) {
                c.x += 1.0;
            }
        }
        observed += q_changed.iter(&world).count() as u64;
        world.clear_trackers();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: observed }
}

fn sched_sys_a(mut q: Query<(&mut C0, &C1)>) { q.iter_mut().for_each(|(mut a, b)| a.x += b.x); }
fn sched_sys_b(mut q: Query<(&mut C1, &C2)>) { q.iter_mut().for_each(|(mut a, b)| a.y += b.y); }
fn sched_sys_c(mut q: Query<(&mut C2, &C3)>) { q.iter_mut().for_each(|(mut a, b)| a.z += b.z); }

fn case_schedule_3sys(cfg: &BenchCfg) -> Measured {
    let passes = scaled(20, cfg);
    let mut world = World::new();
    for i in 0..100_000 {
        world.spawn((
            C0 { x: i as f32, y: 0., z: 0., w: 0. },
            C1 { x: 1., y: 1., z: 1., w: 1. },
            C2 { x: 1., y: 1., z: 1., w: 1. },
            C3 { x: 1., y: 1., z: 1., w: 1. },
        ));
    }
    let mut schedule = Schedule::default();
    schedule.add_systems((sched_sys_a, sched_sys_b, sched_sys_c).chain());
    let t = BenchTimer::start();
    for _ in 0..passes {
        schedule.run(&mut world);
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_c0(&mut world) }
}

#[derive(serde::Serialize)]
struct FairSpec {
    id: &'static str,
    group: &'static str,
    label: String,
    sweep_key: &'static str,
    sweep_value: u64,
    entity_count: usize,
    description: &'static str,
}

#[derive(serde::Serialize)]
struct FairResult {
    #[serde(flatten)]
    spec: FairSpec,
    checksum: String,
    checksum_stable: bool,
    stats: Stats,
}

struct Case {
    spec: FairSpec,
    f: Box<dyn Fn(&BenchCfg) -> Measured>,
}

fn fmt_n(n: usize) -> String {
    if n >= 1_000_000 { format!("{}M", n / 1_000_000) }
    else if n >= 1_000 { format!("{}k", n / 1_000) }
    else { n.to_string() }
}

fn cases() -> Vec<Case> {
    let mut out: Vec<Case> = Vec::new();
    for &n in &[1_000usize, 10_000, 100_000, 1_000_000] {
        out.push(Case { spec: FairSpec { id: "A1", group: "Iteration", label: format!("write 1 component - {}", fmt_n(n)), sweep_key: "entities", sweep_value: n as u64, entity_count: n, description: "20 passes of read-modify-write over a single component column" }, f: Box::new(move |c| case_iter_write_1(n, c)) });
        out.push(Case { spec: FairSpec { id: "A2", group: "Iteration", label: format!("read 1 / write 1 - {}", fmt_n(n)), sweep_key: "entities", sweep_value: n as u64, entity_count: n, description: "20 passes of the canonical position/velocity loop" }, f: Box::new(move |c| case_iter_rw_2(n, c)) });
        out.push(Case { spec: FairSpec { id: "A3", group: "Iteration", label: format!("4 components - {}", fmt_n(n)), sweep_key: "entities", sweep_value: n as u64, entity_count: n, description: "20 passes touching four component columns" }, f: Box::new(move |c| case_iter_rw_4(n, c)) });
        out.push(Case { spec: FairSpec { id: "A4", group: "Iteration", label: format!("8 components - {}", fmt_n(n)), sweep_key: "entities", sweep_value: n as u64, entity_count: n, description: "20 passes touching eight component columns" }, f: Box::new(move |c| case_iter_rw_8(n, c)) });
        out.push(Case { spec: FairSpec { id: "A5", group: "Iteration", label: format!("read-only 2 - {}", fmt_n(n)), sweep_key: "entities", sweep_value: n as u64, entity_count: n, description: "20 read-only passes — isolates the cost change tracking adds to writes" }, f: Box::new(move |c| case_iter_read_2(n, c)) });
    }
    for &k in &[1usize, 8, 64, 512] {
        out.push(Case { spec: FairSpec { id: "B1", group: "Topology", label: format!("{k} archetypes - 100k"), sweep_key: "archetypes", sweep_value: k as u64, entity_count: 100_000, description: "100k entities spread over k archetypes, then iterated" }, f: Box::new(move |c| case_topology(k, c)) });
    }
    out.push(Case { spec: FairSpec { id: "C1", group: "Lifecycle", label: "spawn empty - 200k".into(), sweep_key: "entities", sweep_value: 200_000, entity_count: 200_000, description: "identifier allocation with no component data" }, f: Box::new(case_spawn_empty) });
    out.push(Case { spec: FairSpec { id: "C2", group: "Lifecycle", label: "spawn 2 components - 200k".into(), sweep_key: "entities", sweep_value: 200_000, entity_count: 200_000, description: "allocation plus archetype placement and column writes" }, f: Box::new(case_spawn_2comp) });
    out.push(Case { spec: FairSpec { id: "C3", group: "Lifecycle", label: "despawn - 200k".into(), sweep_key: "entities", sweep_value: 200_000, entity_count: 200_000, description: "removal, row backfill and identifier recycling" }, f: Box::new(case_despawn) });
    out.push(Case { spec: FairSpec { id: "D1", group: "Structural", label: "add component - 100k".into(), sweep_key: "entities", sweep_value: 100_000, entity_count: 100_000, description: "one archetype move per entity" }, f: Box::new(case_add_component) });
    out.push(Case { spec: FairSpec { id: "D2", group: "Structural", label: "remove component - 100k".into(), sweep_key: "entities", sweep_value: 100_000, entity_count: 100_000, description: "the reverse archetype move" }, f: Box::new(case_remove_component) });
    out.push(Case { spec: FairSpec { id: "D3", group: "Structural", label: "add/remove cycle - 20k × 20".into(), sweep_key: "entities", sweep_value: 20_000, entity_count: 20_000, description: "repeated moves, exercising any archetype-transition cache" }, f: Box::new(case_add_remove_cycle) });
    out.push(Case { spec: FairSpec { id: "E1", group: "Random access", label: "random get - 100k × 10".into(), sweep_key: "entities", sweep_value: 100_000, entity_count: 100_000, description: "component lookup by entity handle in shuffled order — the case archetype layouts are weakest at" }, f: Box::new(case_random_get) });
    out.push(Case { spec: FairSpec { id: "E2", group: "Random access", label: "random write - 100k × 10".into(), sweep_key: "entities", sweep_value: 100_000, entity_count: 100_000, description: "the same lookup, mutating" }, f: Box::new(case_random_write) });
    out.push(Case { spec: FairSpec { id: "F1", group: "Change detection", label: "sparse changes - 200k".into(), sweep_key: "entities", sweep_value: 200_000, entity_count: 200_000, description: "1 % of rows mutated per pass, then queried by change filter" }, f: Box::new(case_changed_sparse) });
    out.push(Case { spec: FairSpec { id: "G1", group: "Scheduling", label: "3 systems - 100k".into(), sweep_key: "entities", sweep_value: 100_000, entity_count: 100_000, description: "three registered systems over the same data through the engine's scheduler" }, f: Box::new(case_schedule_3sys) });
    out
}

#[derive(serde::Serialize)]
struct FairRun {
    framework: &'static str,
    schema_version: u32,
    suite: &'static str,
    threading: &'static str,
    env: harness::BenchEnv,
    cfg: BenchCfg,
    results: Vec<FairResult>,
}

fn main() {

    bevy::tasks::ComputeTaskPool::get_or_init(|| {
        bevy::tasks::TaskPoolBuilder::new().num_threads(1).build()
    });

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg = BenchCfg { warmup: 3, reps: 15, parallel: false, work_scale: 1.0 };
    let mut out_path = "../../results/fair_bevy.json".to_string();
    let mut only: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--reps" => { i += 1; cfg.reps = args[i].parse().unwrap(); }
            "--warmup" => { i += 1; cfg.warmup = args[i].parse().unwrap(); }
            "--scale" => { i += 1; cfg.work_scale = args[i].parse().unwrap(); }
            "--only" => { i += 1; only = Some(args[i].clone()); }
            "--out" => { i += 1; out_path = args[i].clone(); }
            other => { eprintln!("unknown argument: {other}"); std::process::exit(2); }
        }
        i += 1;
    }

    let env = harness::BenchEnv::capture();
    println!("\n=== BEVY 0.18.1 — NEUTRAL ECS SUITE (schema v3, single-threaded) ===");
    println!("  cpu         : {}", env.cpu_brand);
    println!("  warmup/reps : {}/{}", cfg.warmup, cfg.reps);
    println!();

    let mut results = Vec::new();
    let mut group = String::new();
    for case in cases() {
        if let Some(f) = &only {
            if case.spec.id != f && case.spec.group != f {
                continue;
            }
        }
        for _ in 0..cfg.warmup {
            std::hint::black_box((case.f)(&cfg).checksum);
        }
        let mut samples = Vec::with_capacity(cfg.reps);
        let mut checksum = 0u64;
        let mut stable = true;
        for i in 0..cfg.reps {
            let m = (case.f)(&cfg);
            samples.push(m.ms);
            if i == 0 { checksum = m.checksum; } else if m.checksum != checksum { stable = false; }
        }
        let stats = Stats::from_samples(samples);
        if case.spec.group != group {
            group = case.spec.group.to_string();
            println!("  {}", group.to_uppercase());
        }
        println!(
            "    {:<32} {:>10.3} ms   ci95 [{:>8.3}, {:>8.3}]   rsd {:>5.1} %{}",
            case.spec.label, stats.median, stats.ci95_median[0], stats.ci95_median[1],
            stats.rsd * 100.0, if stable { "" } else { "   [!] UNSTABLE" }
        );
        results.push(FairResult { spec: case.spec, checksum: checksum.to_string(), checksum_stable: stable, stats });
    }

    let run = FairRun {
        framework: "bevy",
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

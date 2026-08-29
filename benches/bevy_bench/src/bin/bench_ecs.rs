use bevy::prelude::*;
use std::io::Write;

#[path = "../harness.rs"]
mod harness;
use harness::*;

use harness::Timer as BenchTimer;

#[derive(Component, Clone, Copy)]
#[repr(C)]
struct Pos {
    x: f32,
    y: f32,
    z: f32,
}
#[derive(Component, Clone, Copy)]
#[repr(C)]
struct Vel {
    x: f32,
    y: f32,
    z: f32,
}
#[derive(Component)]
struct Marker;
#[derive(Component)]
struct MarkerA;
#[derive(Component)]
struct MarkerB;

macro_rules! define_markers {
    ($($name:ident),*) => { $(#[allow(dead_code)] #[derive(Component)] struct $name;)* };
}
define_markers!(
    M0, M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16, M17, M18, M19, M20,
    M21, M22, M23, M24, M25
);

fn checksum_pos_x(world: &mut World) -> u64 {
    let mut q = world.query::<&Pos>();
    let mut sum = 0.0f64;
    for p in q.iter(world) {
        sum += p.x as f64;
    }
    sum.to_bits()
}

fn live_entities(world: &mut World) -> u64 {
    let mut q = world.query::<Entity>();
    q.iter(world).count() as u64
}

macro_rules! drive {
    ($query:expr, $world:expr, $parallel:expr, $body:expr) => {
        if $parallel {
            $query.par_iter_mut(&mut $world).for_each($body);
        } else {
            $query.iter_mut(&mut $world).for_each($body);
        }
    };
}

fn case_spawn(cfg: &BenchCfg) -> Measured {
    let n = scaled(1_000_000, cfg);
    let mut world = World::new();
    let t = BenchTimer::start();
    for _ in 0..n {
        world.spawn((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: live_entities(&mut world) }
}

fn case_despawn(cfg: &BenchCfg) -> Measured {
    let n = scaled(1_000_000, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..n).map(|_| world.spawn_empty().id()).collect();
    let t = BenchTimer::start();
    for &e in &ents {
        world.despawn(e);
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: live_entities(&mut world) }
}

fn case_dense_iteration(cfg: &BenchCfg) -> Measured {
    let iters = scaled(100, cfg);
    let mut world = World::new();
    for _ in 0..1_000_000 {
        world.spawn((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
    }
    let mut query = world.query::<(&mut Pos, &Vel)>();
    let t = BenchTimer::start();
    for _ in 0..iters {
        drive!(query, world, cfg.parallel, |(mut p, v): (Mut<Pos>, &Vel)| p.x += v.x);

        world.increment_change_tick();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_pos_x(&mut world) }
}

fn case_fragmented_iteration(cfg: &BenchCfg) -> Measured {
    let iters = scaled(100, cfg);
    let mut world = World::new();
    for i in 0..100_000 {
        let mut e = world.spawn((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
        match i % 26 {
            0 => { e.insert(M0); }
            1 => { e.insert(M1); }
            2 => { e.insert(M2); }
            3 => { e.insert(M3); }
            4 => { e.insert(M4); }
            5 => { e.insert(M5); }
            6 => { e.insert(M6); }
            7 => { e.insert(M7); }
            8 => { e.insert(M8); }
            9 => { e.insert(M9); }
            10 => { e.insert(M10); }
            11 => { e.insert(M11); }
            12 => { e.insert(M12); }
            13 => { e.insert(M13); }
            14 => { e.insert(M14); }
            15 => { e.insert(M15); }
            16 => { e.insert(M16); }
            17 => { e.insert(M17); }
            18 => { e.insert(M18); }
            19 => { e.insert(M19); }
            20 => { e.insert(M20); }
            21 => { e.insert(M21); }
            22 => { e.insert(M22); }
            23 => { e.insert(M23); }
            24 => { e.insert(M24); }
            _ => { e.insert(M25); }
        }
    }
    let mut query = world.query::<(&mut Pos, &Vel)>();
    let t = BenchTimer::start();
    for _ in 0..iters {
        drive!(query, world, cfg.parallel, |(mut p, v): (Mut<Pos>, &Vel)| p.x += v.x);
        world.increment_change_tick();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_pos_x(&mut world) }
}

fn case_add_remove_churn(cfg: &BenchCfg) -> Measured {
    let iters = scaled(100, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..100_000).map(|_| world.spawn_empty().id()).collect();
    let t = BenchTimer::start();
    for _ in 0..iters {
        for &e in &ents {
            world.entity_mut(e).insert(Marker);
        }
        for &e in &ents {
            world.entity_mut(e).remove::<Marker>();
        }
    }
    let ms = t.elapsed_ms();

    let mut q = world.query_filtered::<Entity, With<Marker>>();
    let remaining = q.iter(&world).count() as u64;
    Measured { ms, checksum: live_entities(&mut world) << 32 | remaining }
}

fn case_query_filtering(cfg: &BenchCfg) -> Measured {
    let iters = scaled(100, cfg);
    let mut world = World::new();
    for i in 0..200_000 {
        let mut e = world.spawn((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
        if i % 2 == 0 {
            e.insert(MarkerA);
        } else {
            e.insert(MarkerB);
        }
    }
    let mut query =
        world.query_filtered::<(&mut Pos, &Vel), (With<MarkerA>, Without<MarkerB>)>();
    let t = BenchTimer::start();
    for _ in 0..iters {
        drive!(query, world, cfg.parallel, |(mut p, v): (Mut<Pos>, &Vel)| p.x += v.x);
        world.increment_change_tick();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_pos_x(&mut world) }
}

fn insert_marker_bits(e: &mut EntityWorldMut, bits: u32) {
    if bits & 1 != 0 { e.insert(M0); }
    if bits & 2 != 0 { e.insert(M1); }
    if bits & 4 != 0 { e.insert(M2); }
    if bits & 8 != 0 { e.insert(M3); }
    if bits & 16 != 0 { e.insert(M4); }
    if bits & 32 != 0 { e.insert(M5); }
    if bits & 64 != 0 { e.insert(M6); }
    if bits & 128 != 0 { e.insert(M7); }
}

fn case_mixed_density(cfg: &BenchCfg) -> Measured {
    let iters = scaled(100, cfg);
    let mut world = World::new();
    for i in 0..255u32 {
        let mut e = world.spawn((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
        insert_marker_bits(&mut e, i);
    }
    for _ in 0..100_000 {
        let mut e = world.spawn((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
        insert_marker_bits(&mut e, 0xFF);
    }
    let mut query = world.query::<(&mut Pos, &Vel)>();
    let t = BenchTimer::start();
    for _ in 0..iters {
        drive!(query, world, cfg.parallel, |(mut p, v): (Mut<Pos>, &Vel)| p.x += v.x);
        world.increment_change_tick();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_pos_x(&mut world) }
}

fn sys1_par(mut q: Query<(&mut Pos, &Vel)>) { q.par_iter_mut().for_each(|(mut p, v)| p.x += v.x); }
fn sys2_par(mut q: Query<(&mut Pos, &Vel)>) { q.par_iter_mut().for_each(|(mut p, v)| p.y += v.y); }
fn sys3_par(mut q: Query<(&mut Pos, &Vel)>) { q.par_iter_mut().for_each(|(mut p, v)| p.z += v.z); }
fn sys1_ser(mut q: Query<(&mut Pos, &Vel)>) { q.iter_mut().for_each(|(mut p, v)| p.x += v.x); }
fn sys2_ser(mut q: Query<(&mut Pos, &Vel)>) { q.iter_mut().for_each(|(mut p, v)| p.y += v.y); }
fn sys3_ser(mut q: Query<(&mut Pos, &Vel)>) { q.iter_mut().for_each(|(mut p, v)| p.z += v.z); }

fn case_scheduling(cfg: &BenchCfg) -> Measured {
    let iters = scaled(100, cfg);
    let mut world = World::new();
    for _ in 0..100_000 {
        world.spawn((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
    }
    let mut schedule = Schedule::default();
    if cfg.parallel {
        schedule.add_systems((sys1_par, sys2_par, sys3_par));
    } else {
        schedule.add_systems((sys1_ser, sys2_ser, sys3_ser));
    }
    let t = BenchTimer::start();
    for _ in 0..iters {
        schedule.run(&mut world);
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_pos_x(&mut world) }
}

fn sys_add(mut cmds: Commands, q: Query<Entity, Without<Marker>>) {
    q.iter().for_each(|e| {
        cmds.entity(e).insert(Marker);
    });
}
fn sys_remove(mut cmds: Commands, q: Query<Entity, With<Marker>>) {
    q.iter().for_each(|e| {
        cmds.entity(e).remove::<Marker>();
    });
}

fn case_system_churn(cfg: &BenchCfg) -> Measured {
    let iters = scaled(100, cfg);
    let mut world = World::new();
    for _ in 0..100_000 {
        world.spawn_empty();
    }
    let mut schedule = Schedule::default();
    schedule.add_systems((sys_add, sys_remove));
    let t = BenchTimer::start();
    for _ in 0..iters {
        schedule.run(&mut world);
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: live_entities(&mut world) }
}

fn case_sparse_change_stress(cfg: &BenchCfg) -> Measured {
    let iters = scaled(100, cfg);
    let mut world = World::new();
    let mut touched = Vec::new();
    for i in 0..1_000_000u32 {
        let mut e = world.spawn((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
        let arch_idx = i % 100;
        insert_marker_bits(&mut e, arch_idx & 0x7F);
        if arch_idx == 0 && touched.len() < 1000 {
            touched.push(e.id());
        }
    }
    let mut query_mutate = world.query::<&mut Pos>();
    let mut query_changed = world.query_filtered::<&Pos, Changed<Pos>>();
    let mut observed = 0u64;
    let t = BenchTimer::start();
    for _ in 0..iters {
        for &e in &touched {
            if let Ok(mut p) = query_mutate.get_mut(&mut world, e) {
                p.x += 1.0;
            }
        }
        for p in query_changed.iter(&world) {
            observed += p.x as u64;
        }
        world.clear_trackers();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: observed }
}

fn case_bulk_structural_churn(cfg: &BenchCfg) -> Measured {
    let rounds = scaled(100, cfg);
    let per_round = 10_000;
    let mut world = World::new();
    let t = BenchTimer::start();
    for _ in 0..rounds {
        let mut ents = Vec::with_capacity(per_round);
        for _ in 0..per_round {

            ents.push(
                world
                    .spawn((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }, Marker))
                    .id(),
            );
        }
        for &e in &ents {
            world.entity_mut(e).remove::<Marker>();
        }
        for &e in &ents {
            world.despawn(e);
        }
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: live_entities(&mut world) }
}

struct Case {
    spec: BenchSpec,
    f: CaseFn,
}

fn core_cases() -> Vec<Case> {

    vec![
        Case { spec: BenchSpec { id: 1, name: "1. Entity Spawn", entity_count: 1_000_000, inner_iters: 1, group: Group::Core, subquestion: SubQuestion::NativeComparison, measures: "1M individual spawn((Pos, Vel)) calls incl. archetype lookup and column growth", excludes: "World construction", checksum_of: "live entity count after the measured loop" }, f: case_spawn },
        Case { spec: BenchSpec { id: 2, name: "2. Entity Despawn", entity_count: 1_000_000, inner_iters: 1, group: Group::Core, subquestion: SubQuestion::NativeComparison, measures: "1M individual despawn() calls", excludes: "spawning the 1M entities and collecting their handles", checksum_of: "live entity count after the measured loop (must be 0)" }, f: case_despawn },
        Case { spec: BenchSpec { id: 3, name: "3. Dense Iteration", entity_count: 1_000_000, inner_iters: 100, group: Group::Core, subquestion: SubQuestion::EcsProperties, measures: "100 passes of (&mut Pos, &Vel) over one dense table, incl. per-pass change-tick increment", excludes: "spawning the 1M entities and building the initial query state", checksum_of: "sum of all Pos.x as f64 bits (expected 100.0 per entity)" }, f: case_dense_iteration },
        Case { spec: BenchSpec { id: 4, name: "4. Fragmented Iteration", entity_count: 100_000, inner_iters: 100, group: Group::Core, subquestion: SubQuestion::EcsProperties, measures: "100 passes of (&mut Pos, &Vel) spread over 26 archetypes", excludes: "spawning and distributing the entities over the marker components", checksum_of: "sum of all Pos.x as f64 bits" }, f: case_fragmented_iteration },
        Case { spec: BenchSpec { id: 5, name: "5. Add/Remove Churn", entity_count: 100_000, inner_iters: 100, group: Group::Core, subquestion: SubQuestion::EcsProperties, measures: "100 rounds of adding and removing one marker on 100k entities (200k archetype moves per round)", excludes: "spawning the entities", checksum_of: "live entity count and remaining marker matches (must be 0)" }, f: case_add_remove_churn },
        Case { spec: BenchSpec { id: 6, name: "6. Query Filtering", entity_count: 200_000, inner_iters: 100, group: Group::Core, subquestion: SubQuestion::EcsProperties, measures: "100 passes of (&mut Pos, &Vel) filtered by (With<A>, Without<B>) — half the entities match", excludes: "spawning and marking the entities", checksum_of: "sum of all Pos.x as f64 bits (only matching rows advance)" }, f: case_query_filtering },
        Case { spec: BenchSpec { id: 7, name: "7. Mixed Density", entity_count: 100_255, inner_iters: 100, group: Group::Core, subquestion: SubQuestion::EcsProperties, measures: "100 passes over 255 single-entity archetypes plus one archetype holding 100k entities", excludes: "building the 256 archetype combinations", checksum_of: "sum of all Pos.x as f64 bits" }, f: case_mixed_density },
        Case { spec: BenchSpec { id: 8, name: "8. Scheduling", entity_count: 100_000, inner_iters: 100, group: Group::Core, subquestion: SubQuestion::NativeComparison, measures: "100 full schedule runs of three systems that write different Pos fields", excludes: "World construction, entity spawning and schedule construction", checksum_of: "sum of all Pos.x as f64 bits" }, f: case_scheduling },
        Case { spec: BenchSpec { id: 9, name: "9. System Churn", entity_count: 100_000, inner_iters: 100, group: Group::Core, subquestion: SubQuestion::EcsProperties, measures: "100 updates of two systems adding and removing a marker through deferred Commands", excludes: "World construction, entity spawning and schedule construction", checksum_of: "live entity count" }, f: case_system_churn },
        Case { spec: BenchSpec { id: 10, name: "10. Sparse Change Stress", entity_count: 1_000_000, inner_iters: 100, group: Group::Core, subquestion: SubQuestion::EcsProperties, measures: "100 rounds of mutating up to 1000 rows through the tracked write path, then querying Changed<Pos> across 100 archetypes", excludes: "spawning and distributing 1M entities over 100 component combinations", checksum_of: "accumulated values observed through Changed<Pos>" }, f: case_sparse_change_stress },
        Case { spec: BenchSpec { id: 11, name: "11. Bulk Structural Churn", entity_count: 1_000_000, inner_iters: 100, group: Group::Core, subquestion: SubQuestion::NativeComparison, measures: "100 rounds of spawning 10k entities, removing a marker from each and despawning them — 1M entities in total", excludes: "World construction", checksum_of: "live entity count after the measured loop (must be 0)" }, f: case_bulk_structural_churn },
    ]
}

#[derive(serde::Serialize)]
struct BenchRun {
    framework: &'static str,
    schema_version: u32,
    env: BenchEnv,
    cfg: BenchCfg,
    results: Vec<CaseResult>,
}

fn main() {
    bevy::tasks::ComputeTaskPool::get_or_init(|| bevy::tasks::TaskPoolBuilder::new().build());

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg = BenchCfg::default();
    let mut ids: Vec<u32> = Vec::new();
    let mut out_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--serial" => cfg.parallel = false,
            "--parallel" => cfg.parallel = true,
            "--reps" => { i += 1; cfg.reps = args[i].parse().expect("--reps expects a number"); }
            "--warmup" => { i += 1; cfg.warmup = args[i].parse().expect("--warmup expects a number"); }
            "--scale" => { i += 1; cfg.work_scale = args[i].parse().expect("--scale expects a number"); }
            "--out" => { i += 1; out_path = Some(args[i].clone()); }
            other => match other.parse::<u32>() {
                Ok(id) => ids.push(id),
                Err(_) => { eprintln!("unknown argument: {other}"); std::process::exit(2); }
            },
        }
        i += 1;
    }

    let mode = if cfg.parallel { "parallel" } else { "serial" };
    let out = out_path.unwrap_or_else(|| format!("../../results/bevy_ecs_{mode}.json"));

    let env = BenchEnv::capture();
    println!("\n=== BEVY 0.18.1 ECS BENCHMARK (schema v2) ===");
    println!("  cpu         : {}", env.cpu_brand);
    println!("  os          : {}", env.os_description);
    println!("  task pool   : {} threads", env.available_parallelism);
    println!("  mode        : {mode}");
    println!("  warmup/reps : {}/{}", cfg.warmup, cfg.reps);
    println!();

    let mut results = Vec::new();
    for case in core_cases() {
        if !ids.is_empty() && !ids.contains(&case.spec.id) {
            continue;
        }
        let r = measure(case.spec, case.f, &cfg);
        let s = &r.stats;
        println!(
            "  {:<28} median {:>10.3} ms   ci95 [{:>9.3}, {:>9.3}]   rsd {:>5.1} %   min {:>10.3} ms{}",
            r.spec.name, s.median, s.ci95_median[0], s.ci95_median[1], s.rsd * 100.0, s.min,
            if r.checksum_stable { "" } else { "   [!] UNSTABLE CHECKSUM" }
        );
        results.push(r);
    }

    let run = BenchRun { framework: "bevy", schema_version: 2, env, cfg, results };

    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let json = serde_json::to_string_pretty(&run).expect("serialising results");
    std::fs::File::create(&out)
        .expect("creating result file")
        .write_all(json.as_bytes())
        .expect("writing result file");
    println!("\nWrote {out}");
}

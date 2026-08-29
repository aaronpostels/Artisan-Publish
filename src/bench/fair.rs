use super::harness::*;
use crate::ecs::*;
use crate::engine::App;

macro_rules! payload {
    ($($name:ident),*) => {
        $(
            #[derive(Clone, Copy, Default)]
            #[repr(C)]
            pub struct $name { pub x: f32, pub y: f32, pub z: f32, pub w: f32 }
        )*
    };
}
payload!(C0, C1, C2, C3, C4, C5, C6, C7);

macro_rules! topology_markers {
    ($($name:ident),*) => { $(pub struct $name;)* };
}
topology_markers!(
    T0, T1, T2, T3, T4, T5, T6, T7, T8
);

pub struct Tag;

fn shuffled_indices(n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    for i in (1..n).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

fn f64_sum_bits(v: f64) -> u64 {
    v.to_bits()
}

fn spawn_a(world: &mut World, n: usize, width: usize) {
    for i in 0..n {
        let base = i as f32;
        let e = world.spawn_with((C0 { x: base, y: 0., z: 0., w: 0. },));
        if width >= 2 { world.add_component(e, C1 { x: 1., y: 1., z: 1., w: 1. }); }
        if width >= 4 {
            world.add_component(e, C2 { x: 1., y: 1., z: 1., w: 1. });
            world.add_component(e, C3 { x: 1., y: 1., z: 1., w: 1. });
        }
        if width >= 8 {
            world.add_component(e, C4 { x: 1., y: 1., z: 1., w: 1. });
            world.add_component(e, C5 { x: 1., y: 1., z: 1., w: 1. });
            world.add_component(e, C6 { x: 1., y: 1., z: 1., w: 1. });
            world.add_component(e, C7 { x: 1., y: 1., z: 1., w: 1. });
        }
    }
}

fn checksum_c0(world: &mut World) -> u64 {
    let mut st = <Query<&C0>>::init_state(world);
    let mut q = unsafe { <Query<&C0>>::get_param(&mut st, world) };
    let mut sum = 0.0f64;
    q.for_each(|c| sum += c.x as f64);
    f64_sum_bits(sum)
}

const ITER_PASSES: usize = 20;

fn case_iter_write_1(n: usize) -> impl Fn(&BenchCfg) -> Measured {
    move |cfg| {
        let passes = scaled(ITER_PASSES, cfg);
        let mut world = World::new();
        spawn_a(&mut world, n, 1);
        let mut st = <Query<&mut C0>>::init_state(&mut world);
        let t = Timer::start();
        for _ in 0..passes {
            let mut q = unsafe { <Query<&mut C0>>::get_param(&mut st, &world) };
            q.for_each(|c| c.x += 1.0);
            world.current_tick += 1;
        }
        let ms = t.elapsed_ms();
        Measured { ms, checksum: checksum_c0(&mut world) }
    }
}

fn case_iter_rw_2(n: usize) -> impl Fn(&BenchCfg) -> Measured {
    move |cfg| {
        let passes = scaled(ITER_PASSES, cfg);
        let mut world = World::new();
        spawn_a(&mut world, n, 2);
        let mut st = <Query<(&mut C0, &C1)>>::init_state(&mut world);
        let t = Timer::start();
        for _ in 0..passes {
            let mut q = unsafe { <Query<(&mut C0, &C1)>>::get_param(&mut st, &world) };
            q.for_each(|(a, b)| a.x += b.x);
            world.current_tick += 1;
        }
        let ms = t.elapsed_ms();
        Measured { ms, checksum: checksum_c0(&mut world) }
    }
}

fn case_iter_rw_4(n: usize) -> impl Fn(&BenchCfg) -> Measured {
    move |cfg| {
        let passes = scaled(ITER_PASSES, cfg);
        let mut world = World::new();
        spawn_a(&mut world, n, 4);
        let mut st = <Query<(&mut C0, &C1, &C2, &C3)>>::init_state(&mut world);
        let t = Timer::start();
        for _ in 0..passes {
            let mut q = unsafe { <Query<(&mut C0, &C1, &C2, &C3)>>::get_param(&mut st, &world) };
            q.for_each(|(a, b, c, d)| a.x += b.x + c.x + d.x);
            world.current_tick += 1;
        }
        let ms = t.elapsed_ms();
        Measured { ms, checksum: checksum_c0(&mut world) }
    }
}

fn case_iter_rw_8(n: usize) -> impl Fn(&BenchCfg) -> Measured {
    move |cfg| {
        let passes = scaled(ITER_PASSES, cfg);
        let mut world = World::new();
        spawn_a(&mut world, n, 8);
        let mut st =
            <Query<(&mut C0, &C1, &C2, &C3, &C4, &C5, &C6, &C7)>>::init_state(&mut world);
        let t = Timer::start();
        for _ in 0..passes {
            let mut q = unsafe {
                <Query<(&mut C0, &C1, &C2, &C3, &C4, &C5, &C6, &C7)>>::get_param(&mut st, &world)
            };
            q.for_each(|(a, b, c, d, e, f, g, h)| {
                a.x += b.x + c.x + d.x + e.x + f.x + g.x + h.x
            });
            world.current_tick += 1;
        }
        let ms = t.elapsed_ms();
        Measured { ms, checksum: checksum_c0(&mut world) }
    }
}

fn case_iter_read_2(n: usize) -> impl Fn(&BenchCfg) -> Measured {
    move |cfg| {
        let passes = scaled(ITER_PASSES, cfg);
        let mut world = World::new();
        spawn_a(&mut world, n, 2);
        let mut st = <Query<(&C0, &C1)>>::init_state(&mut world);

        let mut acc = 0.0f64;
        let t = Timer::start();
        for _ in 0..passes {
            let mut q = unsafe { <Query<(&C0, &C1)>>::get_param(&mut st, &world) };
            q.for_each(|(a, b)| acc += (a.x + b.x) as f64);
            world.current_tick += 1;
        }
        let ms = t.elapsed_ms();
        Measured { ms, checksum: f64_sum_bits(acc) }
    }
}

fn spawn_topology(world: &mut World, n: usize, k: usize) {
    for i in 0..n {
        let e = world.spawn_with((
            C0 { x: i as f32, y: 0., z: 0., w: 0. },
            C1 { x: 1., y: 1., z: 1., w: 1. },
        ));
        let bits = i % k;
        if bits & 1 != 0 { world.add_component(e, T0); }
        if bits & 2 != 0 { world.add_component(e, T1); }
        if bits & 4 != 0 { world.add_component(e, T2); }
        if bits & 8 != 0 { world.add_component(e, T3); }
        if bits & 16 != 0 { world.add_component(e, T4); }
        if bits & 32 != 0 { world.add_component(e, T5); }
        if bits & 64 != 0 { world.add_component(e, T6); }
        if bits & 128 != 0 { world.add_component(e, T7); }
        if bits & 256 != 0 { world.add_component(e, T8); }
    }
}

fn case_topology(k: usize) -> impl Fn(&BenchCfg) -> Measured {
    move |cfg| {
        let passes = scaled(ITER_PASSES, cfg);
        let mut world = World::new();
        spawn_topology(&mut world, 100_000, k);
        let mut st = <Query<(&mut C0, &C1)>>::init_state(&mut world);
        let t = Timer::start();
        for _ in 0..passes {
            let mut q = unsafe { <Query<(&mut C0, &C1)>>::get_param(&mut st, &world) };
            q.for_each(|(a, b)| a.x += b.x);
            world.current_tick += 1;
        }
        let ms = t.elapsed_ms();
        Measured { ms, checksum: checksum_c0(&mut world) }
    }
}

const LIFECYCLE_N: usize = 200_000;

fn case_spawn_empty(cfg: &BenchCfg) -> Measured {
    let n = scaled(LIFECYCLE_N, cfg);
    let mut world = World::new();
    let t = Timer::start();
    for _ in 0..n {
        world.spawn();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: world.entity_count() as u64 }
}

fn case_spawn_2comp(cfg: &BenchCfg) -> Measured {
    let n = scaled(LIFECYCLE_N, cfg);
    let mut world = World::new();
    world.register::<C0>();
    world.register::<C1>();
    let t = Timer::start();
    for i in 0..n {
        world.spawn_with((
            C0 { x: i as f32, y: 0., z: 0., w: 0. },
            C1 { x: 1., y: 1., z: 1., w: 1. },
        ));
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: world.entity_count() as u64 }
}

fn case_despawn(cfg: &BenchCfg) -> Measured {
    let n = scaled(LIFECYCLE_N, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..n)
        .map(|i| {
            world.spawn_with((
                C0 { x: i as f32, y: 0., z: 0., w: 0. },
                C1 { x: 1., y: 1., z: 1., w: 1. },
            ))
        })
        .collect();
    let t = Timer::start();
    for &e in &ents {
        world.kill(e);
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: world.entity_count() as u64 }
}

const STRUCT_N: usize = 100_000;

fn case_add_component(cfg: &BenchCfg) -> Measured {
    let n = scaled(STRUCT_N, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..n)
        .map(|i| world.spawn_with((C0 { x: i as f32, y: 0., z: 0., w: 0. },)))
        .collect();
    let t = Timer::start();
    for &e in &ents {
        world.add_component(e, Tag);
    }
    let ms = t.elapsed_ms();

    let mut st = <Query<Entity, With<Tag>>>::init_state(&mut world);
    let mut q = unsafe { <Query<Entity, With<Tag>>>::get_param(&mut st, &world) };
    let mut tagged = 0u64;
    q.for_each(|_| tagged += 1);
    Measured { ms, checksum: tagged }
}

fn case_remove_component(cfg: &BenchCfg) -> Measured {
    let n = scaled(STRUCT_N, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..n)
        .map(|i| world.spawn_with((C0 { x: i as f32, y: 0., z: 0., w: 0. }, Tag)))
        .collect();
    let t = Timer::start();
    for &e in &ents {
        world.remove_component::<Tag>(e);
    }
    let ms = t.elapsed_ms();

    let mut st = <Query<Entity, With<Tag>>>::init_state(&mut world);
    let mut q = unsafe { <Query<Entity, With<Tag>>>::get_param(&mut st, &world) };
    let mut tagged = 0u64;
    q.for_each(|_| tagged += 1);
    Measured { ms, checksum: (world.entity_count() as u64) << 32 | tagged }
}

fn case_add_remove_cycle(cfg: &BenchCfg) -> Measured {
    let cycles = scaled(20, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..20_000)
        .map(|i| world.spawn_with((C0 { x: i as f32, y: 0., z: 0., w: 0. },)))
        .collect();
    let t = Timer::start();
    for _ in 0..cycles {
        for &e in &ents {
            world.add_component(e, Tag);
        }
        for &e in &ents {
            world.remove_component::<Tag>(e);
        }
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: world.entity_count() as u64 }
}

const RANDOM_N: usize = 100_000;

fn case_random_get(cfg: &BenchCfg) -> Measured {
    let passes = scaled(10, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..RANDOM_N)
        .map(|i| {
            world.spawn_with((
                C0 { x: i as f32, y: 0., z: 0., w: 0. },
                C1 { x: 1., y: 1., z: 1., w: 1. },
            ))
        })
        .collect();
    let order = shuffled_indices(RANDOM_N);
    let mut acc = 0.0f64;
    let t = Timer::start();
    for _ in 0..passes {
        for &idx in &order {
            if let Some(c) = world.get_component::<C0>(ents[idx]) {
                acc += c.x as f64;
            }
        }
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: f64_sum_bits(acc) }
}

fn case_random_write(cfg: &BenchCfg) -> Measured {
    let passes = scaled(10, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..RANDOM_N)
        .map(|i| {
            world.spawn_with((
                C0 { x: i as f32, y: 0., z: 0., w: 0. },
                C1 { x: 1., y: 1., z: 1., w: 1. },
            ))
        })
        .collect();
    let order = shuffled_indices(RANDOM_N);
    let t = Timer::start();
    for _ in 0..passes {
        for &idx in &order {
            if let Some(c) = world.get_component_mut::<C0>(ents[idx]) {
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
            world.spawn_with((
                C0 { x: i as f32, y: 0., z: 0., w: 0. },
                C1 { x: 1., y: 1., z: 1., w: 1. },
            ))
        })
        .collect();

    let touched: Vec<Entity> = ents.iter().copied().step_by(100).collect();

    let mut st = <Query<&C0, Changed<C0>>>::init_state(&mut world);
    let mut observed = 0u64;
    let t = Timer::start();
    for _ in 0..passes {
        for &e in &touched {
            if let Some(c) = world.get_component_mut::<C0>(e) {
                c.x += 1.0;
            }
        }
        let mut q = unsafe { <Query<&C0, Changed<C0>>>::get_param(&mut st, &world) };
        q.for_each(|_| observed += 1);
        world.current_tick += 1;
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: observed }
}

fn sched_sys_a(mut q: Query<(&mut C0, &C1)>) { q.for_each(|(a, b)| a.x += b.x); }
fn sched_sys_b(mut q: Query<(&mut C1, &C2)>) { q.for_each(|(a, b)| a.y += b.y); }
fn sched_sys_c(mut q: Query<(&mut C2, &C3)>) { q.for_each(|(a, b)| a.z += b.z); }

fn case_schedule_3sys(cfg: &BenchCfg) -> Measured {
    let passes = scaled(20, cfg);
    let mut app = App::new();
    for i in 0..100_000 {
        app.world.spawn_with((
            C0 { x: i as f32, y: 0., z: 0., w: 0. },
            C1 { x: 1., y: 1., z: 1., w: 1. },
            C2 { x: 1., y: 1., z: 1., w: 1. },
            C3 { x: 1., y: 1., z: 1., w: 1. },
        ));
    }
    app.add_system(sched_sys_a);
    app.add_system(sched_sys_b);
    app.add_system(sched_sys_c);
    let t = Timer::start();
    for _ in 0..passes {
        app.update();
    }
    let ms = t.elapsed_ms();
    Measured { ms, checksum: checksum_c0(&mut app.world) }
}

pub struct FairCase {
    pub spec: FairSpec,
    pub f: Box<dyn Fn(&BenchCfg) -> Measured>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FairSpec {
    pub id: &'static str,
    pub group: &'static str,
    pub label: String,

    pub sweep_key: &'static str,
    pub sweep_value: u64,
    pub entity_count: usize,
    pub description: &'static str,
}

const N_SWEEP: [usize; 4] = [1_000, 10_000, 100_000, 1_000_000];

pub fn fair_cases() -> Vec<FairCase> {
    let mut out: Vec<FairCase> = Vec::new();

    for &n in &N_SWEEP {
        out.push(FairCase {
            spec: FairSpec { id: "A1", group: "Iteration", label: format!("write 1 component - {}", fmt_n(n)), sweep_key: "entities", sweep_value: n as u64, entity_count: n, description: "20 passes of read-modify-write over a single component column" },
            f: Box::new(case_iter_write_1(n)),
        });
        out.push(FairCase {
            spec: FairSpec { id: "A2", group: "Iteration", label: format!("read 1 / write 1 - {}", fmt_n(n)), sweep_key: "entities", sweep_value: n as u64, entity_count: n, description: "20 passes of the canonical position/velocity loop" },
            f: Box::new(case_iter_rw_2(n)),
        });
        out.push(FairCase {
            spec: FairSpec { id: "A3", group: "Iteration", label: format!("4 components - {}", fmt_n(n)), sweep_key: "entities", sweep_value: n as u64, entity_count: n, description: "20 passes touching four component columns" },
            f: Box::new(case_iter_rw_4(n)),
        });
        out.push(FairCase {
            spec: FairSpec { id: "A4", group: "Iteration", label: format!("8 components - {}", fmt_n(n)), sweep_key: "entities", sweep_value: n as u64, entity_count: n, description: "20 passes touching eight component columns" },
            f: Box::new(case_iter_rw_8(n)),
        });
        out.push(FairCase {
            spec: FairSpec { id: "A5", group: "Iteration", label: format!("read-only 2 - {}", fmt_n(n)), sweep_key: "entities", sweep_value: n as u64, entity_count: n, description: "20 read-only passes — isolates the cost change tracking adds to writes" },
            f: Box::new(case_iter_read_2(n)),
        });
    }

    for &k in &[1usize, 8, 64, 512] {
        out.push(FairCase {
            spec: FairSpec { id: "B1", group: "Topology", label: format!("{k} archetypes - 100k"), sweep_key: "archetypes", sweep_value: k as u64, entity_count: 100_000, description: "100k entities spread over k archetypes, then iterated" },
            f: Box::new(case_topology(k)),
        });
    }

    out.push(FairCase { spec: FairSpec { id: "C1", group: "Lifecycle", label: "spawn empty - 200k".into(), sweep_key: "entities", sweep_value: 200_000, entity_count: 200_000, description: "identifier allocation with no component data" }, f: Box::new(case_spawn_empty) });
    out.push(FairCase { spec: FairSpec { id: "C2", group: "Lifecycle", label: "spawn 2 components - 200k".into(), sweep_key: "entities", sweep_value: 200_000, entity_count: 200_000, description: "allocation plus archetype placement and column writes" }, f: Box::new(case_spawn_2comp) });
    out.push(FairCase { spec: FairSpec { id: "C3", group: "Lifecycle", label: "despawn - 200k".into(), sweep_key: "entities", sweep_value: 200_000, entity_count: 200_000, description: "removal, row backfill and identifier recycling" }, f: Box::new(case_despawn) });

    out.push(FairCase { spec: FairSpec { id: "D1", group: "Structural", label: "add component - 100k".into(), sweep_key: "entities", sweep_value: 100_000, entity_count: 100_000, description: "one archetype move per entity" }, f: Box::new(case_add_component) });
    out.push(FairCase { spec: FairSpec { id: "D2", group: "Structural", label: "remove component - 100k".into(), sweep_key: "entities", sweep_value: 100_000, entity_count: 100_000, description: "the reverse archetype move" }, f: Box::new(case_remove_component) });
    out.push(FairCase { spec: FairSpec { id: "D3", group: "Structural", label: "add/remove cycle - 20k × 20".into(), sweep_key: "entities", sweep_value: 20_000, entity_count: 20_000, description: "repeated moves, exercising any archetype-transition cache" }, f: Box::new(case_add_remove_cycle) });

    out.push(FairCase { spec: FairSpec { id: "E1", group: "Random access", label: "random get - 100k × 10".into(), sweep_key: "entities", sweep_value: 100_000, entity_count: 100_000, description: "component lookup by entity handle in shuffled order — the case archetype layouts are weakest at" }, f: Box::new(case_random_get) });
    out.push(FairCase { spec: FairSpec { id: "E2", group: "Random access", label: "random write - 100k × 10".into(), sweep_key: "entities", sweep_value: 100_000, entity_count: 100_000, description: "the same lookup, mutating" }, f: Box::new(case_random_write) });

    out.push(FairCase { spec: FairSpec { id: "F1", group: "Change detection", label: "sparse changes - 200k".into(), sweep_key: "entities", sweep_value: 200_000, entity_count: 200_000, description: "1 % of rows mutated per pass, then queried by change filter" }, f: Box::new(case_changed_sparse) });

    out.push(FairCase { spec: FairSpec { id: "G1", group: "Scheduling", label: "3 systems - 100k".into(), sweep_key: "entities", sweep_value: 100_000, entity_count: 100_000, description: "three registered systems over the same data through the engine's scheduler" }, f: Box::new(case_schedule_3sys) });

    out
}

fn fmt_n(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1_000 {
        format!("{}k", n / 1_000)
    } else {
        n.to_string()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FairResult {
    #[serde(flatten)]
    pub spec: FairSpec,
    pub checksum: String,
    pub checksum_stable: bool,
    pub stats: Stats,
}

pub fn run_fair_suite(
    cfg: &BenchCfg,
    filter: Option<&str>,
    mut on_progress: impl FnMut(&FairResult),
) -> Vec<FairResult> {
    let mut out = Vec::new();
    for case in fair_cases() {
        if let Some(f) = filter {
            if case.spec.id != f && case.spec.group != f {
                continue;
            }
        }

        for _ in 0..cfg.warmup {
            std::hint::black_box((case.f)(cfg).checksum);
        }
        let mut samples = Vec::with_capacity(cfg.reps);
        let mut checksum = 0u64;
        let mut stable = true;
        for i in 0..cfg.reps {
            let m = (case.f)(cfg);
            samples.push(m.ms);
            if i == 0 {
                checksum = m.checksum;
            } else if m.checksum != checksum {
                stable = false;
            }
        }

        let r = FairResult {
            spec: case.spec,
            checksum: checksum.to_string(),
            checksum_stable: stable,
            stats: Stats::from_samples(samples),
        };
        on_progress(&r);
        out.push(r);
    }
    out
}

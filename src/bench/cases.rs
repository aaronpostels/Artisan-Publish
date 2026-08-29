use super::harness::*;
use crate::ecs::*;
use crate::engine::App;

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Vel {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub struct Marker;
pub struct MarkerA;
pub struct MarkerB;

macro_rules! define_markers {
    ($($name:ident),*) => { $(pub struct $name;)* };
}
define_markers!(
    M0, M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16, M17, M18, M19, M20,
    M21, M22, M23, M24, M25
);

#[inline]
fn drive<Q, F>(q: &mut Query<'_, Q, F>, parallel: bool, f: impl Fn(Q::Item<'_>) + Send + Sync)
where
    Q: WorldQuery,
    F: QueryFilter,
{
    if parallel {
        q.par_for_each(f);
    } else {
        q.for_each(f);
    }
}

fn checksum_pos_x(world: &mut World) -> u64 {
    let mut state = <Query<&Pos>>::init_state(world);
    let mut q = unsafe { <Query<&Pos>>::get_param(&mut state, world) };
    let mut sum = 0.0f64;
    q.for_each(|p| sum += p.x as f64);
    sum.to_bits()
}

fn spawn_dense(world: &mut World, n: usize) {
    for _ in 0..n {
        world.spawn_with((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
    }
}

fn case_spawn(cfg: &BenchCfg) -> Measured {
    let n = scaled(1_000_000, cfg);
    let mut world = World::new();
    world.register::<Pos>();
    world.register::<Vel>();

    let t = Timer::start();
    for _ in 0..n {
        world.spawn_with((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
    }
    let ms = t.elapsed_ms();

    Measured { ms, checksum: world.entity_count() as u64 }
}

fn case_despawn(cfg: &BenchCfg) -> Measured {
    let n = scaled(1_000_000, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..n).map(|_| world.spawn()).collect();

    let t = Timer::start();
    for e in &ents {
        world.kill(*e);
    }
    let ms = t.elapsed_ms();

    Measured { ms, checksum: world.entity_count() as u64 }
}

fn case_dense_iteration(cfg: &BenchCfg) -> Measured {
    let n = 1_000_000;
    let iters = scaled(100, cfg);
    let mut world = World::new();
    spawn_dense(&mut world, n);

    let mut state = <Query<(&mut Pos, &Vel)>>::init_state(&mut world);
    let t = Timer::start();
    for _ in 0..iters {
        let mut q = unsafe { <Query<(&mut Pos, &Vel)>>::get_param(&mut state, &world) };
        drive(&mut q, cfg.parallel, |(p, v)| p.x += v.x);
        world.current_tick += 1;
    }
    let ms = t.elapsed_ms();

    Measured { ms, checksum: checksum_pos_x(&mut world) }
}

fn case_fragmented_iteration(cfg: &BenchCfg) -> Measured {
    let n = 100_000;
    let iters = scaled(100, cfg);
    let mut world = World::new();
    for i in 0..n {
        let e = world.spawn_with((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
        add_marker_by_index(&mut world, e, i % 26);
    }

    let mut state = <Query<(&mut Pos, &Vel)>>::init_state(&mut world);
    let t = Timer::start();
    for _ in 0..iters {
        let mut q = unsafe { <Query<(&mut Pos, &Vel)>>::get_param(&mut state, &world) };
        drive(&mut q, cfg.parallel, |(p, v)| p.x += v.x);
        world.current_tick += 1;
    }
    let ms = t.elapsed_ms();

    Measured { ms, checksum: checksum_pos_x(&mut world) }
}

fn add_marker_by_index(world: &mut World, e: Entity, idx: usize) {
    match idx {
        0 => world.add_component(e, M0),
        1 => world.add_component(e, M1),
        2 => world.add_component(e, M2),
        3 => world.add_component(e, M3),
        4 => world.add_component(e, M4),
        5 => world.add_component(e, M5),
        6 => world.add_component(e, M6),
        7 => world.add_component(e, M7),
        8 => world.add_component(e, M8),
        9 => world.add_component(e, M9),
        10 => world.add_component(e, M10),
        11 => world.add_component(e, M11),
        12 => world.add_component(e, M12),
        13 => world.add_component(e, M13),
        14 => world.add_component(e, M14),
        15 => world.add_component(e, M15),
        16 => world.add_component(e, M16),
        17 => world.add_component(e, M17),
        18 => world.add_component(e, M18),
        19 => world.add_component(e, M19),
        20 => world.add_component(e, M20),
        21 => world.add_component(e, M21),
        22 => world.add_component(e, M22),
        23 => world.add_component(e, M23),
        24 => world.add_component(e, M24),
        _ => world.add_component(e, M25),
    }
}

fn case_add_remove_churn(cfg: &BenchCfg) -> Measured {
    let n = 100_000;
    let iters = scaled(100, cfg);
    let mut world = World::new();
    let ents: Vec<Entity> = (0..n).map(|_| world.spawn()).collect();

    let t = Timer::start();
    for _ in 0..iters {
        for &e in &ents {
            world.add_component(e, Marker);
        }
        for &e in &ents {
            world.remove_component::<Marker>(e);
        }
    }
    let ms = t.elapsed_ms();

    let mut state = <Query<Entity, With<Marker>>>::init_state(&mut world);
    let mut q = unsafe { <Query<Entity, With<Marker>>>::get_param(&mut state, &world) };
    let mut remaining = 0u64;
    q.for_each(|_| remaining += 1);

    Measured { ms, checksum: (world.entity_count() as u64) << 32 | remaining }
}

type FilteredQuery<'a> = Query<'a, (&'a mut Pos, &'a Vel), (With<MarkerA>, Without<MarkerB>)>;

fn case_query_filtering(cfg: &BenchCfg) -> Measured {
    let n = 200_000;
    let iters = scaled(100, cfg);
    let mut world = World::new();
    for i in 0..n {
        let e = world.spawn_with((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
        if i % 2 == 0 {
            world.add_component(e, MarkerA);
        } else {
            world.add_component(e, MarkerB);
        }
    }

    let mut state = <FilteredQuery>::init_state(&mut world);
    let t = Timer::start();
    for _ in 0..iters {
        let mut q = unsafe { <FilteredQuery>::get_param(&mut state, &world) };
        drive(&mut q, cfg.parallel, |(p, v)| p.x += v.x);
        world.current_tick += 1;
    }
    let ms = t.elapsed_ms();

    Measured { ms, checksum: checksum_pos_x(&mut world) }
}

fn case_mixed_density(cfg: &BenchCfg) -> Measured {
    let iters = scaled(100, cfg);
    let mut world = World::new();

    for i in 0..255u32 {
        let e = world.spawn_with((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
        add_marker_bits(&mut world, e, i);
    }

    for _ in 0..100_000 {
        let e = world.spawn_with((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
        add_marker_bits(&mut world, e, 0xFF);
    }

    let mut state = <Query<(&mut Pos, &Vel)>>::init_state(&mut world);
    let t = Timer::start();
    for _ in 0..iters {
        let mut q = unsafe { <Query<(&mut Pos, &Vel)>>::get_param(&mut state, &world) };
        drive(&mut q, cfg.parallel, |(p, v)| p.x += v.x);
        world.current_tick += 1;
    }
    let ms = t.elapsed_ms();

    Measured { ms, checksum: checksum_pos_x(&mut world) }
}

fn add_marker_bits(world: &mut World, e: Entity, bits: u32) {
    if bits & 1 != 0 { world.add_component(e, M0); }
    if bits & 2 != 0 { world.add_component(e, M1); }
    if bits & 4 != 0 { world.add_component(e, M2); }
    if bits & 8 != 0 { world.add_component(e, M3); }
    if bits & 16 != 0 { world.add_component(e, M4); }
    if bits & 32 != 0 { world.add_component(e, M5); }
    if bits & 64 != 0 { world.add_component(e, M6); }
    if bits & 128 != 0 { world.add_component(e, M7); }
}

fn sys1_par(mut q: Query<(&mut Pos, &Vel)>) { q.par_for_each(|(p, v)| p.x += v.x); }
fn sys2_par(mut q: Query<(&mut Pos, &Vel)>) { q.par_for_each(|(p, v)| p.y += v.y); }
fn sys3_par(mut q: Query<(&mut Pos, &Vel)>) { q.par_for_each(|(p, v)| p.z += v.z); }
fn sys1_ser(mut q: Query<(&mut Pos, &Vel)>) { q.for_each(|(p, v)| p.x += v.x); }
fn sys2_ser(mut q: Query<(&mut Pos, &Vel)>) { q.for_each(|(p, v)| p.y += v.y); }
fn sys3_ser(mut q: Query<(&mut Pos, &Vel)>) { q.for_each(|(p, v)| p.z += v.z); }

fn case_scheduling(cfg: &BenchCfg) -> Measured {
    let n = 100_000;
    let iters = scaled(100, cfg);
    let mut app = App::new();
    spawn_dense(&mut app.world, n);

    if cfg.parallel {
        app.add_system(sys1_par);
        app.add_system(sys2_par);
        app.add_system(sys3_par);
    } else {
        app.add_system(sys1_ser);
        app.add_system(sys2_ser);
        app.add_system(sys3_ser);
    }

    let t = Timer::start();
    for _ in 0..iters {
        app.update();
    }
    let ms = t.elapsed_ms();

    Measured { ms, checksum: checksum_pos_x(&mut app.world) }
}

fn sys_add(mut cmds: Commands, mut q: Query<Entity, Without<Marker>>) {
    q.for_each(|e| cmds.insert(e, Marker));
}
fn sys_remove(mut cmds: Commands, mut q: Query<Entity, With<Marker>>) {
    q.for_each(|e| cmds.remove::<Marker>(e));
}

fn case_system_churn(cfg: &BenchCfg) -> Measured {
    let n = 100_000;
    let iters = scaled(100, cfg);
    let mut app = App::new();
    for _ in 0..n {
        app.world.spawn();
    }
    app.add_system(sys_add);
    app.add_system(sys_remove);

    let t = Timer::start();
    for _ in 0..iters {
        app.update();
    }
    let ms = t.elapsed_ms();

    Measured { ms, checksum: app.world.entity_count() as u64 }
}

fn case_sparse_change_stress(cfg: &BenchCfg) -> Measured {
    let n = 1_000_000;
    let iters = scaled(100, cfg);
    let mut world = World::new();
    let mut touched = Vec::new();
    for i in 0..n {
        let e = world.spawn_with((Pos { x: 0., y: 0., z: 0. }, Vel { x: 1., y: 1., z: 1. }));
        let arch_idx = (i % 100) as u32;
        add_marker_bits(&mut world, e, arch_idx & 0x7F);
        if arch_idx == 0 && touched.len() < 1000 {
            touched.push(e);
        }
    }

    let mut state = <Query<&Pos, Changed<Pos>>>::init_state(&mut world);
    let mut observed = 0u64;
    let t = Timer::start();
    for _ in 0..iters {
        for &e in &touched {
            if let Some(p) = world.get_component_mut::<Pos>(e) {
                p.x += 1.0;
            }
        }
        let mut q = unsafe { <Query<&Pos, Changed<Pos>>>::get_param(&mut state, &world) };
        q.for_each(|p| observed += p.x as u64);
        world.current_tick += 1;
    }
    let ms = t.elapsed_ms();

    Measured { ms, checksum: observed }
}

fn case_bulk_structural_churn(cfg: &BenchCfg) -> Measured {
    let rounds = scaled(100, cfg);
    let per_round = 10_000;
    let mut world = World::new();
    world.register::<Pos>();
    world.register::<Vel>();
    world.register::<Marker>();

    let t = Timer::start();
    for _ in 0..rounds {
        let mut ents = Vec::with_capacity(per_round);
        for _ in 0..per_round {
            ents.push(world.spawn_with((
                Pos { x: 0., y: 0., z: 0. },
                Vel { x: 1., y: 1., z: 1. },
                Marker,
            )));
        }
        for &e in &ents {
            world.remove_component::<Marker>(e);
        }
        for &e in &ents {
            world.kill(e);
        }
    }
    let ms = t.elapsed_ms();

    Measured { ms, checksum: world.entity_count() as u64 }
}

pub struct Case {
    pub spec: BenchSpec,
    pub f: CaseFn,
}

pub fn core_cases() -> Vec<Case> {
    vec![
        Case {
            spec: BenchSpec {
                id: 1,
                name: "1. Entity Spawn",
                entity_count: 1_000_000,
                inner_iters: 1,
                group: Group::Core,
                subquestion: SubQuestion::NativeComparison,
                measures: "1M individual spawn_with((Pos, Vel)) calls incl. archetype lookup and column growth",
                excludes: "World construction and component type registration",
                checksum_of: "live entity count after the measured loop",
            },
            f: case_spawn,
        },
        Case {
            spec: BenchSpec {
                id: 2,
                name: "2. Entity Despawn",
                entity_count: 1_000_000,
                inner_iters: 1,
                group: Group::Core,
                subquestion: SubQuestion::NativeComparison,
                measures: "1M individual kill() calls incl. swap-remove and free-list handling",
                excludes: "spawning the 1M entities and collecting their handles",
                checksum_of: "live entity count after the measured loop (must be 0)",
            },
            f: case_despawn,
        },
        Case {
            spec: BenchSpec {
                id: 3,
                name: "3. Dense Iteration",
                entity_count: 1_000_000,
                inner_iters: 100,
                group: Group::Core,
                subquestion: SubQuestion::EcsProperties,
                measures: "100 passes of (&mut Pos, &Vel) over one dense archetype, incl. per-pass query retrieval and tick increment",
                excludes: "spawning the 1M entities and building the initial query state",
                checksum_of: "sum of all Pos.x as f64 bits (expected 100.0 per entity)",
            },
            f: case_dense_iteration,
        },
        Case {
            spec: BenchSpec {
                id: 4,
                name: "4. Fragmented Iteration",
                entity_count: 100_000,
                inner_iters: 100,
                group: Group::Core,
                subquestion: SubQuestion::EcsProperties,
                measures: "100 passes of (&mut Pos, &Vel) spread over 26 archetypes",
                excludes: "spawning and distributing the entities over the marker components",
                checksum_of: "sum of all Pos.x as f64 bits",
            },
            f: case_fragmented_iteration,
        },
        Case {
            spec: BenchSpec {
                id: 5,
                name: "5. Add/Remove Churn",
                entity_count: 100_000,
                inner_iters: 100,
                group: Group::Core,
                subquestion: SubQuestion::EcsProperties,
                measures: "100 rounds of adding and removing one marker on 100k entities (200k archetype moves per round)",
                excludes: "spawning the entities",
                checksum_of: "live entity count and remaining marker matches (must be 0)",
            },
            f: case_add_remove_churn,
        },
        Case {
            spec: BenchSpec {
                id: 6,
                name: "6. Query Filtering",
                entity_count: 200_000,
                inner_iters: 100,
                group: Group::Core,
                subquestion: SubQuestion::EcsProperties,
                measures: "100 passes of (&mut Pos, &Vel) filtered by (With<A>, Without<B>) — half the entities match",
                excludes: "spawning and marking the entities",
                checksum_of: "sum of all Pos.x as f64 bits (only matching rows advance)",
            },
            f: case_query_filtering,
        },
        Case {
            spec: BenchSpec {
                id: 7,
                name: "7. Mixed Density",
                entity_count: 100_255,
                inner_iters: 100,
                group: Group::Core,
                subquestion: SubQuestion::EcsProperties,
                measures: "100 passes over 255 single-entity archetypes plus one archetype holding 100k entities",
                excludes: "building the 256 archetype combinations",
                checksum_of: "sum of all Pos.x as f64 bits",
            },
            f: case_mixed_density,
        },
        Case {
            spec: BenchSpec {
                id: 8,
                name: "8. Scheduling",
                entity_count: 100_000,
                inner_iters: 100,
                group: Group::Core,
                subquestion: SubQuestion::NativeComparison,
                measures: "100 full App::update() calls running three systems that write different Pos fields",
                excludes: "App construction, entity spawning and system registration",
                checksum_of: "sum of all Pos.x as f64 bits",
            },
            f: case_scheduling,
        },
        Case {
            spec: BenchSpec {
                id: 9,
                name: "9. System Churn",
                entity_count: 100_000,
                inner_iters: 100,
                group: Group::Core,
                subquestion: SubQuestion::EcsProperties,
                measures: "100 updates of two systems adding and removing a marker through deferred Commands",
                excludes: "App construction, entity spawning and system registration",
                checksum_of: "live entity count",
            },
            f: case_system_churn,
        },
        Case {
            spec: BenchSpec {
                id: 10,
                name: "10. Sparse Change Stress",
                entity_count: 1_000_000,
                inner_iters: 100,
                group: Group::Core,
                subquestion: SubQuestion::EcsProperties,
                measures: "100 rounds of mutating up to 1000 rows through the tracked write path, then querying Changed<Pos> across 100 archetypes",
                excludes: "spawning and distributing 1M entities over 100 component combinations",
                checksum_of: "accumulated values observed through Changed<Pos>",
            },
            f: case_sparse_change_stress,
        },
        Case {
            spec: BenchSpec {
                id: 11,
                name: "11. Bulk Structural Churn",
                entity_count: 1_000_000,
                inner_iters: 100,
                group: Group::Core,
                subquestion: SubQuestion::NativeComparison,
                measures: "100 rounds of spawning 10k entities, removing a marker from each and despawning them — 1M entities in total",
                excludes: "World construction and component type registration",
                checksum_of: "live entity count after the measured loop (must be 0)",
            },
            f: case_bulk_structural_churn,
        },
    ]
}

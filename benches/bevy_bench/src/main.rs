use bevy::prelude::*;
use std::time::Instant;
use std::io::{self, Write};
use std::hint::black_box;
use serde::Serialize;

#[derive(Serialize)]
struct BenchResult {
    name: String,
    entity_count: usize,
    iterations: usize,
    min_time_ms: f64,
}

#[derive(Serialize)]
struct Output {
    framework: String,
    benchmarks: Vec<BenchResult>,
}

#[derive(Component, Clone, Copy)] #[repr(C)] struct Pos { x: f32, y: f32, z: f32 }
#[derive(Component, Clone, Copy)] #[repr(C)] struct Vel { x: f32, y: f32, z: f32 }
#[derive(Component)] struct Marker;
#[derive(Component)] struct MarkerA;
#[derive(Component)] struct MarkerB;

#[derive(Message)] struct Ev(usize);

macro_rules! define_markers {
    ($($name:ident),*) => { $(#[allow(dead_code)] #[derive(Component)] struct $name;)* };
}
define_markers!(M0, M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12, M13, M14, M15, M16, M17, M18, M19, M20, M21, M22, M23, M24, M25);

fn main() {
    bevy::tasks::ComputeTaskPool::get_or_init(|| bevy::tasks::TaskPoolBuilder::new().build());
    let args: Vec<String> = std::env::args().collect();
    let selected: Vec<usize> = args.iter().filter_map(|s| s.parse().ok()).collect();
    let is_active = |id: usize| selected.is_empty() || selected.contains(&id);

    println!("\n=== BEVY BENCHMARK RUNNER ===");
    let iterations = 10;
    let mut results = Vec::new();

    let record = |name: &str, count: usize, iters: usize, min_time: f64| -> BenchResult {
        BenchResult { name: name.to_string(), entity_count: count, iterations: iters, min_time_ms: min_time }
    };

    if is_active(1) {
        print!("  -> Running 1. Entity Spawn... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            let start = Instant::now();
            for _ in 0..1_000_000 { world.spawn((Pos{x:0.,y:0.,z:0.}, Vel{x:1.,y:1.,z:1.})); }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("1. Entity Spawn", 1_000_000, iterations, min_t));
    }

    if is_active(2) {
        print!("  -> Running 2. Entity Despawn... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            let ents: Vec<Entity> = (0..1_000_000).map(|_| world.spawn_empty().id()).collect();
            let start = Instant::now();
            for e in ents { world.despawn(e); }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("2. Entity Despawn", 1_000_000, iterations, min_t));
    }

    if is_active(3) {
        print!("  -> Running 3. Dense Iteration... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            world.spawn_batch((0..1_000_000).map(|_| (Pos{x:0.,y:0.,z:0.}, Vel{x:1.,y:1.,z:1.})));
            let mut query = world.query::<(&mut Pos, &Vel)>();
            let start = Instant::now();
            for _ in 0..100 {
                query.par_iter_mut(&mut world).for_each(|(mut p, v)| { p.x += v.x; });
            }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("3. Dense Iteration", 1_000_000, iterations, min_t));
    }

    if is_active(4) {
        print!("  -> Running 4. Fragmented Iteration... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            for i in 0..100_000 {
                let mut e = world.spawn((Pos{x:0.,y:0.,z:0.}, Vel{x:1.,y:1.,z:1.}));
                match i % 26 {
                    0 => { e.insert(M0); }, 1 => { e.insert(M1); }, 2 => { e.insert(M2); }, 3 => { e.insert(M3); },
                    4 => { e.insert(M4); }, 5 => { e.insert(M5); }, 6 => { e.insert(M6); }, 7 => { e.insert(M7); },
                    8 => { e.insert(M8); }, 9 => { e.insert(M9); }, 10=> { e.insert(M10);}, 11=> { e.insert(M11);},
                    12=> { e.insert(M12);}, 13=> { e.insert(M13);}, 14=> { e.insert(M14);}, 15=> { e.insert(M15);},
                    16=> { e.insert(M16);}, 17=> { e.insert(M17);}, 18=> { e.insert(M18);}, 19=> { e.insert(M19);},
                    20=> { e.insert(M20);}, 21=> { e.insert(M21);}, 22=> { e.insert(M22);}, 23=> { e.insert(M23);},
                    24=> { e.insert(M24);}, _ => { e.insert(M25);},
                }
            }
            let mut query = world.query::<(&mut Pos, &Vel)>();
            let start = Instant::now();
            for _ in 0..100 {
                query.par_iter_mut(&mut world).for_each(|(mut p, v)| { p.x += v.x; });
            }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("4. Fragmented Iteration", 100_000, iterations, min_t));
    }

    if is_active(5) {
        print!("  -> Running 5. Add/Remove Churn... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            let ents: Vec<Entity> = (0..100_000).map(|_| world.spawn_empty().id()).collect();
            let start = Instant::now();
            for _ in 0..100 {
                for &e in &ents { world.entity_mut(e).insert(Marker); }
                for &e in &ents { world.entity_mut(e).remove::<Marker>(); }
            }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("5. Add/Remove Churn", 100_000, iterations, min_t));
    }

    if is_active(6) {
        print!("  -> Running 6. Query Filtering... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            for i in 0..200_000 {
                let mut e = world.spawn((Pos{x:0.,y:0.,z:0.}, Vel{x:1.,y:1.,z:1.}));
                if i % 2 == 0 { e.insert(MarkerA); } else { e.insert(MarkerB); }
            }
            let mut query = world.query_filtered::<(&mut Pos, &Vel), (With<MarkerA>, Without<MarkerB>)>();
            let start = Instant::now();
            for _ in 0..100 {
                query.par_iter_mut(&mut world).for_each(|(mut p, v)| { p.x += v.x; });
            }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("6. Query Filtering", 200_000, iterations, min_t));
    }

    if is_active(7) {
        print!("  -> Running 7. Mixed Density... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            for i in 0..255 {
                let mut e = world.spawn((Pos{x:0.,y:0.,z:0.}, Vel{x:1.,y:1.,z:1.}));
                if (i & 1) != 0 { e.insert(M0); } if (i & 2) != 0 { e.insert(M1); }
                if (i & 4) != 0 { e.insert(M2); } if (i & 8) != 0 { e.insert(M3); }
                if (i & 16) != 0 { e.insert(M4); } if (i & 32) != 0 { e.insert(M5); }
                if (i & 64) != 0 { e.insert(M6); } if (i & 128) != 0 { e.insert(M7); }
            }
            for _ in 0..100_000 {
                let mut e = world.spawn((Pos{x:0.,y:0.,z:0.}, Vel{x:1.,y:1.,z:1.}));
                e.insert((M0, M1, M2, M3, M4, M5, M6, M7));
            }
            let mut query = world.query::<(&mut Pos, &Vel)>();
            let start = Instant::now();
            for _ in 0..100 {
                query.par_iter_mut(&mut world).for_each(|(mut p, v)| { p.x += v.x; });
            }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("7. Mixed Density", 100_255, iterations, min_t));
    }

    if is_active(8) {
        print!("  -> Running 8. Scheduling... "); io::stdout().flush().unwrap();
        fn sys1(mut q: Query<(&mut Pos, &Vel)>) { q.par_iter_mut().for_each(|(mut p, v)| p.x += v.x); }
        fn sys2(mut q: Query<(&mut Pos, &Vel)>) { q.par_iter_mut().for_each(|(mut p, v)| p.y += v.y); }
        fn sys3(mut q: Query<(&mut Pos, &Vel)>) { q.par_iter_mut().for_each(|(mut p, v)| p.z += v.z); }
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            world.spawn_batch((0..100_000).map(|_| (Pos{x:0.,y:0.,z:0.}, Vel{x:1.,y:1.,z:1.})));
            let mut schedule = Schedule::default();
            schedule.add_systems((sys1, sys2, sys3));
            let start = Instant::now();
            for _ in 0..100 { schedule.run(&mut world); }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("8. Scheduling", 100_000, iterations, min_t));
    }

    if is_active(9) {
        print!("  -> Running 9. System Churn... "); io::stdout().flush().unwrap();
        fn sys_add(mut cmds: Commands, q: Query<Entity, Without<Marker>>) {
            q.iter().for_each(|e| { cmds.entity(e).insert(Marker); });
        }
        fn sys_remove(mut cmds: Commands, q: Query<Entity, With<Marker>>) {
            q.iter().for_each(|e| { cmds.entity(e).remove::<Marker>(); });
        }
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            world.spawn_batch((0..100_000).map(|_| ()));
            let mut schedule = Schedule::default();
            schedule.add_systems((sys_add, sys_remove));
            let start = Instant::now();
            for _ in 0..100 { schedule.run(&mut world); }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("9. System Churn", 100_000, iterations, min_t));
    }

    if is_active(10) {
        print!("  -> Running 10. Sparse Change Stress... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            let mut ents = Vec::new();
            for i in 0..1_000_000 {
                let mut e = world.spawn((Pos{x:0.,y:0.,z:0.}, Vel{x:1.,y:1.,z:1.}));
                let arch_idx = i % 100;
                if (arch_idx & 1) != 0 { e.insert(M0); }
                if (arch_idx & 2) != 0 { e.insert(M1); }
                if (arch_idx & 4) != 0 { e.insert(M2); }
                if (arch_idx & 8) != 0 { e.insert(M3); }
                if (arch_idx & 16) != 0 { e.insert(M4); }
                if (arch_idx & 32) != 0 { e.insert(M5); }
                if (arch_idx & 64) != 0 { e.insert(M6); }
                if arch_idx == 0 && ents.len() < 1000 { ents.push(e.id()); }
            }
            let mut query_mutate = world.query::<&mut Pos>();
            let mut query_changed = world.query_filtered::<&Pos, Changed<Pos>>();

            let start = Instant::now();
            for _ in 0..100 {
                for &e in &ents {
                    if let Ok(mut p) = query_mutate.get_mut(&mut world, e) { p.x += 1.0; }
                }
                let mut count = 0;
                for p in query_changed.iter(&world) { count += p.x as usize; }
                black_box(count);
                world.clear_trackers();
            }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("10. Sparse Change Stress", 1_000_000, iterations, min_t));
    }

    if is_active(11) {
        print!("  -> Running 11. Bulk Structural Churn... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            let start = Instant::now();
            for _ in 0..100 {
                let ents = world.spawn_batch((0..10_000).map(|_| (Pos{x:0.,y:0.,z:0.}, Vel{x:1.,y:1.,z:1.}, Marker))).collect::<Vec<_>>();
                for &e in &ents { world.entity_mut(e).remove::<Marker>(); }
                for &e in &ents { world.despawn(e); }
            }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("11. Bulk Structural Churn", 100_000, iterations, min_t));
    }

    if is_active(12) {
        print!("  -> Running 12. Message Throughput... "); io::stdout().flush().unwrap();
        fn w(mut mw: MessageWriter<Ev>) { for i in 0..10_000 { mw.write(Ev(i)); } }
        fn r0(mut mr: MessageReader<Ev>) { for e in mr.read() { black_box(e.0); } }
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_message::<Ev>();
            app.add_systems(Update, (w, r0, r0, r0, r0, r0, r0, r0, r0, r0, r0));
            let start = Instant::now();
            for _ in 0..100 { app.update(); }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("12. Event Throughput", 10_000, iterations, min_t));
    }

    if is_active(13) {
        print!("  -> Running 13. Hierarchy Despawn... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            let mut roots = Vec::new();
            for _ in 0..1000 {
                let root = world.spawn_empty()
                    .with_children(|p1| {
                        for _ in 0..10 {
                            p1.spawn_empty().with_children(|p2| {
                                for _ in 0..10 { p2.spawn_empty(); }
                            });
                        }
                    }).id();
                roots.push(root);
            }
            let start = Instant::now();
            for r in roots { world.despawn(r); }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("13. Hierarchy Despawn", 111_000, iterations, min_t));
    }

    if is_active(14) {
        print!("  -> Running 14. Hierarchy Stress Test... "); io::stdout().flush().unwrap();
        #[derive(Component)] struct CustomTransform { m: [f32; 16] }
        #[derive(Component)] struct CustomGlobalTransform { m: [f32; 16] }
        fn transform_propagate_system(mut q: Query<(&CustomTransform, &mut CustomGlobalTransform, Option<&ChildOf>), Changed<CustomTransform>>) {
            q.par_iter_mut().for_each(|(t, mut gt, _p)| { gt.m = t.m; });
        }
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(bevy::transform::TransformPlugin);
            let mut roots = Vec::new();
            for _ in 0..1_000 {
                let root = app.world_mut().spawn((CustomTransform{m:[1.0;16]}, CustomGlobalTransform{m:[1.0;16]}))
                    .with_children(|p1| { for _ in 0..10 { p1.spawn((CustomTransform{m:[1.0;16]}, CustomGlobalTransform{m:[1.0;16]})); } })
                    .id();
                roots.push(root);
            }
            app.add_systems(Update, transform_propagate_system);
            let start = Instant::now();
            for _ in 0..100 {
                for &r in &roots { if let Some(mut t) = app.world_mut().get_mut::<CustomTransform>(r) { t.m[0] += 1.0; } }
                app.update();
            }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("14. Hierarchy Stress", 11_000, iterations, min_t));
    }

    if is_active(15) {
        print!("  -> Running 15. State Switching Stress... "); io::stdout().flush().unwrap();
        #[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)] enum GState { #[default] A, B }
        fn dummy_sys(_q: Query<Entity>) {}
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(bevy::state::app::StatesPlugin);
            app.init_state::<GState>();
            for _ in 0..50 {
                app.add_systems(Update, dummy_sys.run_if(in_state(GState::A)));
                app.add_systems(Update, dummy_sys.run_if(in_state(GState::B)));
            }
            let start = Instant::now();
            for _ in 0..1000 {
                app.world_mut().resource_mut::<NextState<GState>>().set(GState::B);
                app.update();
                app.world_mut().resource_mut::<NextState<GState>>().set(GState::A);
                app.update();
            }
            let d = start.elapsed().as_secs_f64() * 1000.0;
            if d < min_t { min_t = d; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("15. State Switching", 100, iterations, min_t));
    }

    if is_active(16) {
        print!("  -> Running 16. Fixed Catch-up... "); io::stdout().flush().unwrap();
        fn move_fixed(mut q: Query<(&mut Pos, &Vel)>) { q.par_iter_mut().for_each(|(mut p, v)| p.x += v.x); }
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(100)));
            for _ in 0..10_000 { app.world_mut().spawn((Pos{x:0.,y:0.,z:0.}, Vel{x:1.,y:1.,z:1.})); }
            app.add_systems(FixedUpdate, move_fixed);
            let start = Instant::now();
            for _ in 0..1000 { app.update(); }
            let d = start.elapsed().as_secs_f64() * 1000.0;
            if d < min_t { min_t = d; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("16. Fixed Catch-up", 10_000, iterations, min_t));
    }

    if is_active(17) {
        print!("  -> Running 17. 2D Transform Propagation... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(bevy::transform::TransformPlugin);
            let mut roots = Vec::new();
            for _ in 0..10_000 {
                let root = app.world_mut().spawn((Transform::default(), GlobalTransform::default()))
                    .with_children(|p1| { for _ in 0..10 { p1.spawn((Transform::default(), GlobalTransform::default())); } })
                    .id();
                roots.push(root);
            }
            let start = Instant::now();
            for _ in 0..100 {
                for &r in &roots { if let Some(mut t) = app.world_mut().get_mut::<Transform>(r) { t.translation.x += 1.0; } }
                app.update();
            }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("17. 2D Transform Propagation", 110_000, iterations, min_t));
    }

    if is_active(18) {
        print!("  -> Running 18. World Serialization... "); io::stdout().flush().unwrap();
        #[derive(Component, Clone, Copy, Default, bevy::reflect::Reflect)] #[reflect(Component)] struct SerPos { x: f32, y: f32, z: f32 }
        #[derive(Component, Clone, Copy, Default, bevy::reflect::Reflect)] #[reflect(Component)] struct SerVel { x: f32, y: f32, z: f32 }
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins((bevy::asset::AssetPlugin::default(), bevy::scene::ScenePlugin));
            app.init_resource::<AppTypeRegistry>();
            { let registry = app.world().resource::<AppTypeRegistry>(); let mut registry = registry.write(); registry.register::<SerPos>(); registry.register::<SerVel>(); }
            let mut ents = Vec::new();
            for _ in 0..50_000 { ents.push(app.world_mut().spawn((SerPos{x:0.,y:0.,z:0.}, SerVel{x:1.,y:1.,z:1.})).id()); }
            let start = Instant::now();
            let type_registry = app.world().resource::<AppTypeRegistry>().clone();
            let scene = bevy::scene::DynamicSceneBuilder::from_world(app.world()).extract_entities(ents.into_iter()).build();
            let type_registry_read = type_registry.read();
            let serializer = bevy::scene::serde::SceneSerializer::new(&scene, &type_registry_read);
            let json = serde_json::to_string(&serializer).unwrap();
            let deserializer = bevy::scene::serde::SceneDeserializer { type_registry: &type_registry_read };
            let mut json_deserializer = serde_json::Deserializer::from_str(&json);
            let deserialized_scene = serde::de::DeserializeSeed::deserialize(deserializer, &mut json_deserializer).unwrap();
            let mut app2 = App::new();
            app2.add_plugins(MinimalPlugins); app2.add_plugins((bevy::asset::AssetPlugin::default(), bevy::scene::ScenePlugin));
            app2.init_resource::<AppTypeRegistry>();
            { let registry = app2.world().resource::<AppTypeRegistry>(); let mut registry = registry.write(); registry.register::<SerPos>(); registry.register::<SerVel>(); }
            let mut entity_map = bevy::ecs::entity::EntityHashMap::default();
            deserialized_scene.write_to_world_with(app2.world_mut(), &mut entity_map, &type_registry).unwrap();
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("18. World Serialization", 50_000, iterations, min_t));
    }

    if is_active(19) {
        print!("  -> Running 19. Grid Lookup... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        #[derive(Resource)] struct TileGrid { cells: std::collections::HashMap<(i32, i32), Entity> }
        for _ in 0..iterations {
            let mut world = World::new();
            let mut grid = TileGrid { cells: std::collections::HashMap::default() };
            for x in 0..1000 { for y in 0..1000 { let e = world.spawn_empty().id(); grid.cells.insert((x, y), e); } }
            world.insert_resource(grid);
            let start = Instant::now();
            for _ in 0..100 { let grid_res = world.resource::<TileGrid>(); for x in 0..1000 { black_box(grid_res.cells.get(&(x, 500))); } }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("19. Grid Lookup", 1_000_000, iterations, min_t));
    }

    if is_active(20) {
        print!("  -> Running 20. 3D Transform Propagation... "); io::stdout().flush().unwrap();
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(bevy::transform::TransformPlugin);
            let mut roots = Vec::new();
            for _ in 0..10_000 {
                let root = app.world_mut().spawn((Transform::default(), GlobalTransform::default()))
                    .with_children(|p1| { for _ in 0..10 { p1.spawn((Transform::default(), GlobalTransform::default())); } })
                    .id();
                roots.push(root);
            }
            let start = Instant::now();
            for _ in 0..100 {
                for &r in &roots { if let Some(mut t) = app.world_mut().get_mut::<Transform>(r) { t.rotation *= bevy::math::Quat::from_rotation_y(0.01); } }
                app.update();
            }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("20. 3D Transform Propagation", 110_000, iterations, min_t));
    }

    if is_active(21) {
        print!("  -> Running 21. Vivarium Simulation Step... "); io::stdout().flush().unwrap();
        #[allow(dead_code)]
        #[derive(Component, Clone, Copy, Default)]
        struct BenchTransform {
            pub translation: [f32; 3],
            pub rotation: [f32; 4],
            pub scale: [f32; 3],
        }
        #[allow(dead_code)]
        #[derive(Component, Clone, Copy, Default)]
        struct BenchMaterial {
            pub base_color: [f32; 4],
            pub emissive: [f32; 3],
            pub metallic: f32,
            pub roughness: f32,
            pub pad: [f32; 3],
        }
        #[allow(dead_code)]
        #[derive(Component, Clone, Copy, Default)]
        struct BenchSettlement {
            pub id: f32,
            pub face_index: f32,
            pub faction_id: f32,
            pub population: f32,
            pub infrastructure: f32,
            pub wealth: f32,
            pub name_seed: f32,
            pub is_capital: f32,
        }
        #[allow(dead_code)]
        #[derive(Component, Clone, Default)]
        struct BenchMesh {
            pub vertices: Vec<f32>,
            pub indices: Vec<u32>,
            pub version: u32,
        }
        #[allow(dead_code)]
        #[derive(Component, Clone, Default)]
        struct BenchSimulationState {
            pub seed_value: u32,
            pub face_owner: Vec<i32>,
            pub face_score: Vec<f32>,
            pub faction_colors: Vec<u32>,
            pub faction_tech: Vec<f32>,
            pub step_counter: u32,
            pub year_value: u32,
            pub neighbors_flat: Vec<u32>,
            pub neighbors_offsets: Vec<u32>,
            pub base_colors: Vec<f32>,
            pub run_simulation: f32,
            pub num_colonies: f32,
            pub is_water: Vec<f32>,
            pub arability: Vec<f32>,
            pub minerals: Vec<f32>,
            pub temps: Vec<f32>,
            pub moistures: Vec<f32>,
            pub elevations: Vec<f32>,
            pub face_centers: Vec<f32>,
        }
        fn vivarium_step_system(
            mut planet_q: Query<(&mut BenchSimulationState, &BenchMesh)>,
            mut s_q: Query<(&mut BenchSettlement, &mut BenchTransform, &mut BenchMaterial)>,
        ) {
            for (mut state, mesh) in planet_q.iter_mut() {
                let num_faces = mesh.indices.len() / 3;
                let num_factions = state.faction_colors.len();
                let mut regional_load = vec![0.0_f32; num_faces];
                let mut regional_nodes = vec![0; num_faces];
                for (set, _trans, _mat) in s_q.iter() {
                    let face_id = set.face_index as usize;
                    if face_id < num_faces {
                        regional_load[face_id] += set.population;
                        regional_nodes[face_id] += 1;
                    }
                }
                for (mut set, mut transform, mut mat) in s_q.iter_mut() {
                    let s_face_index = set.face_index as usize;
                    let s_faction_id = set.faction_id as usize;
                    if s_face_index >= num_faces || s_faction_id >= num_factions { continue; }
                    let tech = state.faction_tech[s_faction_id];
                    let arab = state.arability[s_face_index];
                    let base_cap = arab * 850_000.0 * (1.0 + tech) * (1.0 + set.infrastructure);
                    let load = regional_load[s_face_index];
                    let effective_capacity = (base_cap - load).max(50.0);
                    let mut pop = set.population;
                    if pop <= effective_capacity {
                        pop += 0.01 * pop * (1.0 - (pop / effective_capacity));
                    } else {
                        pop -= 0.01 * pop * (1.0 - (effective_capacity / pop));
                    }
                    set.population = pop;
                    let cx = state.face_centers[s_face_index * 3];
                    let cy = state.face_centers[s_face_index * 3 + 1];
                    let cz = state.face_centers[s_face_index * 3 + 2];
                    transform.translation = [cx, cy, cz];
                    mat.base_color = [1.0, 0.0, 0.0, 1.0];
                }
                for f in 0..num_faces {
                    state.face_owner[f] = -1;
                    state.face_score[f] = 0.0;
                }
                for (set, transform, _mat) in s_q.iter() {
                    let px = transform.translation[0];
                    let py = transform.translation[1];
                    let pz = transform.translation[2];
                    let max_dist_sq = 1.0;
                    for f in 0..num_faces {
                        let cx = state.face_centers[f * 3];
                        let cy = state.face_centers[f * 3 + 1];
                        let cz = state.face_centers[f * 3 + 2];
                        let dist_sq = (px - cx)*(px - cx) + (py - cy)*(py - cy) + (pz - cz)*(pz - cz);
                        if dist_sq < max_dist_sq {
                            let score = set.population / (dist_sq + 0.015);
                            if score > state.face_score[f] {
                                state.face_score[f] = score;
                                state.face_owner[f] = set.faction_id as i32;
                            }
                        }
                    }
                }
            }
        }
        let mut min_t = f64::MAX;
        for _ in 0..iterations {
            let mut world = World::new();
            let mut mesh = BenchMesh {
                vertices: vec![0.0; 20480 * 3 * 12],
                indices: (0..(20480 * 3) as u32).collect(),
                version: 1,
            };
            for i in 0..20480 * 3 {
                let offset = i * 12;
                mesh.vertices[offset] = 10.0;
                mesh.vertices[offset + 1] = 0.0;
                mesh.vertices[offset + 2] = 0.0;
            }
            let mut face_centers = vec![0.0; 20480 * 3];
            for f in 0..20480 {
                let i0 = mesh.indices[f * 3] as usize * 12;
                let i1 = mesh.indices[f * 3 + 1] as usize * 12;
                let i2 = mesh.indices[f * 3 + 2] as usize * 12;
                face_centers[f * 3] = (mesh.vertices[i0] + mesh.vertices[i1] + mesh.vertices[i2]) / 3.0;
                face_centers[f * 3 + 1] = (mesh.vertices[i0 + 1] + mesh.vertices[i1 + 1] + mesh.vertices[i2 + 1]) / 3.0;
                face_centers[f * 3 + 2] = (mesh.vertices[i0 + 2] + mesh.vertices[i1 + 2] + mesh.vertices[i2 + 2]) / 3.0;
            }
            let state = BenchSimulationState {
                seed_value: 12345,
                face_owner: vec![-1; 20480],
                face_score: vec![0.0; 20480],
                faction_colors: vec![0xff0000; 10],
                faction_tech: vec![0.001; 10],
                step_counter: 0,
                year_value: 1,
                neighbors_flat: Vec::new(),
                neighbors_offsets: vec![0; 20481],
                base_colors: vec![0.5; 20480 * 3],
                run_simulation: 1.0,
                num_colonies: 10.0,
                is_water: vec![0.0; 20480],
                arability: vec![0.8; 20480],
                minerals: vec![0.5; 20480],
                temps: vec![0.5; 20480],
                moistures: vec![0.5; 20480],
                elevations: vec![0.1; 20480],
                face_centers,
            };
            world.spawn((mesh, state));
            for i in 0..500 {
                world.spawn((
                    BenchTransform::default(),
                    BenchMaterial::default(),
                    BenchSettlement {
                        id: i as f32,
                        face_index: (i % 20480) as f32,
                        faction_id: (i % 10) as f32,
                        population: 1000.0,
                        infrastructure: 0.1,
                        wealth: 500.0,
                        name_seed: i as f32,
                        is_capital: if i < 10 { 1.0 } else { 0.0 },
                    }
                ));
            }
            let system_id = world.register_system(vivarium_step_system);
            let start = Instant::now();
            for _step in 0..5 {
                world.run_system(system_id).unwrap();
                world.clear_trackers();
            }
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            if duration < min_t { min_t = duration; }
        }
        println!("Done! ({:.2} ms)", min_t);
        results.push(record("21. Vivarium Simulation Step", 20480, iterations, min_t));
    }

    if !results.is_empty() {
        let output = Output { framework: "bevy".to_string(), benchmarks: results };
        let json = serde_json::to_string_pretty(&output).unwrap();
        let _ = std::fs::create_dir_all("results");
        let mut file = std::fs::File::create("results/bevy_results.json").unwrap();
        file.write_all(json.as_bytes()).unwrap();
    }
}

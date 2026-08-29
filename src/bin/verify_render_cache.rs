use artisan::ecs::*;
use artisan::engine::VisibilityGen;
use artisan::engine::component::*;

fn scene(n: usize) -> artisan::WasmEngine {
    let mut engine = artisan::WasmEngine::new();
    {
        let world = &mut engine.app_mut().world;
        for i in 0..n {
            let x = i as f32 * 4.0;
            world.spawn_with((
                Transform {
                    translation: [x, 0.0, -20.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                },
                GlobalTransform { matrix: glam::Mat4::from_translation(glam::Vec3::new(x, 0.0, -20.0)) },
                GPUInstanceTransform::default(),
                StandardMaterial::default(),
                MeshHandle { id: 1.0 },
                ShaderHandle { id: 0.0 },
                AABB { min: [-1.0, -1.0, -1.0], max: [1.0, 1.0, 1.0], half_size: [1.0, 1.0] },
                Visibility { visible: 1 },
            ));
        }
        world.spawn_with((
            Transform {
                translation: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            GlobalTransform { matrix: glam::Mat4::IDENTITY },
            Camera3D { fov: 0.9, aspect: 1.0, near: 0.1, far: 500.0, ..Default::default() },
        ));
    }
    engine
}

fn visible_instances(engine: &mut artisan::WasmEngine) -> u32 {
    engine.get_batches_3d_internal().chunks(5).map(|b| b[4]).sum()
}

fn vis_gen(engine: &artisan::WasmEngine) -> u64 {
    engine.app().world.get_resource::<VisibilityGen>().map(|g| g.get()).unwrap_or(0)
}

fn camera_entity(engine: &artisan::WasmEngine) -> Entity {
    let world = &engine.app().world;
    world
        .archetypes
        .iter()
        .flat_map(|a| a.entities.iter().copied())
        .find(|&e| world.get_component::<Camera3D>(e).is_some())
        .expect("camera entity")
}

fn main() {
    let mut failures = 0;
    let mut check = |name: &str, ok: bool, detail: String| {
        println!("  [{}] {name}{}", if ok { "ok" } else { "FAIL" }, if detail.is_empty() { String::new() } else { format!(" — {detail}") });
        if !ok {
            failures += 1;
        }
    };

    println!("\n=== 3D batch cache invalidation ===\n");

    {
        let mut engine = scene(64);
        engine.tick(0.016);
        let a = vis_gen(&engine);
        engine.tick(0.016);
        let b = vis_gen(&engine);
        check(
            "a static frame does not advance the visibility generation",
            a == b,
            format!("gen {a} -> {b}"),
        );
    }

    {
        let mut engine = scene(64);
        engine.tick(0.016);

        let before = visible_instances(&mut engine);
        let gen_before = vis_gen(&engine);
        let structural_before = engine.app().world.structural_gen;

        check(
            "entities are visible before the camera moves",
            before > 0,
            format!("{before} instances"),
        );

        let cam = camera_entity(&engine);
        if let Some(t) = engine.app_mut().world.get_component_mut::<Transform>(cam) {
            t.rotation = glam::Quat::from_rotation_y(std::f32::consts::PI).to_array();
        }
        engine.tick(0.016);

        let gen_after = vis_gen(&engine);
        let structural_after = engine.app().world.structural_gen;
        let after = visible_instances(&mut engine);

        check(
            "the camera rotation is not a structural change",
            structural_before == structural_after,
            format!("structural_gen {structural_before} -> {structural_after}"),
        );
        check(
            "turning the camera advances the visibility generation",
            gen_after > gen_before,
            format!("gen {gen_before} -> {gen_after}"),
        );
        check(
            "the extracted batch list reflects the new visibility",
            after < before,
            format!("{before} -> {after} instances"),
        );
    }

    println!();
    if failures == 0 {
        println!("all checks passed\n");
    } else {
        eprintln!("{failures} check(s) failed\n");
        std::process::exit(1);
    }
}

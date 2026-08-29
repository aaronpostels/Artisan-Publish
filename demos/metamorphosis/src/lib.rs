use artisan::WasmEngine;
use artisan::ecs::{Commands, Entity, Query, Res, SystemConfig, With, Without};
use artisan::engine::Time;
use artisan::engine::component::{
    AmbientLight, Camera3D, DirectionalLight, GPUInstanceTransform, GlobalTransform, MeshHandle,
    StandardMaterial, Transform,
};
use std::sync::atomic::{AtomicU32, Ordering};
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Ember {
    pub heat: f32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Frost {
    pub chill: f32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Spark {
    pub charge: f32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Drift {
    pub ox: f32,
    pub oy: f32,
    pub oz: f32,

    pub spin: f32,
}

const SEP: f32 = 18.0;

const CLOUD_R: f32 = 5.6;

const EASE: f32 = 1.9;

const COLOR_EASE: f32 = 1.6;

const CORNER_POS: [[f32; 3]; 8] = [
    [-SEP, -SEP, -SEP],
    [SEP, -SEP, -SEP],
    [-SEP, SEP, -SEP],
    [SEP, SEP, -SEP],
    [-SEP, -SEP, SEP],
    [SEP, -SEP, SEP],
    [-SEP, SEP, SEP],
    [SEP, SEP, SEP],
];

const CORNER_COL: [[f32; 3]; 8] = [
    [0.30, 0.34, 0.42],
    [0.98, 0.42, 0.18],
    [0.24, 0.72, 0.95],
    [0.96, 0.85, 0.32],
    [0.72, 0.32, 0.95],
    [0.99, 0.30, 0.55],
    [0.30, 0.92, 0.78],
    [0.96, 0.96, 0.98],
];

pub struct Churn {

    pub stride: u32,

    pub migrations: AtomicU32,
}

#[inline(always)]
fn hash3(id: u32) -> (f32, f32, f32) {
    let mut h = id.wrapping_mul(0x9E37_79B9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85EB_CA6B);
    let a = h;
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    let b = h;
    h ^= h >> 16;
    let c = h;
    const INV: f32 = 1.0 / 4_294_967_296.0;
    (a as f32 * INV, b as f32 * INV, c as f32 * INV)
}

fn cloud_offset(id: u32) -> Drift {
    let (u, v, w) = hash3(id);

    let r = w.cbrt() * CLOUD_R;
    let cos_phi = v * 2.0 - 1.0;
    let sin_phi = (1.0 - cos_phi * cos_phi).max(0.0).sqrt();
    let theta = u * std::f32::consts::TAU;
    Drift {
        ox: sin_phi * theta.cos() * r,
        oy: cos_phi * r,
        oz: sin_phi * theta.sin() * r,

        spin: if u < 0.5 { -1.0 } else { 1.0 },
    }
}

macro_rules! corner_system {
    ($name:ident, $idx:expr, $fe:ident, $ff:ident, $fs:ident) => {
        fn $name(
            mut q: Query<
                (&mut Transform, &Drift, &mut StandardMaterial),
                ($fe<Ember>, ($ff<Frost>, $fs<Spark>)),
            >,
            time: Res<Time>,
        ) {
            let t = time.elapsed_seconds;
            let dt = time.delta_seconds.min(0.05);

            let blend = 1.0 - (-dt * EASE).exp();
            let cblend = 1.0 - (-dt * COLOR_EASE).exp();
            let home = CORNER_POS[$idx];
            let col = CORNER_COL[$idx];

            let (sin_a, cos_a) = (t * (0.11 + 0.037 * $idx as f32)).sin_cos();

            q.par_for_each(|(tr, d, mat)| {

                let s = sin_a * d.spin;
                let tx = home[0] + d.ox * cos_a + d.oz * s;
                let ty = home[1] + d.oy;
                let tz = home[2] - d.ox * s + d.oz * cos_a;

                tr.translation[0] += (tx - tr.translation[0]) * blend;
                tr.translation[1] += (ty - tr.translation[1]) * blend;
                tr.translation[2] += (tz - tr.translation[2]) * blend;

                let dr = col[0] - mat.base_color[0];
                let dg = col[1] - mat.base_color[1];
                let db = col[2] - mat.base_color[2];
                if dr * dr + dg * dg + db * db > 1e-8 {
                    mat.base_color[0] += dr * cblend;
                    mat.base_color[1] += dg * cblend;
                    mat.base_color[2] += db * cblend;
                    mat.emissive[0] = mat.base_color[0] * 0.32;
                    mat.emissive[1] = mat.base_color[1] * 0.32;
                    mat.emissive[2] = mat.base_color[2] * 0.32;
                }
            });
        }
    };
}

corner_system!(sys_corner_0, 0, Without, Without, Without);
corner_system!(sys_corner_1, 1, With, Without, Without);
corner_system!(sys_corner_2, 2, Without, With, Without);
corner_system!(sys_corner_3, 3, With, With, Without);
corner_system!(sys_corner_4, 4, Without, Without, With);
corner_system!(sys_corner_5, 5, With, Without, With);
corner_system!(sys_corner_6, 6, Without, With, With);
corner_system!(sys_corner_7, 7, With, With, With);

macro_rules! churn_pair {
    ($add:ident, $rem:ident, $M:ident, $field:ident, $salt:expr) => {
        fn $add(
            mut cmds: Commands,
            mut q: Query<Entity, Without<$M>>,
            churn: Res<Churn>,
            time: Res<Time>,
        ) {
            let stride = churn.stride;
            if stride == 0 {
                return;
            }
            let phase = ((time.elapsed_seconds * 61.0) as u32).wrapping_add($salt);
            let (mut i, mut n) = (0u32, 0u32);
            q.for_each(|e| {
                if i.wrapping_add(phase) % stride == 0 {
                    cmds.insert(e, $M { $field: 1.0 });
                    n += 1;
                }
                i = i.wrapping_add(1);
            });
            churn.migrations.fetch_add(n, Ordering::Relaxed);
        }

        fn $rem(
            mut cmds: Commands,
            mut q: Query<Entity, With<$M>>,
            churn: Res<Churn>,
            time: Res<Time>,
        ) {
            let stride = churn.stride;
            if stride == 0 {
                return;
            }

            let phase = ((time.elapsed_seconds * 61.0) as u32)
                .wrapping_add($salt)
                .wrapping_add(stride / 2);
            let (mut i, mut n) = (0u32, 0u32);
            q.for_each(|e| {
                if i.wrapping_add(phase) % stride == 0 {
                    cmds.remove::<$M>(e);
                    n += 1;
                }
                i = i.wrapping_add(1);
            });
            churn.migrations.fetch_add(n, Ordering::Relaxed);
        }
    };
}

churn_pair!(sys_add_ember, sys_rem_ember, Ember, heat, 0);
churn_pair!(sys_add_frost, sys_rem_frost, Frost, chill, 7);
churn_pair!(sys_add_spark, sys_rem_spark, Spark, charge, 19);

fn spawn_scene_lights_and_camera(engine: &mut WasmEngine) {
    let world = &mut engine.app_mut().world;

    let ambient = world.spawn();
    world.add_component(
        ambient,
        AmbientLight {
            color: [0.84, 0.86, 0.92],
            intensity: 8600.0,
        },
    );

    let sun = world.spawn();
    world.add_component(
        sun,
        DirectionalLight {
            color: [1.0, 0.95, 0.88],
            intensity: 9400.0,
            direction: [0.42, -0.72, 0.55],
            pad: 0.0,
        },
    );

    let cam = world.spawn();
    world.add_component(
        cam,
        Transform {
            translation: [0.0, 12.0, 74.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
    );
    world.add_component(cam, GlobalTransform::default());
    world.add_component(
        cam,
        Camera3D {
            fov: 1.16,
            aspect: 1.777,
            near: 0.1,
            far: 500.0,
            exposure: 1.0 / (1.2 * 2.0_f32.powf(13.0)),
            ..Default::default()
        },
    );
}

#[wasm_bindgen]
pub fn create_lattice(count: u32, mesh_id: u32, cube_scale: f32, stride: u32) -> WasmEngine {
    let mut engine = WasmEngine::new();
    {
        let w = &mut engine.app_mut().world;
        w.register_schema::<Ember>("Ember", 0, 1);
        w.register_schema::<Frost>("Frost", 0, 1);
        w.register_schema::<Spark>("Spark", 0, 1);
        w.register_schema::<Drift>("Drift", 0, 4);
    }
    spawn_scene_lights_and_camera(&mut engine);

    let world = &mut engine.app_mut().world;

    for i in 0..count {
        let corner = (i % 8) as usize;
        let home = CORNER_POS[corner];
        let col = CORNER_COL[corner];

        let drift = cloud_offset(i);

        let (u, v, w) = hash3(i ^ 0x5BF0_3635);
        let (a, b, c, d) = (u * 2.0 - 1.0, v * 2.0 - 1.0, w * 2.0 - 1.0, 0.5);
        let len = (a * a + b * b + c * c + d * d).sqrt().max(1e-4);

        let base = (
            Transform {
                translation: [
                    home[0] + drift.ox,
                    home[1] + drift.oy,
                    home[2] + drift.oz,
                ],
                rotation: [a / len, b / len, c / len, d / len],
                scale: [cube_scale; 3],
            },
            GlobalTransform::default(),
            GPUInstanceTransform::default(),
            MeshHandle {
                id: mesh_id as f32,
            },
            StandardMaterial {
                base_color: [col[0], col[1], col[2], 1.0],
                emissive: [col[0] * 0.32, col[1] * 0.32, col[2] * 0.32],
                metallic: 0.0,
                roughness: 0.55,
                pad: [0.0; 3],
            },
            drift,
        );

        let (t, g, gi, m, s, d) = base;
        let e = Ember { heat: 1.0 };
        let f = Frost { chill: 1.0 };
        let k = Spark { charge: 1.0 };
        match corner {
            0 => world.spawn_with((t, g, gi, m, s, d)),
            1 => world.spawn_with((t, g, gi, m, s, d, e)),
            2 => world.spawn_with((t, g, gi, m, s, d, f)),
            3 => world.spawn_with((t, g, gi, m, s, d, e, f)),
            4 => world.spawn_with((t, g, gi, m, s, d, k)),
            5 => world.spawn_with((t, g, gi, m, s, d, e, k)),
            6 => world.spawn_with((t, g, gi, m, s, d, f, k)),
            _ => world.spawn_with((t, g, gi, m, s, d, e, f, k)),
        };
    }

    world.insert_resource(Churn {
        stride,
        migrations: AtomicU32::new(0),
    });

    let app = engine.app_mut();

    app.add_system(sys_add_ember.before("transform"));
    app.add_system(sys_rem_ember.before("transform"));
    app.add_system(sys_add_frost.before("transform"));
    app.add_system(sys_rem_frost.before("transform"));
    app.add_system(sys_add_spark.before("transform"));
    app.add_system(sys_rem_spark.before("transform"));

    app.add_system(sys_corner_0.before("transform"));
    app.add_system(sys_corner_1.before("transform"));
    app.add_system(sys_corner_2.before("transform"));
    app.add_system(sys_corner_3.before("transform"));
    app.add_system(sys_corner_4.before("transform"));
    app.add_system(sys_corner_5.before("transform"));
    app.add_system(sys_corner_6.before("transform"));
    app.add_system(sys_corner_7.before("transform"));

    engine
}

#[wasm_bindgen]
pub fn set_churn(engine: &mut WasmEngine, stride: u32) {
    if let Some(c) = engine.app_mut().world.get_resource_mut::<Churn>() {
        c.stride = stride;
    }
}

#[wasm_bindgen]
pub fn take_migrations(engine: &WasmEngine) -> u32 {
    engine
        .app()
        .world
        .get_resource::<Churn>()
        .map(|c| c.migrations.swap(0, Ordering::Relaxed))
        .unwrap_or(0)
}

#[wasm_bindgen]
pub fn corner_counts(engine: &WasmEngine) -> Vec<u32> {
    let w = &engine.app().world;
    let mesh = match w.get_component_id::<MeshHandle>() {
        Some(id) => id,
        None => return vec![0; 8],
    };
    let (ei, fi, si) = (
        w.get_component_id::<Ember>(),
        w.get_component_id::<Frost>(),
        w.get_component_id::<Spark>(),
    );

    let mut out = vec![0u32; 8];
    for a in &w.archetypes {
        if a.entities.is_empty() || !a.signature.contains(&mesh) {
            continue;
        }
        let has = |id: Option<usize>| id.is_some_and(|c| a.signature.contains(&c));
        let idx = usize::from(has(ei)) | usize::from(has(fi)) << 1 | usize::from(has(si)) << 2;
        out[idx] += a.entities.len() as u32;
    }
    out
}

#[wasm_bindgen]
pub fn live_archetypes(engine: &WasmEngine) -> u32 {
    engine
        .app()
        .world
        .archetypes
        .iter()
        .filter(|a| !a.entities.is_empty())
        .count() as u32
}

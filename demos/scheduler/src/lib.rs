use artisan::WasmEngine;
use artisan::ecs::{Query, Res, SystemConfig};
use artisan::engine::Time;
use artisan::engine::component::{
    AmbientLight, Camera3D, DirectionalLight, GPUInstanceTransform, GlobalTransform, MeshHandle,
    StandardMaterial, Transform,
};
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Cell {
    pub hx: f32,
    pub hz: f32,

    pub radius: f32,
    pub pad: f32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Vel {
    pub vy: f32,
    pub pad: [f32; 3],
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Wave {
    pub height: f32,
    pub sharp: f32,
    pub pad: [f32; 2],
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Heat {
    pub v: f32,
    pub pad: [f32; 3],
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Charge {
    pub v: f32,
    pub pad: [f32; 3],
}

const GAP: f32 = 0.62;

const AMP: f32 = 2.1;

fn sys_gravity(mut q: Query<&mut Vel>, time: Res<Time>) {
    let t = time.elapsed_seconds;
    let pull = (t * 0.7).sin() * 0.35;
    q.for_each(|v| {
        v.vy = (v.vy + pull * 0.06) * 0.985;
    });
}

fn sys_heat(mut q: Query<(&mut Heat, &Cell)>, time: Res<Time>) {
    let t = time.elapsed_seconds;
    q.for_each(|(h, c)| {
        let target = 0.5 + 0.5 * (c.radius * 0.20 - t * 1.9).sin();
        h.v += (target - h.v) * 0.25;
    });
}

fn sys_charge(mut q: Query<(&mut Charge, &Cell)>, time: Res<Time>) {
    let t = time.elapsed_seconds;
    q.for_each(|(c, cell)| {
        let target = 0.5 + 0.5 * ((cell.hx + cell.hz) * 0.11 + t * 1.4).sin();
        c.v += (target - c.v) * 0.25;
    });
}

fn sys_wave(mut q: Query<(&mut Wave, &Cell)>, time: Res<Time>) {
    let t = time.elapsed_seconds;
    q.for_each(|(w, c)| {
        let a = (c.hx * 0.18 - t * 1.7).sin();
        let r = (c.radius * 0.26 - t * 2.3).sin();
        let h = a * 0.42 + r * 0.78;
        w.height = h * AMP;
        w.sharp = h;
    });
}

fn sys_integrate(mut q: Query<(&mut Transform, &Vel, &Cell)>, time: Res<Time>) {
    let dt = time.delta_seconds.min(0.05);
    q.for_each(|(tr, v, c)| {
        tr.translation[0] = c.hx;
        tr.translation[2] = c.hz;

        tr.translation[1] = v.vy * dt * 0.35;
    });
}

fn sys_shade(mut q: Query<(&mut StandardMaterial, &Heat, &Charge)>) {
    q.for_each(|(m, h, c)| {
        m.base_color[0] = 0.16 + h.v * 0.82;
        m.base_color[1] = 0.20 + c.v * 0.42;
        m.base_color[2] = 0.55 + (1.0 - h.v) * 0.42;
    });
}

fn sys_ripple(mut q: Query<(&mut Transform, &Wave)>) {
    q.for_each(|(tr, w)| {
        tr.translation[1] += w.height;
        let s = 0.26 + w.sharp.abs() * 0.30;
        tr.scale = [s, s, s];
    });
}

fn sys_recolor(mut q: Query<(&mut StandardMaterial, &Wave)>) {
    q.for_each(|(m, w)| {
        let crest = (w.sharp * 0.5 + 0.5).clamp(0.0, 1.0);
        let glow = crest * crest * 0.9;
        m.emissive[0] = m.base_color[0] * glow;
        m.emissive[1] = m.base_color[1] * glow;
        m.emissive[2] = m.base_color[2] * glow;
    });
}

fn spawn_scene_lights_and_camera(engine: &mut WasmEngine, extent: f32) {
    let world = &mut engine.app_mut().world;

    let ambient = world.spawn();
    world.add_component(
        ambient,
        AmbientLight {
            color: [0.80, 0.84, 0.94],
            intensity: 7600.0,
        },
    );

    let sun = world.spawn();
    world.add_component(
        sun,
        DirectionalLight {
            color: [1.0, 0.94, 0.86],
            intensity: 10200.0,
            direction: [0.35, -0.80, 0.48],
            pad: 0.0,
        },
    );

    let cam = world.spawn();
    world.add_component(
        cam,
        Transform {
            translation: [0.0, extent * 0.55, extent * 1.25],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
    );
    world.add_component(cam, GlobalTransform::default());
    world.add_component(
        cam,
        Camera3D {
            fov: 1.05,
            aspect: 1.777,
            near: 0.1,
            far: 900.0,
            exposure: 1.0 / (1.2 * 2.0_f32.powf(13.0)),
            ..Default::default()
        },
    );
}

#[wasm_bindgen]
pub fn create_field(side: u32, mesh_id: u32) -> WasmEngine {
    let mut engine = WasmEngine::new();
    {
        let w = &mut engine.app_mut().world;
        w.register_schema::<Cell>("Cell", 0, 4);
        w.register_schema::<Vel>("Vel", 0, 4);
        w.register_schema::<Wave>("Wave", 0, 4);
        w.register_schema::<Heat>("Heat", 0, 4);
        w.register_schema::<Charge>("Charge", 0, 4);
    }

    let side = side.max(2);
    let extent = side as f32 * GAP;
    spawn_scene_lights_and_camera(&mut engine, extent);

    let world = &mut engine.app_mut().world;
    let half = extent * 0.5;

    for gz in 0..side {
        for gx in 0..side {
            let hx = gx as f32 * GAP - half;
            let hz = gz as f32 * GAP - half;
            let radius = (hx * hx + hz * hz).sqrt();

            world.spawn_with((
                Transform {
                    translation: [hx, 0.0, hz],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [0.3; 3],
                },
                GlobalTransform::default(),
                GPUInstanceTransform::default(),
                MeshHandle {
                    id: mesh_id as f32,
                },
                StandardMaterial {
                    base_color: [0.4, 0.5, 0.8, 1.0],
                    emissive: [0.0; 3],
                    metallic: 0.0,
                    roughness: 0.5,
                    pad: [0.0; 3],
                },
                Cell { hx, hz, radius, pad: 0.0 },
                Vel::default(),
                Wave::default(),
                Heat { v: 0.5, pad: [0.0; 3] },
                Charge { v: 0.5, pad: [0.0; 3] },
            ));
        }
    }

    let app = engine.app_mut();

    app.add_system(sys_gravity.before("transform"));
    app.add_system(sys_heat.before("transform"));
    app.add_system(sys_charge.before("transform"));
    app.add_system(sys_wave.before("transform"));
    app.add_system(sys_integrate.before("transform"));
    app.add_system(sys_shade.before("transform"));
    app.add_system(sys_ripple.before("transform"));
    app.add_system(sys_recolor.before("transform"));

    app.update_schedule.instrument = true;
    engine
}

#[wasm_bindgen]
pub fn set_parallel(engine: &mut WasmEngine, parallel: bool) {
    engine.app_mut().update_schedule.parallel = parallel;
}

#[wasm_bindgen]
pub fn schedule_json(engine: &WasmEngine) -> String {
    let app = engine.app();
    app.update_schedule.describe(&app.world)
}

#[wasm_bindgen]
pub fn trace_data(engine: &WasmEngine) -> Vec<f64> {
    let trace = &engine.app().update_schedule.trace;
    let base = trace
        .iter()
        .map(|r| r.start_ms)
        .fold(f64::INFINITY, f64::min);
    if !base.is_finite() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(trace.len() * 5);
    for r in trace {
        out.push(r.stage as f64);
        out.push(r.index_in_stage as f64);
        out.push(r.thread as f64);
        out.push(r.start_ms - base);
        out.push(r.end_ms - base);
    }
    out
}

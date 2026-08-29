use artisan::WasmEngine;
use artisan::ecs::{Query, Res, ResMut, SystemConfig};
use artisan::engine::Time;
use artisan::engine::component::{
    AmbientLight, Camera3D, DirectionalLight, GPUDrivenSimulation, GPUInstanceTransform,
    GlobalTransform, MeshHandle, StandardMaterial, Transform,
};
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Flow {
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,

    pub agility: f32,
}

const BOUNDS: f32 = 34.0;

const FLOW_SPEED: f32 = 7.0;

const GRID: usize = 24;

const GRID_REFRESH: u32 = 3;

pub struct FlowGrid {

    cells: Vec<f32>,
    ticks: u32,
}

impl FlowGrid {
    fn new() -> Self {
        Self {
            cells: vec![0.0; GRID * GRID * GRID * 3],
            ticks: 0,
        }
    }

    #[inline(always)]
    fn cell_span() -> f32 {
        (BOUNDS * 2.0) / GRID as f32
    }

    fn rebuild(&mut self, t: f32) {
        let span = Self::cell_span();
        let mut i = 0;
        for gz in 0..GRID {
            let z = -BOUNDS + (gz as f32 + 0.5) * span;
            for gy in 0..GRID {
                let y = -BOUNDS + (gy as f32 + 0.5) * span;
                for gx in 0..GRID {
                    let x = -BOUNDS + (gx as f32 + 0.5) * span;
                    let (vx, vy, vz) = flow_field(x, y, z, t);
                    self.cells[i] = vx;
                    self.cells[i + 1] = vy;
                    self.cells[i + 2] = vz;
                    i += 3;
                }
            }
        }
    }

    #[inline(always)]
    fn sample(&self, x: f32, y: f32, z: f32) -> (f32, f32, f32) {
        let span = Self::cell_span();
        let gx = (((x + BOUNDS) / span) as i32).clamp(0, GRID as i32 - 1) as usize;
        let gy = (((y + BOUNDS) / span) as i32).clamp(0, GRID as i32 - 1) as usize;
        let gz = (((z + BOUNDS) / span) as i32).clamp(0, GRID as i32 - 1) as usize;
        let i = ((gz * GRID + gy) * GRID + gx) * 3;
        unsafe {
            (
                *self.cells.get_unchecked(i),
                *self.cells.get_unchecked(i + 1),
                *self.cells.get_unchecked(i + 2),
            )
        }
    }
}

#[inline(always)]
fn flow_field(x: f32, y: f32, z: f32, t: f32) -> (f32, f32, f32) {
    const S: f32 = 0.09;

    let mut vx = (y * S + t * 0.31).sin() + (z * S * 1.3 - t * 0.21).cos();
    let mut vy = (z * S * 1.1 + t * 0.27).sin() + (x * S * 0.9 + t * 0.19).cos();
    let mut vz = (x * S * 1.2 - t * 0.23).sin() + (y * S * 1.05 + t * 0.25).cos();

    vx += -z * 0.05;
    vz += x * 0.05;

    let r2 = x * x + y * y + z * z;
    let r = r2.sqrt();
    let over = (r / BOUNDS - 0.6).max(0.0);
    if over > 0.0 && r > 0.001 {
        let pull = over * over * 3.0;
        vx -= x / r * pull;
        vy -= y / r * pull;
        vz -= z / r * pull;
    }

    (vx, vy, vz)
}

fn sys_refresh_grid(mut grid: ResMut<FlowGrid>, time: Res<Time>) {
    if grid.ticks % GRID_REFRESH == 0 {
        let t = time.elapsed_seconds;
        grid.rebuild(t);
    }
    grid.ticks = grid.ticks.wrapping_add(1);
}

fn sys_flow(mut q: Query<(&mut Transform, &mut Flow)>, time: Res<Time>, grid: Res<FlowGrid>) {
    let dt = time.delta_seconds.min(0.05);

    let blend = 1.0 - (-dt * 2.5).exp();
    let grid = &*grid;

    q.par_for_each(|(tr, f)| {
        let p = tr.translation;
        let (fx, fy, fz) = grid.sample(p[0], p[1], p[2]);

        let speed = FLOW_SPEED * f.agility;
        f.vx += (fx * speed - f.vx) * blend;
        f.vy += (fy * speed - f.vy) * blend;
        f.vz += (fz * speed - f.vz) * blend;

        tr.translation[0] = p[0] + f.vx * dt;
        tr.translation[1] = p[1] + f.vy * dt;
        tr.translation[2] = p[2] + f.vz * dt;
    });
}

struct Rng(u32);
impl Rng {
    #[inline(always)]
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32
    }
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h % 1.0) * 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c * 0.5;
    [r + m, g + m, b + m]
}

fn spawn_scene_lights_and_camera(engine: &mut WasmEngine) {
    let world = &mut engine.app_mut().world;

    let ambient = world.spawn();
    world.add_component(
        ambient,

        AmbientLight {
            color: [0.86, 0.87, 0.90],
            intensity: 8200.0,
        },
    );

    let sun = world.spawn();
    world.add_component(
        sun,
        DirectionalLight {
            color: [1.0, 0.93, 0.82],
            intensity: 9800.0,
            direction: [0.4, -0.75, 0.52],
            pad: 0.0,
        },
    );

    let cam = world.spawn();
    world.add_component(
        cam,
        Transform {
            translation: [0.0, 6.0, 52.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
    );
    world.add_component(cam, GlobalTransform::default());
    let exposure = 1.0 / (1.2 * 2.0_f32.powf(13.0));
    world.add_component(
        cam,
        Camera3D {

            fov: 1.45,
            aspect: 1.777,
            near: 0.1,
            far: 400.0,
            exposure,
            ..Default::default()
        },
    );
}

#[wasm_bindgen]
pub fn create_swarm_cpu(count: u32, mesh_id: u32, cube_scale: f32) -> WasmEngine {
    let mut engine = WasmEngine::new();
    engine
        .app_mut()
        .world
        .register_schema::<Flow>("Flow", 0, 4);
    spawn_scene_lights_and_camera(&mut engine);

    let world = &mut engine.app_mut().world;
    let mut rng = Rng(0x9E3779B9);

    for _ in 0..count {

        let theta = rng.next_f32() * std::f32::consts::TAU;
        let cos_phi = rng.next_f32() * 2.0 - 1.0;
        let sin_phi = (1.0 - cos_phi * cos_phi).max(0.0).sqrt();
        let dist = rng.next_f32().powf(0.55) * BOUNDS * 0.75;

        let x = sin_phi * theta.cos() * dist;
        let y = cos_phi * dist * 0.65;
        let z = sin_phi * theta.sin() * dist;

        let (rx, ry, rz, rw) = {
            let a = rng.next_f32() * 2.0 - 1.0;
            let b = rng.next_f32() * 2.0 - 1.0;
            let c = rng.next_f32() * 2.0 - 1.0;
            let d = rng.next_f32() * 2.0 - 1.0;
            let len = (a * a + b * b + c * c + d * d).sqrt().max(1e-4);
            (a / len, b / len, c / len, d / len)
        };

        let hue = (z.atan2(x) / std::f32::consts::TAU + 0.5 + y * 0.006).rem_euclid(1.0);
        let lift = 0.45 + (dist / BOUNDS) * 0.3;
        let rgb = hsl_to_rgb(hue, 0.72, lift);
        let agility = 0.7 + rng.next_f32() * 0.6;

        world.spawn_with((
            Transform {
                translation: [x, y, z],
                rotation: [rx, ry, rz, rw],
                scale: [cube_scale; 3],
            },
            GlobalTransform::default(),
            GPUInstanceTransform::default(),
            MeshHandle {
                id: mesh_id as f32,
            },
            StandardMaterial {
                base_color: [rgb[0], rgb[1], rgb[2], 1.0],

                emissive: [rgb[0] * 0.3, rgb[1] * 0.3, rgb[2] * 0.3],
                metallic: 0.0,
                roughness: 0.65,
                pad: [0.0; 3],
            },
            Flow {
                vx: 0.0,
                vy: 0.0,
                vz: 0.0,
                agility,
            },
        ));
    }

    engine.app_mut().world.insert_resource(FlowGrid::new());
    engine.app_mut().add_system(sys_refresh_grid.before("transform"));
    engine.app_mut().add_system(sys_flow.before("transform"));
    engine
}

#[wasm_bindgen]
pub fn create_swarm_gpu(count: u32, mesh_id: u32, cube_scale: f32) -> WasmEngine {
    let mut engine = WasmEngine::new();
    spawn_scene_lights_and_camera(&mut engine);

    let world = &mut engine.app_mut().world;
    let sim = world.spawn();
    world.add_component(
        sim,
        GPUDrivenSimulation {
            max_instances: count as f32,
            mesh_id: mesh_id as f32,
            shader_type: 7.0,
            speed: 1.0,
            size: cube_scale,
            gravity: 0.0,
            noise_scale: 1.0,
            pad: 0.0,
        },
    );

    engine
}

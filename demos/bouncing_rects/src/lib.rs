use artisan::WasmEngine;
use artisan::engine::component::{Camera3D, GPUDrivenSimulation, GlobalTransform, Transform};
use wasm_bindgen::prelude::*;

const FOV: f32 = 0.8;

const CAM_DIST: f32 = 2.365_204_5;

const SHADER_TYPE: f32 = 21.0;

#[wasm_bindgen]
pub fn create_rects(count: u32, mesh_id: u32) -> WasmEngine {
    let mut engine = WasmEngine::new();
    let world = &mut engine.app_mut().world;

    let cam = world.spawn();
    world.add_component(
        cam,
        Transform {
            translation: [0.0, 0.0, CAM_DIST],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
    );
    world.add_component(cam, GlobalTransform::default());
    world.add_component(
        cam,
        Camera3D {
            fov: FOV,
            aspect: 1.777,
            near: 0.1,
            far: 100.0,
            exposure: 1.0,
            ..Default::default()
        },
    );

    let sim = world.spawn();
    world.add_component(
        sim,
        GPUDrivenSimulation {
            max_instances: count as f32,
            mesh_id: mesh_id as f32,
            shader_type: SHADER_TYPE,
            speed: 1.0,
            size: 1.0,

            gravity: 1.777,
            noise_scale: 1.0,
            pad: 0.0,
        },
    );

    engine
}

#[wasm_bindgen]
pub fn mesh_unit_quad() -> js_sys::Object {
    #[rustfmt::skip]
    let v: Vec<f32> = vec![

        -1.0, -1.0, 0.0,    0.0, 0.0, 1.0,   0.0, 1.0,  1.0, 1.0, 1.0, 1.0,
         1.0, -1.0, 0.0,    0.0, 0.0, 1.0,   1.0, 1.0,  1.0, 1.0, 1.0, 1.0,
        -1.0,  1.0, 0.0,    0.0, 0.0, 1.0,   0.0, 0.0,  1.0, 1.0, 1.0, 1.0,
         1.0,  1.0, 0.0,    0.0, 0.0, 1.0,   1.0, 0.0,  1.0, 1.0, 1.0, 1.0,
    ];
    let i: Vec<u32> = vec![0, 1, 2, 2, 1, 3];

    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &"vertices".into(),
        &js_sys::Float32Array::from(v.as_slice()),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &"indices".into(),
        &js_sys::Uint32Array::from(i.as_slice()),
    )
    .unwrap();
    obj
}

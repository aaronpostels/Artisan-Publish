#![allow(clippy::type_complexity)]
pub mod bench;
pub mod ecs;
pub mod engine;

use wasm_bindgen::prelude::*;
use engine::{App, Plugin, Time};
use crate::ecs::{SystemConfig, Component};
use crate::engine::component::*;
use std::collections::HashMap;

pub use wasm_bindgen_rayon::init_thread_pool;
pub use crate::engine::mesh::{mesh_cube, mesh_sphere, mesh_plane, mesh_cylinder, mesh_icosphere, mesh_icosphere_native, mesh_sphere_native, mesh_cylinder_native, build_face_adjacency_native, mesh_quad_2d, mesh_circle_2d, mesh_ring_2d, mesh_capsule_2d};
pub use crate::engine::logger::{wasm_info, wasm_warn, wasm_error};

#[macro_export]
macro_rules! register_component_schema {
    ($world:expr, $type:ty, $($field:ident),* $(,)?) => {
        let id = $world.register_serializable::<$type>();
        if !$world.schemas.iter().any(|s| s.id == id) {
            let fields = vec![$(stringify!($field).to_string()),*];
            let elements = fields.len() as u32;
            let name = stringify!($type).split("::").last().unwrap_or(stringify!($type)).to_string();
            $world.schemas.push($crate::ecs::world::ComponentSchema {
                id,
                name,
                array_type: 0,
                elements,
                fields,
            });
        }
    };
    ($world:expr, $type:ty, $elements:expr, $($field:ident : $offset:expr),* $(,)?) => {
        let id = $world.register_serializable::<$type>();
        if !$world.schemas.iter().any(|s| s.id == id) {
            let mut fields = vec!["".to_string(); $elements];
            for (field_name, offset) in vec![$( (stringify!($field).to_string(), $offset as usize) ),*] {
                if offset < fields.len() {
                    fields[offset] = field_name;
                }
            }
            let name = stringify!($type).split("::").last().unwrap_or(stringify!($type)).to_string();
            $world.schemas.push($crate::ecs::world::ComponentSchema {
                id,
                name,
                array_type: 0,
                elements: $elements,
                fields,
            });
        }
    };
}

#[derive(Default)]
struct RenderState {
    draw_calls: Vec<u32>,
    light_data: Vec<f32>,
}

pub struct CorePlugin;
impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Time>();
        app.init_resource::<crate::engine::input::Input>();
        app.init_resource::<crate::engine::spatial::SpatialGrid>();
        app.init_resource::<crate::engine::spatial_3d::SpatialGrid3D>();
        app.init_resource::<crate::engine::spatial::TileGrid>();
        app.init_resource::<crate::engine::VisibilityGen>();
        app.add_system(crate::engine::sys_integrate_velocity.label("physics"));
        app.add_system(crate::engine::sys_update_path_followers.label("path_follower").after("physics"));
        app.add_system(crate::engine::sys_apply_angular_velocity.label("angular_physics").after("physics"));
        app.add_system(crate::engine::sys_fly_camera.label("input").before("physics"));
        app.add_system(crate::engine::sys_sprite_animate.label("animation"));
        app.add_system(crate::engine::transform::sys_auto_add_gpu_transforms.label("gpu_transform_add").before("transform"));
        app.add_system(crate::engine::transform::sys_update_billboards.label("billboard").before("transform"));
        app.add_system(crate::engine::transform::update_global_transforms.label("transform").after("angular_physics"));
        app.add_system(crate::engine::transform::update_global_transforms_2d.label("transform_2d").after("path_follower"));
        app.add_system(crate::engine::sys_update_camera_matrices.label("camera").after("transform"));
        app.add_system(crate::engine::sys_frustum_culling.label("culling").after("camera"));
        app.add_system(crate::engine::spatial_3d::sys_build_bvh.label("bvh").after("physics"));
        app.add_system(crate::engine::sys_update_spatial_grid.label("spatial").after("transform_2d"));
        app.add_system(crate::engine::sys_update_spatial_grid_3d.label("spatial_3d").after("transform"));
    }
}

#[wasm_bindgen]
pub struct WasmEngine {
    pub(crate) app: App,
    global_resources: HashMap<String, Vec<f32>>,
    render_state: RenderState,
    last_structural_gen_3d: u64,
    last_structural_gen_2d: u64,

    last_visibility_gen: u64,
}

impl WasmEngine {
    pub fn app(&self) -> &App { &self.app }
    pub fn app_mut(&mut self) -> &mut App { &mut self.app }

    pub fn get_batches_3d_internal(&mut self) -> &[u32] {
        self.render_state.draw_calls.clear();
        let gt_cid = self.app.world.get_component_id::<GPUInstanceTransform>();
        let mat_cid = self.app.world.get_component_id::<StandardMaterial>();
        let mesh_cid = self.app.world.get_component_id::<MeshHandle>();
        let shader_cid = self.app.world.get_component_id::<ShaderHandle>();
        let vis_cid = self.app.world.get_component_id::<Visibility>();

        if let (Some(gt_id), Some(mat_id), Some(mesh_id)) = (gt_cid, mat_cid, mesh_cid) {
            for arch in &self.app.world.archetypes {
                let len = arch.entities.len();
                if len == 0 { continue; }
                if gt_id >= arch.component_to_column.len() || mat_id >= arch.component_to_column.len() || mesh_id >= arch.component_to_column.len() { continue; }

                let gt_col = arch.component_to_column[gt_id];
                let mat_col = arch.component_to_column[mat_id];
                let mesh_col = arch.component_to_column[mesh_id];
                let shader_col = shader_cid.and_then(|id| {
                    if id < arch.component_to_column.len() { Some(arch.component_to_column[id]) } else { None }
                }).unwrap_or(u32::MAX);

                let vis_ptr = if let Some(vid) = vis_cid {
                    if vid < arch.component_to_column.len() && arch.component_to_column[vid] != u32::MAX {
                        unsafe { (*arch.columns[arch.component_to_column[vid] as usize].get()).data.as_ptr::<Visibility>() }
                    } else { std::ptr::null() }
                } else { std::ptr::null() };

                if gt_col != u32::MAX && mat_col != u32::MAX && mesh_col != u32::MAX {
                    let gt_base = unsafe { (*arch.columns[gt_col as usize].get()).data.as_ptr::<GPUInstanceTransform>() };
                    let mat_base = unsafe { (*arch.columns[mat_col as usize].get()).data.as_ptr::<StandardMaterial>() };
                    let mesh_base = unsafe { (*arch.columns[mesh_col as usize].get()).data.as_ptr::<MeshHandle>() };
                    let shader_base = if shader_col != u32::MAX {
                        unsafe { (*arch.columns[shader_col as usize].get()).data.as_ptr::<ShaderHandle>() }
                    } else { std::ptr::null() };

                    let mut run_start = u32::MAX;
                    let mut current_mesh = u32::MAX;
                    let mut current_shader = u32::MAX;

                    for i in 0..len {
                        if !vis_ptr.is_null() && unsafe { (*vis_ptr.add(i)).visible == 0 } {
                            if run_start != u32::MAX {
                                self.render_state.draw_calls.push(unsafe { gt_base.add(run_start as usize) } as u32);
                                self.render_state.draw_calls.push(unsafe { mat_base.add(run_start as usize) } as u32);
                                self.render_state.draw_calls.push(current_shader);
                                self.render_state.draw_calls.push(current_mesh);
                                self.render_state.draw_calls.push(i as u32 - run_start);
                                run_start = u32::MAX;
                            }
                            continue;
                        }

                        let mesh = unsafe { (*mesh_base.add(i)).id as u32 };
                        let shader = if !shader_base.is_null() {
                            unsafe { (*shader_base.add(i)).id as u32 }
                        } else { 0 };

                        if run_start == u32::MAX {
                            run_start = i as u32;
                            current_mesh = mesh;
                            current_shader = shader;
                        } else if mesh != current_mesh || shader != current_shader {
                            self.render_state.draw_calls.push(unsafe { gt_base.add(run_start as usize) } as u32);
                            self.render_state.draw_calls.push(unsafe { mat_base.add(run_start as usize) } as u32);
                            self.render_state.draw_calls.push(current_shader);
                            self.render_state.draw_calls.push(current_mesh);
                            self.render_state.draw_calls.push(i as u32 - run_start);

                            run_start = i as u32;
                            current_mesh = mesh;
                            current_shader = shader;
                        }
                    }

                    if run_start != u32::MAX {
                        self.render_state.draw_calls.push(unsafe { gt_base.add(run_start as usize) } as u32);
                        self.render_state.draw_calls.push(unsafe { mat_base.add(run_start as usize) } as u32);
                        self.render_state.draw_calls.push(current_shader);
                        self.render_state.draw_calls.push(current_mesh);
                        self.render_state.draw_calls.push(len as u32 - run_start);
                    }
                }
            }
        }
        &self.render_state.draw_calls
    }

    fn register_standard_schemas(&mut self) {
        self.register_standard_schema::<Transform>("Transform", &["tx", "ty", "tz", "rx", "ry", "rz", "rw", "sx", "sy", "sz"], 10);
        self.register_standard_schema::<Transform2D>("Transform2D", &["x", "y", "z", "rotation", "scale_x", "scale_y"], 6);
        self.register_standard_schema::<GlobalTransform2D>("GlobalTransform2D", &["x", "y", "z", "rotation", "scale_x", "scale_y"], 6);
        self.register_standard_schema::<GlobalTransform>("GlobalTransform", &[], 16);
        self.register_standard_schema::<GPUDrivenSimulation>("GPUDrivenSimulation", &[
            "max_instances", "mesh_id", "shader_type", "speed", "size", "gravity", "noise_scale", "pad"
        ], 8);
        self.register_standard_schema::<Camera3D>("Camera3D", &[
            "fov", "aspect", "near", "far",
            "v00", "v01", "v02", "v03", "v10", "v11", "v12", "v13",
            "v20", "v21", "v22", "v23", "v30", "v31", "v32", "v33",
            "cx", "cy", "cz", "exposure"
        ], 40);
        self.register_standard_schema::<StandardMaterial>("StandardMaterial", &[
            "r", "g", "b", "a", "em_r", "em_g", "em_b", "metallic", "roughness", "pad1", "pad2", "pad3"
        ], 12);
        self.register_standard_schema::<GPUInstanceTransform>("GPUInstanceTransform", &["tx", "ty", "tz", "r_packed_0", "r_packed_1", "scale"], 6);
        self.register_standard_schema::<DirectionalLight>("DirectionalLight", &["r", "g", "b", "intensity", "dx", "dy", "dz", "pad"], 8);
        self.register_standard_schema::<PointLight>("PointLight", &["r", "g", "b", "intensity", "range", "p1", "p2", "p3"], 8);
        self.register_standard_schema::<AmbientLight>("AmbientLight", &["r", "g", "b", "intensity"], 4);
        self.register_standard_schema::<HemisphereLight>("HemisphereLight", &[
            "sky_r", "sky_g", "sky_b", "sky_intensity", "ground_r", "ground_g", "ground_b", "ground_intensity"
        ], 8);
        self.register_standard_schema::<Velocity>("Velocity", &["x", "y"], 2);
        self.register_standard_schema::<AngularVelocity>("AngularVelocity", &["x", "y", "z"], 3);
        self.register_standard_schema::<SpriteMaterial>("SpriteMaterial", &["r", "g", "b", "a", "texture_id", "uv_x", "uv_y", "uv_w", "uv_h"], 9);
        self.register_standard_schema::<SpriteAnimation>("SpriteAnimation", &["fps", "frame_count", "current_frame", "timer", "width_per_frame"], 5);
        self.register_standard_schema::<Camera>("Camera", &["zoom", "active"], 2);
        self.world_register_standard_only::<FlyCamera>("FlyCamera", &["yaw", "pitch", "speed", "sensitivity", "active"], 5);
        self.world_register_standard_only::<Collider>("Collider", &["radius"], 1);
        self.world_register_standard_only::<Collider3D>("Collider3D", &["radius"], 1);
        self.world_register_standard_only::<MeshHandle>("MeshHandle", &["id"], 1);
        self.world_register_standard_only::<ShaderHandle>("ShaderHandle", &["id"], 1);
        self.world_register_standard_only::<DynamicMesh>("DynamicMesh", &[], 3);
        self.world_register_standard_only::<MeshBVH>("MeshBVH", &[], 3);
        self.world_register_standard_only::<AABB>("AABB", &["min_x", "min_y", "min_z", "max_x", "max_y", "max_z", "hw", "hh"], 8);
        self.world_register_standard_only::<Visibility>("Visibility", &["visible"], 1);
        self.world_register_standard_only::<Obstacle>("Obstacle", &[], 0);

        self.register_standard_schema::<BoxCollider2D>("BoxCollider2D", &["half_x", "half_y"], 2);
        self.register_standard_schema::<CapsuleCollider2D>("CapsuleCollider2D", &["half_length", "radius"], 2);
        self.register_standard_schema::<Shape2D>("Shape2D", &[
            "shape_type", "color_r", "color_g", "color_b", "color_a",
            "extents_x", "extents_y", "border_radius",
            "border_color_r", "border_color_g", "border_color_b", "border_color_a", "border_thickness",
            "grad_type", "grad_color_r", "grad_color_g", "grad_color_b", "grad_color_a",
            "grad_p0_x", "grad_p0_y", "grad_p1_x", "grad_p1_y"
        ], 22);
        self.register_standard_schema::<PathFollower>("PathFollower", &["target_id", "target_gen", "speed", "progress", "active"], 5);
        self.register_standard_schema::<NetworkNode>("NetworkNode", &[
            "conn0_id", "conn0_gen", "conn1_id", "conn1_gen", "conn2_id", "conn2_gen", "conn3_id", "conn3_gen",
            "flow0", "flow1", "flow2", "flow3"
        ], 12);
        self.register_standard_schema::<Text2D>("Text2D", &[
            "font_size", "color_r", "color_g", "color_b", "color_a",
            "alignment", "line_spacing", "len",
            "bold", "italic", "underline", "pad"
        ], 132);
    }

    fn register_standard_schema<T: Component + serde::Serialize + serde::de::DeserializeOwned>(&mut self, name: &str, fields: &[&str], elements: u32) {
        let id = self.app.world.register_serializable::<T>();
        if !self.app.world.schemas.iter().any(|s| s.id == id) {
            let fields_vec = fields.iter().map(|&s| s.to_string()).collect();
            self.app.world.schemas.push(crate::ecs::world::ComponentSchema { id, name: name.to_string(), array_type: 0, elements, fields: fields_vec });
        }
    }

    fn world_register_standard_only<T: Component>(&mut self, name: &str, fields: &[&str], elements: u32) {
        let id = self.app.world.register::<T>();
        if !self.app.world.schemas.iter().any(|s| s.id == id) {
            let fields_vec = fields.iter().map(|&s| s.to_string()).collect();
            self.app.world.schemas.push(crate::ecs::world::ComponentSchema { id, name: name.to_string(), array_type: 0, elements, fields: fields_vec });
        }
    }
}

#[wasm_bindgen]
impl WasmEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        crate::engine::logger::init_panic_hook();

        let mut app = App::new();
        app.add_plugin(CorePlugin);
        let mut engine = Self {
            app,
            global_resources: HashMap::new(),
            render_state: RenderState::default(),
            last_structural_gen_3d: 0,
            last_structural_gen_2d: 0,
            last_visibility_gen: 0,
        };
        engine.register_standard_schemas();
        engine
    }

    #[wasm_bindgen]
    pub fn mesh_quad_2d(&self, width: f32, height: f32) -> js_sys::Object {
        crate::engine::mesh::mesh_quad_2d(width, height)
    }

    #[wasm_bindgen]
    pub fn mesh_circle_2d(&self, segments: u32) -> js_sys::Object {
        crate::engine::mesh::mesh_circle_2d(segments)
    }

    #[wasm_bindgen]
    pub fn mesh_ring_2d(&self, inner_radius: f32, outer_radius: f32, segments: u32) -> js_sys::Object {
        crate::engine::mesh::mesh_ring_2d(inner_radius, outer_radius, segments)
    }

    #[wasm_bindgen]
    pub fn mesh_capsule_2d(&self, width: f32, height: f32, segments: u32) -> js_sys::Object {
        crate::engine::mesh::mesh_capsule_2d(width, height, segments)
    }

    #[wasm_bindgen]
    pub fn wasm_register_component(&mut self, name: &str, size: usize, align: usize, elements: u32, fields_json: &str) {
        let schema_exists = self.app.world.schemas.iter().any(|s| s.name == name);
        if schema_exists {
            return;
        }
        let id = match name {
            "Transform" => self.app.world.register_serializable::<Transform>(),
            "Transform2D" => self.app.world.register_serializable::<Transform2D>(),
            "GlobalTransform" => {
                self.app.world.register_serializable::<GlobalTransform>();
                self.app.world.register_serializable::<GPUInstanceTransform>()
            }
            "GlobalTransform2D" => self.app.world.register_serializable::<GlobalTransform2D>(),
            "GPUInstanceTransform" => self.app.world.register_serializable::<GPUInstanceTransform>(),
            "GPUDrivenSimulation" => self.app.world.register_serializable::<GPUDrivenSimulation>(),
            "Camera3D" => self.app.world.register_serializable::<Camera3D>(),
            "StandardMaterial" => self.app.world.register_serializable::<StandardMaterial>(),
            "DirectionalLight" => self.app.world.register_serializable::<DirectionalLight>(),
            "AmbientLight" => self.app.world.register_serializable::<AmbientLight>(),
            "HemisphereLight" => self.app.world.register_serializable::<HemisphereLight>(),
            "PointLight" => self.app.world.register_serializable::<PointLight>(),
            "SpriteMaterial" => self.app.world.register_serializable::<SpriteMaterial>(),
            "SpriteAnimation" => self.app.world.register_serializable::<SpriteAnimation>(),
            "MeshHandle" => self.app.world.register_serializable::<MeshHandle>(),
            "ShaderHandle" => self.app.world.register_serializable::<ShaderHandle>(),
            "Visibility" => self.app.world.register_serializable::<Visibility>(),
            "AABB" => self.app.world.register_serializable::<AABB>(),
            "AngularVelocity" => self.app.world.register_serializable::<AngularVelocity>(),
            "FlyCamera" => self.app.world.register_serializable::<FlyCamera>(),
            "Obstacle" => self.app.world.register_serializable::<Obstacle>(),
            "BoxCollider2D" => self.app.world.register_serializable::<BoxCollider2D>(),
            "CapsuleCollider2D" => self.app.world.register_serializable::<CapsuleCollider2D>(),
            "Shape2D" => self.app.world.register_serializable::<Shape2D>(),
            "PathFollower" => self.app.world.register_serializable::<PathFollower>(),
            "NetworkNode" => self.app.world.register_serializable::<NetworkNode>(),
            "Text2D" => self.app.world.register_serializable::<Text2D>(),
            _ => self.app.world.register_dynamic(name, size, align),
        };
        if !self.app.world.schemas.iter().any(|s| s.id == id) {
            let fields: Vec<String> = serde_json::from_str(fields_json).unwrap_or_default();
            self.app.world.schemas.push(crate::ecs::world::ComponentSchema { id, name: name.to_string(), array_type: 0, elements, fields });
        }
    }

    #[wasm_bindgen]
    pub fn wasm_mark_changed(&mut self, entity_id: u32, comp_id: u32) {
        if let Some(e_gen) = self.app.world.generation_of(entity_id) {
            let id = entity_id as usize;
            let rec = self.app.world.records[id];
            if rec.generation == e_gen && rec.archetype_id != u32::MAX {
                let arch = &mut self.app.world.archetypes[rec.archetype_id as usize];
                if (comp_id as usize) < arch.component_to_column.len() {
                    let col_idx = arch.component_to_column[comp_id as usize];
                    if col_idx != u32::MAX {
                        let col = unsafe { &mut *arch.columns[col_idx as usize].get() };
                        col.changed_ticks[rec.row as usize] = self.app.world.current_tick;
                        col.last_changed_tick.store(self.app.world.current_tick, std::sync::atomic::Ordering::Relaxed);
                        arch.last_modified_tick.store(self.app.world.current_tick, std::sync::atomic::Ordering::Relaxed);

                        let mesh_cid = self.app.world.get_component_id::<MeshHandle>();
                        let mat_cid = self.app.world.get_component_id::<SpriteMaterial>();
                        let smat_cid = self.app.world.get_component_id::<StandardMaterial>();
                        let shape_cid = self.app.world.get_component_id::<Shape2D>();
                        let text_cid = self.app.world.get_component_id::<Text2D>();
                        if Some(comp_id as usize) == mesh_cid || Some(comp_id as usize) == mat_cid || Some(comp_id as usize) == smat_cid || Some(comp_id as usize) == shape_cid || Some(comp_id as usize) == text_cid {
                            self.app.world.structural_gen += 1;
                        }
                    }
                }
            }
        }
    }

    #[wasm_bindgen]
    pub fn wasm_spawn_batch_with(&mut self, count: u32, names: Vec<String>) -> Vec<u32> {
        let mut ids = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let e = self.app.world.spawn();
            for name in &names {
                self.wasm_add_component(e.id, name);
            }
            ids.push(e.id);
        }
        ids
    }

    #[wasm_bindgen]
    pub fn wasm_add_component(&mut self, entity_id: u32, name: &str) {
        if let Some(e_gen) = self.app.world.generation_of(entity_id) {
            let e = crate::ecs::entity::Entity { id: entity_id, generation: e_gen };
            let schema = self.app.world.schemas.iter().find(|s| s.name == name);
            if let Some(s) = schema {
                let comp_id = s.id;
                match name {
                    "Transform" => self.app.world.add_component(e, Transform::default()),
                    "Transform2D" => self.app.world.add_component(e, Transform2D::default()),
                    "GlobalTransform" => {
                        self.app.world.add_component(e, GlobalTransform::default());
                        self.app.world.add_component(e, GPUInstanceTransform::default());
                    }
                    "GlobalTransform2D" => self.app.world.add_component(e, GlobalTransform2D::default()),
                    "GPUInstanceTransform" => self.app.world.add_component(e, GPUInstanceTransform::default()),
                    "Camera3D" => self.app.world.add_component(e, Camera3D::default()),
                    "StandardMaterial" => self.app.world.add_component(e, StandardMaterial::default()),
                    "DirectionalLight" => self.app.world.add_component(e, DirectionalLight::default()),
                    "AmbientLight" => self.app.world.add_component(e, AmbientLight::default()),
                    "HemisphereLight" => self.app.world.add_component(e, HemisphereLight::default()),
                    "PointLight" => self.app.world.add_component(e, PointLight::default()),
                    "SpriteMaterial" => self.app.world.add_component(e, SpriteMaterial::default()),
                    "SpriteAnimation" => self.app.world.add_component(e, SpriteAnimation::default()),
                    "MeshHandle" => self.app.world.add_component(e, MeshHandle::default()),
                    "ShaderHandle" => self.app.world.add_component(e, ShaderHandle::default()),
                    "Visibility" => self.app.world.add_component(e, Visibility::default()),
                    "AABB" => self.app.world.add_component(e, AABB::default()),
                    "AngularVelocity" => self.app.world.add_component(e, AngularVelocity::default()),
                    "FlyCamera" => self.app.world.add_component(e, FlyCamera::default()),
                    "GPUDrivenSimulation" => self.app.world.add_component(e, GPUDrivenSimulation::default()),
                    "Obstacle" => self.app.world.add_component(e, Obstacle::default()),
                    "BoxCollider2D" => self.app.world.add_component(e, BoxCollider2D::default()),
                    "CapsuleCollider2D" => self.app.world.add_component(e, CapsuleCollider2D::default()),
                    "Shape2D" => self.app.world.add_component(e, Shape2D::default()),
                    "PathFollower" => self.app.world.add_component(e, PathFollower::default()),
                    "NetworkNode" => self.app.world.add_component(e, NetworkNode::default()),
                    "Text2D" => self.app.world.add_component(e, Text2D::default()),
                    _ => {
                        let layout = self.app.world.component_layouts[comp_id];
                        let data = vec![0u8; layout.size()];
                        self.app.world.add_component_raw(e, comp_id, data.as_ptr());
                    }
                }
            }
        }
    }

    #[wasm_bindgen]
    pub fn wasm_remove_component(&mut self, entity_id: u32, name: &str) {
        if let Some(e_gen) = self.app.world.generation_of(entity_id) {
            let e = crate::ecs::entity::Entity { id: entity_id, generation: e_gen };
            let schema = self.app.world.schemas.iter().find(|s| s.name == name);
            if let Some(s) = schema {
                match name {
                    "GlobalTransform" => {
                        self.app.world.remove_component_raw(e, s.id);
                        if let Some(gpu_s) = self.app.world.schemas.iter().find(|sc| sc.name == "GPUInstanceTransform") {
                            self.app.world.remove_component_raw(e, gpu_s.id);
                        }
                    }
                    _ => {
                        self.app.world.remove_component_raw(e, s.id);
                    }
                }
            }
        }
    }

    #[wasm_bindgen]
    pub fn wasm_set_dynamic_mesh(&mut self, entity_id: u32, vertices: &[f32], indices: &[u32]) {
        if let Some(e_gen) = self.app.world.generation_of(entity_id) {
            let entity = crate::ecs::Entity { id: entity_id, generation: e_gen };
            if self.app.world.get_component::<DynamicMesh>(entity).is_none() {
                self.app.world.add_component(entity, DynamicMesh::default());
            }
            if let Some(mesh) = self.app.world.get_component_mut::<DynamicMesh>(entity) {
                mesh.vertices = vertices.to_vec();
                mesh.indices = indices.to_vec();
                mesh.version = mesh.version.wrapping_add(1);
            }
        }
    }

    #[wasm_bindgen]
    pub fn wasm_set_mesh_handle(&mut self, entity_id: u32, mesh_id: u32) {
        if let Some(e_gen) = self.app.world.generation_of(entity_id) {
            let entity = crate::ecs::Entity { id: entity_id, generation: e_gen };
            if self.app.world.get_component::<MeshHandle>(entity).is_none() {
                self.app.world.add_component(entity, MeshHandle::default());
            }
            if let Some(handle) = self.app.world.get_component_mut::<MeshHandle>(entity) {
                handle.id = mesh_id as f32;
            }
        }
    }

    #[wasm_bindgen]
    pub fn wasm_set_parent(&mut self, child_id: u32, parent_id: u32) {
        let child_gen = self.app.world.generation_of(child_id);
        let parent_gen = if parent_id == u32::MAX { None } else { self.app.world.generation_of(parent_id) };

        if let Some(c_gen) = child_gen {
            let child = crate::ecs::Entity { id: child_id, generation: c_gen };
            let parent_opt = if let Some(p_gen) = parent_gen {
                Some(crate::ecs::Entity { id: parent_id, generation: p_gen })
            } else { None };

            self.app.world.set_parent(child, parent_opt);
        }
    }

    #[wasm_bindgen]
    pub fn wasm_find_path(&self, sx: i32, sy: i32, ex: i32, ey: i32, diag: bool, max_iter: usize) -> js_sys::Int32Array {
        if let Some(path) = crate::engine::spatial::find_path_astar(&self.app.world, (sx, sy), (ex, ey), diag, max_iter) {
            let mut flat = Vec::with_capacity(path.len() * 2);
            for p in path {
                flat.push(p.0);
                flat.push(p.1);
            }
            js_sys::Int32Array::from(flat.as_slice())
        } else {
            js_sys::Int32Array::new_with_length(0)
        }
    }

    #[wasm_bindgen] pub fn spawn(&mut self) -> u32 { self.app.world.spawn().id }
    #[wasm_bindgen] pub fn wasm_spawn_batch(&mut self, count: u32) -> Vec<u32> {
        let mut ids = Vec::with_capacity(count as usize);
        for _ in 0..count { ids.push(self.spawn()); }
        ids
    }
    #[wasm_bindgen] pub fn kill(&mut self, id: u32) {
        if let Some(e_gen) = self.app.world.generation_of(id) { self.app.world.kill(crate::ecs::entity::Entity { id, generation: e_gen }); }
    }
    #[wasm_bindgen] pub fn tick(&mut self, dt: f32) {
        if let Some(t) = self.app.world.get_resource_mut::<Time>() {
            t.delta_seconds = dt;
            t.elapsed_seconds += dt;
        }
        self.app.update();
    }

    #[wasm_bindgen] pub fn render_tick(&mut self) { self.app.render_update(); }

    #[wasm_bindgen] pub fn get_structural_gen(&self) -> u32 { self.app.world.structural_gen as u32 }

    #[wasm_bindgen]
    pub fn wasm_update_input(
        &mut self,
        keys: Option<js_sys::Uint8Array>,
        mouse_x: f32,
        mouse_y: f32,
        mouse_dx: f32,
        mouse_dy: f32,
        wheel_dy: f32,
        mouse_buttons: Option<js_sys::Uint8Array>
    ) {
        if let Some(input) = self.app.world.get_resource_mut::<crate::engine::input::Input>() {
            input.previous_keys.copy_from_slice(&input.current_keys);
            input.previous_mouse_buttons.copy_from_slice(&input.current_mouse_buttons);

            if let Some(keys_arr) = keys {
                let mut buf = [0u8; 64];
                let len = (keys_arr.length() as usize).min(64);
                keys_arr.slice(0, len as u32).copy_to(&mut buf[..len]);
                for i in 0..len.min(input.current_keys.len()) {
                    input.current_keys[i] = buf[i] != 0;
                }
            }

            input.mouse_x = mouse_x;
            input.mouse_y = mouse_y;
            input.mouse_dx = mouse_dx;
            input.mouse_dy = mouse_dy;
            input.mouse_wheel_delta = wheel_dy;

            if let Some(buttons_arr) = mouse_buttons {
                let mut buf = [0u8; 5];
                let len = (buttons_arr.length() as usize).min(5);
                buttons_arr.slice(0, len as u32).copy_to(&mut buf[..len]);
                for i in 0..len.min(input.current_mouse_buttons.len()) {
                    input.current_mouse_buttons[i] = buf[i] != 0;
                }
            }
        }
    }

    #[wasm_bindgen]
    pub fn wasm_screen_to_ray(&self, ndc_x: f32, ndc_y: f32) -> js_sys::Float32Array {
        let cam_cid = self.app.world.get_component_id::<Camera3D>();
        let gt_cid = self.app.world.get_component_id::<GlobalTransform>();
        if let (Some(cid), Some(gtid)) = (cam_cid, gt_cid) {
            for arch in &self.app.world.archetypes {
                if arch.entities.is_empty() { continue; }
                if cid < arch.component_to_column.len() && gtid < arch.component_to_column.len() {
                    let c_col = arch.component_to_column[cid];
                    let gt_col = arch.component_to_column[gtid];
                    if c_col != u32::MAX && gt_col != u32::MAX {
                        let cams_ptr = unsafe { (*arch.columns[c_col as usize].get()).data.as_ptr::<Camera3D>() };
                        let cam = unsafe { &*cams_ptr.add(0) };
                        let vp = glam::Mat4::from_cols_array(&cam.view_proj);
                        let inv_vp = vp.inverse();

                        let p0 = inv_vp * glam::Vec4::new(ndc_x, ndc_y, 0.1, 1.0);
                        let p1 = inv_vp * glam::Vec4::new(ndc_x, ndc_y, 0.9, 1.0);

                        let origin = p0.truncate() / p0.w;
                        let target = p1.truncate() / p1.w;
                        let dir = (target - origin).normalize();

                        let arr = [origin.x, origin.y, origin.z, dir.x, dir.y, dir.z];
                        return js_sys::Float32Array::from(&arr[..]);
                    }
                }
            }
        }
        js_sys::Float32Array::new_with_length(0)
    }

    #[wasm_bindgen]
    pub fn wasm_raycast_3d(&self, ox: f32, oy: f32, oz: f32, dx: f32, dy: f32, dz: f32) -> js_sys::Float32Array {
        use crate::engine::math::Ray;
        use glam::Vec3;
        let dir_unnorm = Vec3::new(dx, dy, dz);
        let ray = Ray::new(Vec3::new(ox, oy, oz), dir_unnorm.normalize());
        if let Some((ent, t, tri)) = crate::engine::spatial_3d::world_raycast(&self.app.world, &ray) {
            let arr = [ent.id as f32, t, tri as f32];
            js_sys::Float32Array::from(&arr[..])
        } else {
            js_sys::Float32Array::new_with_length(0)
        }
    }

    #[wasm_bindgen]
    pub fn wasm_query_at(&self, x: f32, y: f32, radius: f32) -> Vec<u32> {
        let mut results = Vec::new();
        let mut candidates = Vec::new();
        if let Some(grid) = self.app.world.get_resource::<crate::engine::spatial::SpatialGrid>() {
            grid.query_radius(x, y, radius + 100.0, &mut candidates);
            let t_id = self.app.world.get_component_id::<Transform2D>();
            let c_id = self.app.world.get_component_id::<Collider>();
            let b_id = self.app.world.get_component_id::<BoxCollider2D>();
            let _cap_id = self.app.world.get_component_id::<CapsuleCollider2D>();

            for e in candidates {
                if let Some(tid) = t_id {
                    if let Some(pos) = self.app.world.get_component_by_id::<Transform2D>(e, tid) {
                        let dx = pos.x - x;
                        let dy = pos.y - y;
                        let dist_sq = dx * dx + dy * dy;

                        let mut hit = false;
                        if let Some(cid) = c_id {
                            let col_rad = self.app.world.get_component_by_id::<Collider>(e, cid).map(|c| c.radius).unwrap_or(0.0);
                            let combined_rad = col_rad + radius;
                            if dist_sq <= combined_rad * combined_rad { hit = true; }
                        }
                        if !hit && b_id.is_some() {
                            if let Some(bcol) = self.app.world.get_component_by_id::<BoxCollider2D>(e, b_id.unwrap()) {
                                let c = pos.rotation.cos();
                                let s = pos.rotation.sin();
                                let local_x = dx * c + dy * s;
                                let local_y = -dx * s + dy * c;
                                if local_x.abs() <= bcol.half_x + radius && local_y.abs() <= bcol.half_y + radius {
                                    hit = true;
                                }
                            }
                        }
                        if hit { results.push(e.id); }
                    }
                }
            }
        }
        results
    }

    #[wasm_bindgen]
    pub fn wasm_pick_2d(&self, x: f32, y: f32) -> u32 {
        let mut candidates = Vec::new();
        if let Some(grid) = self.app.world.get_resource::<crate::engine::spatial::SpatialGrid>() {
            grid.query_radius(x, y, 100.0, &mut candidates);

            let t_id = self.app.world.get_component_id::<Transform2D>();
            let c_id = self.app.world.get_component_id::<Collider>();
            let box_id = self.app.world.get_component_id::<BoxCollider2D>();

            let mut closest_dist = f32::MAX;
            let mut closest_ent = u32::MAX;

            for e in candidates {
                if let Some(pos) = self.app.world.get_component_by_id::<Transform2D>(e, t_id.unwrap_or(usize::MAX)) {
                    let dx = pos.x - x;
                    let dy = pos.y - y;
                    let dist = (dx*dx + dy*dy).sqrt();

                    if let Some(cid) = c_id {
                        if let Some(col) = self.app.world.get_component_by_id::<Collider>(e, cid) {
                            if dist <= col.radius && dist < closest_dist {
                                closest_dist = dist;
                                closest_ent = e.id;
                            }
                        }
                    }
                    if let Some(bid) = box_id {
                        if let Some(bcol) = self.app.world.get_component_by_id::<BoxCollider2D>(e, bid) {
                            let c = pos.rotation.cos();
                            let s = pos.rotation.sin();
                            let local_x = dx * c + dy * s;
                            let local_y = -dx * s + dy * c;
                            if local_x.abs() <= bcol.half_x && local_y.abs() <= bcol.half_y {
                                if dist < closest_dist {
                                    closest_dist = dist;
                                    closest_ent = e.id;
                                }
                            }
                        }
                    }
                }
            }
            closest_ent
        } else {
            u32::MAX
        }
    }

    #[wasm_bindgen]
    pub fn wasm_query_at_3d(&self, x: f32, y: f32, z: f32, radius: f32) -> Vec<u32> {
        let mut results = Vec::new();
        let mut candidates = Vec::new();
        if let Some(grid) = self.app.world.get_resource::<crate::engine::spatial_3d::SpatialGrid3D>() {
            grid.query_radius(x, y, z, radius + 100.0, &mut candidates);
            let t_id = self.app.world.get_component_id::<Transform>();
            let c_id = self.app.world.get_component_id::<Collider3D>();
            if let (Some(tid), Some(cid)) = (t_id, c_id) {
                for e in candidates {
                    if let Some(pos) = self.app.world.get_component_by_id::<Transform>(e, tid) {
                        let col_rad = self.app.world.get_component_by_id::<Collider3D>(e, cid).map(|c| c.radius).unwrap_or(0.0);
                        let dx = pos.translation[0] - x;
                        let dy = pos.translation[1] - y;
                        let dz = pos.translation[2] - z;
                        let dist_sq = dx * dx + dy * dy + dz * dz;
                        let combined_rad = col_rad + radius;
                        if dist_sq <= combined_rad * combined_rad {
                            results.push(e.id);
                        }
                    }
                }
            }
        }
        results
    }

    #[wasm_bindgen]
    pub fn wasm_tile_get(&self, x: i32, y: i32, layer: u8) -> u32 {
        self.app.world.get_resource::<crate::engine::spatial::TileGrid>()
            .and_then(|g| g.get_entity_at_tile(x, y, layer))
            .map(|e| e.id)
            .unwrap_or(u32::MAX)
    }

    #[wasm_bindgen]
    pub fn wasm_tile_set(&mut self, x: i32, y: i32, layer: u8, entity_id: u32) {
        if let Some(entity_gen) = self.app.world.generation_of(entity_id) {
            if let Some(grid) = self.app.world.get_resource_mut::<crate::engine::spatial::TileGrid>() {
                grid.set_entity_at_tile(x, y, layer, crate::ecs::Entity { id: entity_id, generation: entity_gen });
            }
        }
    }

    #[wasm_bindgen]
    pub fn wasm_tile_remove(&mut self, x: i32, y: i32, layer: u8) {
        if let Some(grid) = self.app.world.get_resource_mut::<crate::engine::spatial::TileGrid>() {
            grid.remove_entity_at_tile(x, y, layer);
        }
    }

    #[wasm_bindgen]
    pub fn get_component_ptr(&self, entity_id: u32, name: &str) -> u32 {
        let id = entity_id as usize;
        let rec = self.app.world.records[id];
        let cid = self.app.world.schemas.iter().find(|s| s.name == name).map(|s| s.id);
        if let (Some(cid), Some(arch)) = (cid, self.app.world.archetypes.get(rec.archetype_id as usize)) {
            let col_idx = arch.component_to_column[cid];
            if col_idx != u32::MAX {
                let col = unsafe { &*arch.columns[col_idx as usize].get() };
                return col.data.as_ptr::<u8>() as u32 + (rec.row * col.data.item_size() as u32);
            }
        }
        0
    }

    #[wasm_bindgen]
    pub fn wasm_get_tilemap_data(&self, _entity_id: u32) -> js_sys::Uint32Array {
        js_sys::Uint32Array::new_with_length(0)
    }

    #[wasm_bindgen]
    pub fn wasm_get_inventory(&self, _entity_id: u32) -> js_sys::Float32Array {
        js_sys::Float32Array::new_with_length(0)
    }

    #[wasm_bindgen]
    pub fn wasm_get_render_batches_2d(&mut self) -> js_sys::Uint32Array {
        let current_gen = self.app.world.structural_gen;
        if current_gen != self.last_structural_gen_2d {
            self.render_state.draw_calls.clear();
            let gt2d_cid = self.app.world.get_component_id::<GlobalTransform2D>();
            let mat_cid = self.app.world.get_component_id::<SpriteMaterial>();
            let mesh_cid = self.app.world.get_component_id::<MeshHandle>();
            if let (Some(gt2d_id), Some(mat_id), Some(mesh_id)) = (gt2d_cid, mat_cid, mesh_cid) {
                for arch in &self.app.world.archetypes {
                    let len = arch.entities.len();
                    if len == 0 { continue; }
                    if gt2d_id >= arch.component_to_column.len() || mat_id >= arch.component_to_column.len() || mesh_id >= arch.component_to_column.len() { continue; }
                    let gt2d_col = arch.component_to_column[gt2d_id];
                    let mat_col = arch.component_to_column[mat_id];
                    let mesh_col = arch.component_to_column[mesh_id];
                    if gt2d_col != u32::MAX && mat_col != u32::MAX && mesh_col != u32::MAX {
                        let gt2d_base = unsafe { (*arch.columns[gt2d_col as usize].get()).data.as_ptr::<GlobalTransform2D>() };
                        let mat_base = unsafe { (*arch.columns[mat_col as usize].get()).data.as_ptr::<SpriteMaterial>() };
                        let mesh_base = unsafe { (*arch.columns[mesh_col as usize].get()).data.as_ptr::<MeshHandle>() };
                        let mut run_start = u32::MAX;
                        let mut current_tex = u32::MAX;
                        let mut current_mesh = u32::MAX;
                        for i in 0..len {
                            let _tex = unsafe { (*mat_base.add(i)).texture_id as u32 };
                            let mesh = unsafe { (*mesh_base.add(i)).id as u32 };
                            if run_start == u32::MAX {
                                run_start = i as u32; current_tex = _tex; current_mesh = mesh;
                            } else if _tex != current_tex || mesh != current_mesh {
                                self.render_state.draw_calls.push(unsafe { gt2d_base.add(run_start as usize) } as u32);
                                self.render_state.draw_calls.push(unsafe { mat_base.add(run_start as usize) } as u32);
                                self.render_state.draw_calls.push(current_tex);
                                self.render_state.draw_calls.push(current_mesh);
                                self.render_state.draw_calls.push(i as u32 - run_start);
                                run_start = i as u32; current_tex = _tex; current_mesh = mesh;
                            }
                        }
                        if run_start != u32::MAX {
                            self.render_state.draw_calls.push(unsafe { gt2d_base.add(run_start as usize) } as u32);
                            self.render_state.draw_calls.push(unsafe { mat_base.add(run_start as usize) } as u32);
                            self.render_state.draw_calls.push(current_tex);
                            self.render_state.draw_calls.push(current_mesh);
                            self.render_state.draw_calls.push(len as u32 - run_start);
                        }
                    }
                }
            }
            self.last_structural_gen_2d = current_gen;
        }
        unsafe { js_sys::Uint32Array::view(&self.render_state.draw_calls[..]) }
    }

    #[wasm_bindgen]
    pub fn wasm_get_render_batches_3d(&mut self) -> js_sys::Uint32Array {
        let current_gen = self.app.world.structural_gen;

        let vis_gen = self
            .app
            .world
            .get_resource::<crate::engine::VisibilityGen>()
            .map(|g| g.get())
            .unwrap_or(0);
        if current_gen != self.last_structural_gen_3d || vis_gen != self.last_visibility_gen {
            self.get_batches_3d_internal();
            self.last_structural_gen_3d = current_gen;
            self.last_visibility_gen = vis_gen;
        }
        unsafe { js_sys::Uint32Array::view(&self.render_state.draw_calls[..]) }
    }

    #[wasm_bindgen]
    pub fn wasm_get_light_data(&mut self) -> js_sys::Float32Array {
        self.render_state.light_data.clear();
        let gt_cid = self.app.world.get_component_id::<GlobalTransform>();
        let pl_cid = self.app.world.get_component_id::<PointLight>();

        if let (Some(gt_id), Some(pl_id)) = (gt_cid, pl_cid) {
            for arch in &self.app.world.archetypes {
                if gt_id < arch.component_to_column.len() && pl_id < arch.component_to_column.len() {
                    let gt_col = arch.component_to_column[gt_id];
                    let pl_col = arch.component_to_column[pl_id];
                    if gt_col != u32::MAX && pl_col != u32::MAX {
                        let gts = unsafe { (*arch.columns[gt_col as usize].get()).data.as_ptr::<GlobalTransform>() };
                        let pls = unsafe { (*arch.columns[pl_col as usize].get()).data.as_ptr::<PointLight>() };
                        for i in 0..arch.entities.len() {
                            let gt = unsafe { &*gts.add(i) };
                            let pl = unsafe { &*pls.add(i) };
                            let pos = gt.matrix.w_axis;
                            self.render_state.light_data.extend_from_slice(&[
                                pos.x, pos.y, pos.z, pl.intensity,
                                pl.color[0], pl.color[1], pl.color[2], pl.range
                            ]);
                        }
                    }
                }
            }
        }
        unsafe { js_sys::Float32Array::view(&self.render_state.light_data[..]) }
    }

    #[wasm_bindgen]
    pub fn wasm_save_world(&self) -> String { self.app.world.save_to_json() }

    #[wasm_bindgen]
    pub fn wasm_load_world(&mut self, json: &str) { self.app.world.load_from_json(json); }

    #[wasm_bindgen]
    pub fn set_resource(&mut self, name: &str, data: &[f32]) { self.global_resources.insert(name.to_string(), data.to_vec()); }

    #[wasm_bindgen]
    pub fn get_resource(&self, name: &str) -> Option<Vec<f32>> { self.global_resources.get(name).cloned() }

    #[wasm_bindgen] pub fn get_component_schemas(&self) -> js_sys::Array {
        let arr = js_sys::Array::new();
        for s in &self.app.world.schemas {
            let obj = js_sys::Object::new();
            js_sys::Reflect::set(&obj, &"id".into(), &(s.id as f64).into()).unwrap();
            js_sys::Reflect::set(&obj, &"name".into(), &s.name.clone().into()).unwrap();
            js_sys::Reflect::set(&obj, &"elements".into(), &(s.elements as f64).into()).unwrap();
            let fields_arr = js_sys::Array::new();
            for f in &s.fields {
                fields_arr.push(&js_sys::JsString::from(f.as_str()));
            }
            js_sys::Reflect::set(&obj, &"fields".into(), &fields_arr).unwrap();
            arr.push(&obj);
        }
        arr
    }

    #[wasm_bindgen]
    pub fn wasm_get_dynamic_mesh_vertices(&self, entity_id: u32) -> js_sys::Float32Array {
        if let Some(e_gen) = self.app.world.generation_of(entity_id) {
            if let Some(mesh) = self.app.world.get_component::<crate::engine::component::DynamicMesh>(crate::ecs::Entity { id: entity_id, generation: e_gen }) {
                return unsafe { js_sys::Float32Array::view(&mesh.vertices) };
            }
        }
        js_sys::Float32Array::new_with_length(0)
    }

    #[wasm_bindgen]
    pub fn wasm_get_dynamic_mesh_indices(&self, entity_id: u32) -> js_sys::Uint32Array {
        if let Some(e_gen) = self.app.world.generation_of(entity_id) {
            if let Some(mesh) = self.app.world.get_component::<crate::engine::component::DynamicMesh>(crate::ecs::Entity { id: entity_id, generation: e_gen }) {
                return unsafe { js_sys::Uint32Array::view(&mesh.indices) };
            }
        }
        js_sys::Uint32Array::new_with_length(0)
    }

    #[wasm_bindgen]
    pub fn wasm_get_dynamic_mesh_version(&self, entity_id: u32) -> u32 {
        if let Some(e_gen) = self.app.world.generation_of(entity_id) {
            if let Some(mesh) = self.app.world.get_component::<crate::engine::component::DynamicMesh>(crate::ecs::Entity { id: entity_id, generation: e_gen }) {
                return mesh.version;
            }
        }
        0
    }

    #[wasm_bindgen]
    pub fn wasm_get_dynamic_mesh_color_version(&self, entity_id: u32) -> u32 {
        if let Some(e_gen) = self.app.world.generation_of(entity_id) {
            if let Some(mesh) = self.app.world.get_component::<crate::engine::component::DynamicMesh>(crate::ecs::Entity { id: entity_id, generation: e_gen }) {
                return mesh.color_version;
            }
        }
        0
    }

    #[wasm_bindgen] pub fn get_active_archetypes(&self) -> Vec<u32> { self.app.world.active_archetypes.clone() }
    #[wasm_bindgen] pub fn get_archetype_memory_layout(&self, id: u32) -> js_sys::Uint32Array {
        let arch = &self.app.world.archetypes[id as usize];
        let mut data = vec![arch.entities.len() as u32, arch.entities.capacity() as u32, arch.entities.as_ptr() as u32, arch.signature.len() as u32];
        for (cid, &col) in arch.component_to_column.iter().enumerate() {
            if col != u32::MAX {
                data.push(cid as u32);
                data.push(unsafe { (*arch.columns[col as usize].get()).data.get_raw(0) } as u32);
            }
        }
        js_sys::Uint32Array::from(&data[..])
    }
    #[wasm_bindgen] pub fn memory() -> JsValue { wasm_bindgen::memory() }
}

#[wasm_bindgen]
pub fn artisan_bench_manifest() -> String {
    serde_json::to_string(&crate::bench::manifest()).unwrap_or_else(|_| "[]".into())
}

#[wasm_bindgen]
pub fn artisan_bench_run(id: u32, cfg_json: &str) -> String {
    let cfg: crate::bench::BenchCfg = serde_json::from_str(cfg_json)
        .unwrap_or_else(|_| crate::bench::BenchCfg::default());
    match crate::bench::run_case(id, &cfg) {
        Some(r) => serde_json::to_string(&r).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")),
        None => format!("{{\"error\":\"unknown case id {id}\"}}"),
    }
}

#[wasm_bindgen]
pub fn artisan_bench_env() -> String {
    serde_json::to_string(&crate::bench::BenchEnv::capture()).unwrap_or_else(|_| "{}".into())
}

#[wasm_bindgen]
pub fn artisan_rayon_threads() -> usize {
    rayon::current_num_threads()
}

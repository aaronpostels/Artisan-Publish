use wasm_bindgen::prelude::*;
use artisan::{WasmEngine, register_component_schema};
use artisan::ecs::{Query, Entity, Parent, Children, Without, Res};
use artisan::ecs::world::World;
use artisan::engine::component::{DirectionalLight, HemisphereLight, Camera3D, Transform, GlobalTransform, DynamicMesh, StandardMaterial, MeshBVH, AmbientLight, Billboard};
use artisan::engine::math::hex_to_linear_rgb;
use artisan::engine::Time;
use glam::Vec3;

pub mod components;
pub mod systems;

use components::{AtmosphereConfig, AtmosphereHalo, PlanetConfig, PlanetSimulationState, SpaceRotation, SpaceRotationTilt, NebulaRotation};
use systems::{sys_generate_atmosphere_mesh, sys_generate_planet_mesh, sys_update_atmosphere_halo, visualization_color};

struct Lcg {
    seed: u32,
}

impl Lcg {
    fn new(seed: u32) -> Self {
        Self { seed }
    }
    fn next_f32(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1103515245).wrapping_add(12345) & 0x7FFFFFFF;
        (self.seed as f32) / 2147483648.0
    }
}

fn find_planet_state(world: &World) -> Option<&PlanetSimulationState> {
    let comp_id = world.get_component_id::<PlanetSimulationState>()?;
    for arch in &world.archetypes {
        if comp_id < arch.component_to_column.len() && arch.component_to_column[comp_id] != u32::MAX {
            let col = unsafe { &*arch.columns[arch.component_to_column[comp_id] as usize].get() };
            if !col.data.is_empty() {
                return Some(unsafe { &*col.data.as_ptr::<PlanetSimulationState>() });
            }
        }
    }
    None
}

#[wasm_bindgen]
pub fn wasm_get_face_info(engine: &WasmEngine, face_id: usize) -> Vec<f32> {
    let world = &engine.app().world;
    let mut info = vec![0.0; 6];
    if let Some(state) = find_planet_state(world) {
        if face_id < state.is_water.len() {
            info[0] = state.is_water[face_id];
            info[1] = state.elevations[face_id];
            info[2] = state.temps[face_id];
            info[3] = state.moistures[face_id];
            info[4] = state.arability[face_id];
            info[5] = state.minerals[face_id];
        }
    }
    info
}

fn find_planet_mut(world: &mut World) -> Option<(*const PlanetSimulationState, *mut DynamicMesh)> {
    let state_id = world.get_component_id::<PlanetSimulationState>()?;
    let mesh_id = world.get_component_id::<DynamicMesh>()?;
    for arch in &world.archetypes {
        if arch.entities.is_empty() { continue; }
        if state_id >= arch.component_to_column.len() || mesh_id >= arch.component_to_column.len() { continue; }
        let s_col = arch.component_to_column[state_id];
        let m_col = arch.component_to_column[mesh_id];
        if s_col == u32::MAX || m_col == u32::MAX { continue; }
        let state_ptr = unsafe { (*arch.columns[s_col as usize].get()).data.as_ptr::<PlanetSimulationState>() };
        let mesh_ptr = unsafe { (*arch.columns[m_col as usize].get()).data.as_ptr::<DynamicMesh>() as *mut DynamicMesh };
        return Some((state_ptr, mesh_ptr));
    }
    None
}

#[wasm_bindgen]
pub fn wasm_select_face(engine: &mut WasmEngine, prev_face_id: i32, face_id: i32) {
    let world = &mut engine.app_mut().world;
    let (state_ptr, mesh_ptr) = match find_planet_mut(world) {
        Some(p) => p,
        None => return,
    };
    let state = unsafe { &*state_ptr };
    let mesh = unsafe { &mut *mesh_ptr };
    let num_faces = mesh.indices.len() / 3;

    let set_face_color = |mesh: &mut DynamicMesh, f: usize, color: [f32; 3]| {
        for k in 0..3 {
            let vi = mesh.indices[f * 3 + k] as usize;
            let vo = vi * 12;
            mesh.vertices[vo + 8] = color[0];
            mesh.vertices[vo + 9] = color[1];
            mesh.vertices[vo + 10] = color[2];
        }
    };

    if prev_face_id >= 0 && (prev_face_id as usize) < num_faces {
        let f = prev_face_id as usize;
        let vi0 = mesh.indices[f * 3] as usize;
        let base = [state.base_colors[vi0 * 3], state.base_colors[vi0 * 3 + 1], state.base_colors[vi0 * 3 + 2]];
        set_face_color(mesh, f, base);
    }
    if face_id >= 0 && (face_id as usize) < num_faces {
        let f = face_id as usize;
        let vi0 = mesh.indices[f * 3] as usize;
        let base = [state.base_colors[vi0 * 3], state.base_colors[vi0 * 3 + 1], state.base_colors[vi0 * 3 + 2]];
        let highlight = [0.3, 0.95, 1.0];
        let blend = 0.65;
        let color = [
            base[0] + (highlight[0] - base[0]) * blend,
            base[1] + (highlight[1] - base[1]) * blend,
            base[2] + (highlight[2] - base[2]) * blend,
        ];
        set_face_color(mesh, f, color);
    }
    mesh.version = mesh.version.wrapping_add(1);
}

#[wasm_bindgen]
pub fn wasm_set_visualization(engine: &mut WasmEngine, mode: u32) {
    let world = &mut engine.app_mut().world;
    let (state_ptr, mesh_ptr) = match find_planet_mut(world) {
        Some(value) => value,
        None => return,
    };
    let state = unsafe { &*state_ptr };
    let mesh = unsafe { &mut *mesh_ptr };
    let face_count = mesh.indices.len() / 3;

    for face in 0..face_count {
        let vertex = mesh.indices[face * 3] as usize;
        let biome = [
            state.base_colors[vertex * 3],
            state.base_colors[vertex * 3 + 1],
            state.base_colors[vertex * 3 + 2],
        ];
        let color = visualization_color(
            mode,
            state.elevations[face],
            state.temps[face],
            state.moistures[face],
            state.is_water[face],
            state.arability[face],
            state.minerals[face],
            biome,
        );
        for corner in 0..3 {
            let offset = mesh.indices[face * 3 + corner] as usize * 12;
            mesh.vertices[offset + 8] = color[0];
            mesh.vertices[offset + 9] = color[1];
            mesh.vertices[offset + 10] = color[2];
        }
    }
    mesh.color_version = mesh.color_version.wrapping_add(1);
}

fn generate_star_layer_mesh(count: usize, size: f32, color_hex: u32, opacity: f32, r_min: f32, r_max: f32, rng: &mut Lcg) -> DynamicMesh {
    let mut vertices = Vec::with_capacity(count * 4 * 12);
    let mut indices = Vec::with_capacity(count * 6);
    let base_color = hex_to_linear_rgb(color_hex);

    for idx in 0..count {
        let r = r_min + rng.next_f32() * (r_max - r_min);
        let theta = rng.next_f32() * 2.0 * std::f32::consts::PI;
        let phi = (2.0 * rng.next_f32() - 1.0).acos();

        let px = r * phi.sin() * theta.cos();
        let py = r * phi.sin() * theta.sin();
        let pz = r * phi.cos();

        let p = Vec3::new(px, py, pz);
        let n = p.normalize();

        let u_temp = if n.x.abs() > 0.9 { Vec3::Y } else { Vec3::X };
        let v = n.cross(u_temp).normalize();
        let u = v.cross(n).normalize();

        let hs = size * 0.5;
        let p0 = p - u * hs - v * hs;
        let p1 = p + u * hs - v * hs;
        let p2 = p + u * hs + v * hs;
        let p3 = p - u * hs + v * hs;

        let mut col = base_color;
        let lerp_t = rng.next_f32() * 0.4;
        col[0] += (1.0 - col[0]) * lerp_t;
        col[1] += (1.0 - col[1]) * lerp_t;
        col[2] += (1.0 - col[2]) * lerp_t;

        let uv_coords = [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ];

        let pts = [p0, p1, p2, p3];
        for (i, pt) in pts.iter().enumerate() {
            vertices.push(pt.x);
            vertices.push(pt.y);
            vertices.push(pt.z);

            vertices.push(n.x);
            vertices.push(n.y);
            vertices.push(n.z);

            vertices.push(uv_coords[i][0]);
            vertices.push(uv_coords[i][1]);

            vertices.push(col[0]);
            vertices.push(col[1]);
            vertices.push(col[2]);
            vertices.push(opacity);
        }

        let start_idx = (idx * 4) as u32;
        indices.push(start_idx);
        indices.push(start_idx + 1);
        indices.push(start_idx + 2);
        indices.push(start_idx + 2);
        indices.push(start_idx + 3);
        indices.push(start_idx);
    }

    DynamicMesh {
        vertices,
        indices,
        version: 0,
        color_version: 0,
    }
}

fn generate_galaxy_ring_mesh(count: usize, size: f32, color_hex: u32, opacity: f32, r_min: f32, r_max: f32, thickness_spread: f32, rng: &mut Lcg) -> DynamicMesh {
    let mut vertices = Vec::with_capacity(count * 4 * 12);
    let mut indices = Vec::with_capacity(count * 6);
    let base_color = hex_to_linear_rgb(color_hex);
    let target_color = hex_to_linear_rgb(0xaaccff);

    for idx in 0..count {
        let r = r_min + rng.next_f32() * (r_max - r_min);
        let theta = rng.next_f32() * 2.0 * std::f32::consts::PI;
        let thickness = (rng.next_f32() - 0.5) * thickness_spread;

        let px = r * theta.cos();
        let py = thickness;
        let pz = r * theta.sin();

        let p = Vec3::new(px, py, pz);
        let n = p.normalize();

        let u_temp = if n.x.abs() > 0.9 { Vec3::Y } else { Vec3::X };
        let v = n.cross(u_temp).normalize();
        let u = v.cross(n).normalize();

        let hs = size * 0.5;
        let p0 = p - u * hs - v * hs;
        let p1 = p + u * hs - v * hs;
        let p2 = p + u * hs + v * hs;
        let p3 = p - u * hs + v * hs;

        let lerp_t = rng.next_f32() * 0.2;
        let mut col = base_color;
        col[0] += (target_color[0] - col[0]) * lerp_t;
        col[1] += (target_color[1] - col[1]) * lerp_t;
        col[2] += (target_color[2] - col[2]) * lerp_t;

        let uv_coords = [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ];

        let pts = [p0, p1, p2, p3];
        for (i, pt) in pts.iter().enumerate() {
            vertices.push(pt.x);
            vertices.push(pt.y);
            vertices.push(pt.z);

            vertices.push(n.x);
            vertices.push(n.y);
            vertices.push(n.z);

            vertices.push(uv_coords[i][0]);
            vertices.push(uv_coords[i][1]);

            vertices.push(col[0]);
            vertices.push(col[1]);
            vertices.push(col[2]);
            vertices.push(opacity);
        }

        let start_idx = (idx * 4) as u32;
        indices.push(start_idx);
        indices.push(start_idx + 1);
        indices.push(start_idx + 2);
        indices.push(start_idx + 2);
        indices.push(start_idx + 3);
        indices.push(start_idx);
    }

    DynamicMesh {
        vertices,
        indices,
        version: 0,
        color_version: 0,
    }
}

fn generate_nebula_cloud_mesh(count: usize, size: f32, color_hex: u32, opacity: f32, rng: &mut Lcg) -> DynamicMesh {
    let mut vertices = Vec::with_capacity(count * 4 * 12);
    let mut indices = Vec::with_capacity(count * 6);
    let base_color = hex_to_linear_rgb(color_hex);
    let dark_navy = hex_to_linear_rgb(0x000210);

    for idx in 0..count {
        let r = 350.0 + rng.next_f32() * 700.0;
        let theta = rng.next_f32() * 2.0 * std::f32::consts::PI;
        let phi = (2.0 * rng.next_f32() - 1.0).acos();

        let px = r * phi.sin() * theta.cos();
        let py = r * phi.sin() * theta.sin();
        let pz = r * phi.cos();

        let p = Vec3::new(px, py, pz);
        let n = p.normalize();

        let u_temp = if n.x.abs() > 0.9 { Vec3::Y } else { Vec3::X };
        let v = n.cross(u_temp).normalize();
        let u = v.cross(n).normalize();

        let hs = size * 0.5;
        let p0 = p - u * hs - v * hs;
        let p1 = p + u * hs - v * hs;
        let p2 = p + u * hs + v * hs;
        let p3 = p - u * hs + v * hs;

        let lerp_t = rng.next_f32() * 0.1;
        let mut col = base_color;
        col[0] += (dark_navy[0] - col[0]) * lerp_t;
        col[1] += (dark_navy[1] - col[1]) * lerp_t;
        col[2] += (dark_navy[2] - col[2]) * lerp_t;

        let uv_coords = [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ];

        let pts = [p0, p1, p2, p3];
        for (i, pt) in pts.iter().enumerate() {
            vertices.push(pt.x);
            vertices.push(pt.y);
            vertices.push(pt.z);

            vertices.push(n.x);
            vertices.push(n.y);
            vertices.push(n.z);

            vertices.push(uv_coords[i][0]);
            vertices.push(uv_coords[i][1]);

            vertices.push(col[0]);
            vertices.push(col[1]);
            vertices.push(col[2]);
            vertices.push(opacity);
        }

        let start_idx = (idx * 4) as u32;
        indices.push(start_idx);
        indices.push(start_idx + 1);
        indices.push(start_idx + 2);
        indices.push(start_idx + 2);
        indices.push(start_idx + 3);
        indices.push(start_idx);
    }

    DynamicMesh {
        vertices,
        indices,
        version: 0,
        color_version: 0,
    }
}

pub fn sys_initialize_stars(
    mut q: Query<'_, (Entity, &mut DynamicMesh), Without<PlanetConfig>>,
) {
    q.for_each(|(_ent, mesh)| {
        if mesh.version == 0 {
            mesh.version = 1;
        }
    });
}

pub fn sys_rotate_space_background(
    mut q: Query<'_, (&mut Transform, &SpaceRotation, Option<&SpaceRotationTilt>)>,
    time: Res<Time>,
) {
    let t_elapsed = time.elapsed_seconds;
    q.par_for_each(|(t, sr, tilt)| {
        let angle = t_elapsed * sr.speed;
        let rot_y = glam::Quat::from_rotation_y(angle);
        let final_q = if let Some(t_val) = tilt {
            let tilt_q = glam::Quat::from_rotation_x(t_val.x) * glam::Quat::from_rotation_z(t_val.z);
            tilt_q * rot_y
        } else {
            rot_y
        };
        t.rotation = final_q.to_array();
    });
}

pub fn sys_rotate_nebula_clouds(
    mut q: Query<'_, (&mut Transform, &NebulaRotation)>,
    time: Res<Time>,
) {
    let t_elapsed = time.elapsed_seconds;
    q.par_for_each(|(t, neb)| {
        let i = neb.index;
        let rot_x = neb.init_x;
        let rot_y = neb.init_y - 0.000003 * (i + 1.0) * t_elapsed;
        let rot_z = neb.init_z + 0.000001 * (i + 1.0) * t_elapsed;
        let q_rot = glam::Quat::from_euler(glam::EulerRot::XYZ, rot_x, rot_y, rot_z);
        t.rotation = q_rot.to_array();
    });
}

#[wasm_bindgen]
pub fn create_vivarium_engine(dpi_scale: f32) -> WasmEngine {
    let mut engine = WasmEngine::new();
    engine.app_mut().add_system(sys_generate_planet_mesh);
    engine.app_mut().add_system(sys_generate_atmosphere_mesh);
    engine.app_mut().add_system(sys_update_atmosphere_halo);
    engine.app_mut().add_system(sys_initialize_stars);
    engine.app_mut().add_system(sys_rotate_space_background);
    engine.app_mut().add_system(sys_rotate_nebula_clouds);
    let world = &mut engine.app_mut().world;
    register_component_schema!(world, PlanetConfig, seed, continent_scale, warp_amount, polar_land, water_level, base_height, hill_height, mountain_density, mountain_scale, mountain_height, global_moisture, latitude_bands, weather_warp, moisture_scale, lapse_rate, subdivisions, visualization_mode, version);
    register_component_schema!(world, AtmosphereConfig, subdivisions, generated_subdivisions, visible);
    register_component_schema!(world, AtmosphereHalo, visible);
    register_component_schema!(
        world,
        PlanetSimulationState,
        23,
        seed_value: 0
    );
    register_component_schema!(world, SpaceRotation, speed);
    register_component_schema!(world, SpaceRotationTilt, x, z);
    register_component_schema!(world, NebulaRotation, index, init_x, init_y, init_z);

    let hemi = world.spawn();
    world.add_component(hemi, HemisphereLight {
        sky_color: [1.0, 1.0, 1.0],
        sky_intensity: 1.0,
        ground_color: [0.13333334, 0.2, 0.26666668],
        ground_intensity: 1.0,
    });
    let amb = world.spawn();
    world.add_component(amb, AmbientLight {
        color: [0.2509804, 0.27058825, 0.3137255],
        intensity: 0.4,
    });
    let sun = world.spawn();
    world.add_component(sun, DirectionalLight {
        color: [1.0, 0.99215686, 0.93333333],
        intensity: 1.8,
        direction: [-40.0, -30.0, -20.0],
        pad: 0.0,
    });
    let backlight = world.spawn();
    world.add_component(backlight, DirectionalLight {
        color: [0.53333336, 0.6666667, 0.8],
        intensity: 1.8,
        direction: [40.0, 30.0, 20.0],
        pad: 0.0,
    });

    let cam = world.spawn();
    world.add_component(cam, Transform {
        translation: [0.0, 0.0, 100.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    });
    world.add_component(cam, GlobalTransform::default());
    let exposure = 1.2;
    world.add_component(cam, Camera3D {
        fov: 0.785398,
        aspect: 1.777,
        near: 0.1,
        far: 2000.0,
        exposure,
        ..Default::default()
    });

    let planet = world.spawn();
    world.add_component(planet, Transform {
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    });
    world.add_component(planet, GlobalTransform::default());
    world.add_component(planet, PlanetConfig::default());
    world.add_component(planet, PlanetSimulationState::default());
    world.add_component(planet, DynamicMesh::default());
    world.add_component(planet, MeshBVH::default());
    world.add_component(planet, StandardMaterial {
        base_color: [1.0, 1.0, 1.0, 1.0],
        emissive: [0.0, 0.0, 0.0],
        metallic: 0.1,
        roughness: 1.0,
        pad: [0.0; 3],
    });

    let inner_glow = world.spawn();
    let outer_glow = world.spawn();
    world.add_component(planet, Children(vec![inner_glow]));

    world.add_component(inner_glow, Parent(planet));
    world.add_component(inner_glow, Transform {
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    });
    world.add_component(inner_glow, GlobalTransform::default());
    world.add_component(inner_glow, AtmosphereConfig::default());
    world.add_component(inner_glow, DynamicMesh::default());
    world.add_component(inner_glow, StandardMaterial {
        base_color: [0.1, 0.6, 0.9, 1.0],
        emissive: [0.0, 0.0, 0.0],
        metallic: 0.0,
        roughness: 1.0,
        pad: [2.0, 0.0, 1.0],
    });

    world.add_component(outer_glow, Transform {
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    });
    world.add_component(outer_glow, GlobalTransform::default());
    world.add_component(outer_glow, Billboard { active: 1 });
    world.add_component(outer_glow, AtmosphereHalo::default());
    let mut outer_mesh = DynamicMesh::default();
    outer_mesh.vertices = vec![
        -21.5, -21.5, 0.0,   0.0, 0.0, 1.0,   0.0, 0.0,  1.0, 1.0, 1.0, 1.0,
         21.5, -21.5, 0.0,   0.0, 0.0, 1.0,   1.0, 0.0,  1.0, 1.0, 1.0, 1.0,
         21.5,  21.5, 0.0,   0.0, 0.0, 1.0,   1.0, 1.0,  1.0, 1.0, 1.0, 1.0,
        -21.5,  21.5, 0.0,   0.0, 0.0, 1.0,   0.0, 1.0,  1.0, 1.0, 1.0, 1.0,
    ];
    outer_mesh.indices = vec![
        0, 1, 2,
        2, 3, 0,
    ];
    outer_mesh.version = 1;
    world.add_component(outer_glow, outer_mesh);
    world.add_component(outer_glow, StandardMaterial {
        base_color: [0.1568, 0.3921, 1.0, 0.28],
        emissive: [0.0, 0.0, 0.0],
        metallic: 0.0,
        roughness: 1.0,
        pad: [2.0, 2.5, 0.0],
    });

    let mut rng = Lcg::new(54321);
    let scale_factor = 0.40;

    let space_background = world.spawn();
    world.add_component(space_background, Transform {
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    });
    world.add_component(space_background, GlobalTransform::default());
    world.add_component(space_background, SpaceRotation { speed: 0.0012 });

    let mut space_children = Vec::new();

    let galaxy_ring = world.spawn();
    space_children.push(galaxy_ring);
    world.add_component(galaxy_ring, Parent(space_background));
    world.add_component(galaxy_ring, Transform {
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    });
    world.add_component(galaxy_ring, GlobalTransform::default());
    world.add_component(galaxy_ring, SpaceRotation { speed: 0.0009 });
    world.add_component(galaxy_ring, SpaceRotationTilt {
        x: std::f32::consts::PI * 0.18,
        z: std::f32::consts::PI * 0.08,
    });
    let ring_mesh = generate_galaxy_ring_mesh(2000, (8.0 * scale_factor) / dpi_scale, 0xffffff, 0.45, 400.0, 800.0, 160.0, &mut rng);
    world.add_component(galaxy_ring, ring_mesh);
    world.add_component(galaxy_ring, StandardMaterial {
        base_color: [1.0, 1.0, 1.0, 1.0],
        emissive: [0.0, 0.0, 0.0],
        metallic: 0.0,
        roughness: 1.0,
        pad: [2.0, 2.0, 0.0],
    });

    let star_configs = [
        (3500, (2.0 * scale_factor) / dpi_scale, 0xffffff, 0.65, 400.0, 1800.0, 0.00024),
        (2000, (3.8 * scale_factor) / dpi_scale, 0xaaccff, 0.75, 350.0, 1400.0, 0.00048),
        (1000, (6.0 * scale_factor) / dpi_scale, 0xffeedd, 0.60, 300.0, 1100.0, 0.00072),
        (200, (11.0 * scale_factor) / dpi_scale, 0xffffff, 0.95, 300.0, 900.0, 0.00096),
    ];
    for (count, size, color_hex, opacity, r_min, r_max, speed) in star_configs {
        let star_layer = world.spawn();
        space_children.push(star_layer);
        world.add_component(star_layer, Parent(space_background));
        world.add_component(star_layer, Transform {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        });
        world.add_component(star_layer, GlobalTransform::default());
        world.add_component(star_layer, SpaceRotation { speed });
        let mesh = generate_star_layer_mesh(count, size, color_hex, opacity, r_min, r_max, &mut rng);
        world.add_component(star_layer, mesh);
        world.add_component(star_layer, StandardMaterial {
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.0, 0.0, 0.0],
            metallic: 0.0,
            roughness: 1.0,
            pad: [2.0, 2.0, 0.0],
        });
    }

    let nebula_configs = [
        (1000, 350.0, 0x051a4a, 0.045, 0.0, 0.0, 0.0),
        (700,  500.0, 0x1a054a, 0.035, 0.4, 1.2, 0.3),
        (500,  420.0, 0x054a32, 0.024, -0.3, -1.0, 0.8),
        (350,  600.0, 0x4a2a05, 0.018, 1.1, 0.2, -0.5),
    ];
    for (idx, (count, size_raw, color_hex, opacity, rx, ry, rz)) in nebula_configs.into_iter().enumerate() {
        let nebula_layer = world.spawn();
        space_children.push(nebula_layer);
        world.add_component(nebula_layer, Parent(space_background));
        world.add_component(nebula_layer, Transform {
            translation: [0.0, 0.0, 0.0],
            rotation: glam::Quat::from_euler(glam::EulerRot::XYZ, rx, ry, rz).to_array(),
            scale: [1.0, 1.0, 1.0],
        });
        world.add_component(nebula_layer, GlobalTransform::default());
        world.add_component(nebula_layer, NebulaRotation {
            index: idx as f32,
            init_x: rx,
            init_y: ry,
            init_z: rz,
        });
        let mesh = generate_nebula_cloud_mesh(count, (size_raw * scale_factor) / dpi_scale, color_hex, opacity, &mut rng);
        world.add_component(nebula_layer, mesh);
        world.add_component(nebula_layer, StandardMaterial {
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.0, 0.0, 0.0],
            metallic: 0.0,
            roughness: 1.0,
            pad: [2.0, 3.0, 0.0],
        });
    }

    world.add_component(space_background, Children(space_children));

    engine
}

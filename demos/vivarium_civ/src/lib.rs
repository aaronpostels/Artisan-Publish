use artisan::ecs::world::World;
use artisan::ecs::{Children, Entity, Parent, Query, Res, Without};
use artisan::engine::Time;
use artisan::engine::component::{
    AmbientLight, Billboard, Camera3D, DirectionalLight, DynamicMesh, GlobalTransform,
    HemisphereLight, MeshBVH, MeshHandle, StandardMaterial, Transform,
};
use artisan::engine::math::hex_to_linear_rgb;
use artisan::{WasmEngine, mesh_icosphere_native, register_component_schema};
use glam::Vec3;
use js_sys::{Float32Array, Object, Reflect, Uint32Array};
use wasm_bindgen::prelude::*;

pub mod components;
pub mod systems;

use components::{
    DroughtEvent, NebulaRotation, PlanetConfig, PlanetSimulationState, Settlement, Settler,
    SimTuning, SpaceRotation, SpaceRotationTilt,
};
use systems::{
    apply_tribe_color, bilinear_interpolate_biome, compute_distance_to_water, mesh_circle_marker,
    sys_generate_planet_mesh, sys_snap_settler_render_height, sys_spawn_settlers,
    sys_step_settlers, sys_tick_drought, sys_tick_face_color, sys_tick_resources,
    sys_tribe_dynamics,
};

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
        if comp_id < arch.component_to_column.len() && arch.component_to_column[comp_id] != u32::MAX
        {
            let col = unsafe { &*arch.columns[arch.component_to_column[comp_id] as usize].get() };
            if !col.data.is_empty() {
                return Some(unsafe { &*col.data.as_ptr::<PlanetSimulationState>() });
            }
        }
    }
    None
}

fn find_settlement_archetype(world: &World) -> Option<u32> {
    let comp_id = world.get_component_id::<Settlement>()?;
    for arch in &world.archetypes {
        if comp_id < arch.component_to_column.len() && arch.component_to_column[comp_id] != u32::MAX
        {
            return Some(arch.id);
        }
    }
    None
}

fn find_planet_entity(world: &World) -> Option<Entity> {
    let comp_id = world.get_component_id::<PlanetConfig>()?;
    for arch in &world.archetypes {
        if comp_id < arch.component_to_column.len()
            && arch.component_to_column[comp_id] != u32::MAX
            && !arch.entities.is_empty()
        {
            return Some(arch.entities[0]);
        }
    }
    None
}

#[wasm_bindgen]
pub fn vivarium_settler_marker_mesh() -> Object {
    let (v, i) = mesh_circle_marker(10, 1.0);
    let obj = Object::new();
    Reflect::set(&obj, &"vertices".into(), &Float32Array::from(v.as_slice())).unwrap();
    Reflect::set(&obj, &"indices".into(), &Uint32Array::from(i.as_slice())).unwrap();
    obj
}

#[wasm_bindgen]
pub fn wasm_set_settler_mesh_id(engine: &mut WasmEngine, mesh_id: f32) {
    let world = &mut engine.app_mut().world;
    let Some(planet) = find_planet_entity(world) else {
        return;
    };
    if let Some(state) = world.get_component_mut::<PlanetSimulationState>(planet) {
        state.settler_mesh_id = mesh_id;
    }
}

#[wasm_bindgen]
pub fn wasm_headless_start(engine: &mut WasmEngine, seed_bits: u32, population: f32) {
    let world = &mut engine.app_mut().world;
    let Some(planet) = find_planet_entity(world) else {
        return;
    };
    if let Some(cfg) = world.get_component_mut::<PlanetConfig>(planet) {
        cfg.seed = f32::from_bits(seed_bits);
        cfg.version += 1.0;
    }
    if let Some(state) = world.get_component_mut::<PlanetSimulationState>(planet) {
        state.num_colonies = population;
        state.run_simulation = 1.0;
    }
}

#[wasm_bindgen]
pub fn wasm_headless_set_tuning(engine: &mut WasmEngine, values: &[f32]) {
    let world = &mut engine.app_mut().world;
    let Some(planet) = find_planet_entity(world) else {
        return;
    };
    if let Some(tuning) = world.get_component_mut::<SimTuning>(planet) {
        let n = values.len().min(components::SIM_TUNING_FIELD_COUNT);
        let ptr = tuning as *mut SimTuning as *mut f32;
        for (i, &v) in values.iter().take(n).enumerate() {
            unsafe {
                *ptr.add(i) = v;
            }
        }
    }
}

#[wasm_bindgen]
pub fn wasm_debug_trait_correlation(engine: &WasmEngine) -> Vec<f32> {
    let world = &engine.app().world;
    let settler_id = world.get_component_id::<Settler>();
    let mut buckets = [[0.0f32; 3]; 4];
    if let Some(s_id) = settler_id {
        for arch in &world.archetypes {
            if s_id >= arch.component_to_column.len() {
                continue;
            }
            let s_col = arch.component_to_column[s_id];
            if s_col == u32::MAX {
                continue;
            }
            let s_ptr = unsafe {
                (*arch.columns[s_col as usize].get())
                    .data
                    .as_ptr::<Settler>()
            };
            for i in 0..arch.entities.len() {
                let s = unsafe { &*s_ptr.add(i) };
                let b = if s.cooperation >= 0.5 { 0 } else { 1 };
                buckets[b][0] += s.hunger;
                buckets[b][1] += s.age;
                buckets[b][2] += 1.0;
            }
        }
    }

    let mut agg_buckets = [[0.0f32; 3]; 2];
    let mut mob_buckets = [[0.0f32; 3]; 2];
    if let Some(s_id) = settler_id {
        for arch in &world.archetypes {
            if s_id >= arch.component_to_column.len() {
                continue;
            }
            let s_col = arch.component_to_column[s_id];
            if s_col == u32::MAX {
                continue;
            }
            let s_ptr = unsafe {
                (*arch.columns[s_col as usize].get())
                    .data
                    .as_ptr::<Settler>()
            };
            for i in 0..arch.entities.len() {
                let s = unsafe { &*s_ptr.add(i) };
                let ab = if s.aggression >= 0.5 { 0 } else { 1 };
                agg_buckets[ab][0] += s.hunger;
                agg_buckets[ab][1] += s.age;
                agg_buckets[ab][2] += 1.0;
                let mb = if s.mobility >= 0.5 { 0 } else { 1 };
                mob_buckets[mb][0] += s.hunger;
                mob_buckets[mb][1] += s.age;
                mob_buckets[mb][2] += 1.0;
            }
        }
    }
    let mut out = Vec::with_capacity(18);
    for b in [
        buckets[0],
        buckets[1],
        agg_buckets[0],
        agg_buckets[1],
        mob_buckets[0],
        mob_buckets[1],
    ] {
        let n = b[2].max(1.0);
        out.push(b[0] / n);
        out.push(b[1] / n);
        out.push(b[2]);
    }
    out
}

#[wasm_bindgen]
pub fn wasm_debug_tile_correlation(engine: &WasmEngine) -> Vec<f32> {
    let world = &engine.app().world;
    let settler_id = world.get_component_id::<Settler>();
    let state = match find_planet_state(world) {
        Some(s) => s,
        None => return vec![0.0; 8],
    };
    let mut buckets = [[0.0f32; 3]; 2];
    if let Some(s_id) = settler_id {
        for arch in &world.archetypes {
            if s_id >= arch.component_to_column.len() {
                continue;
            }
            let s_col = arch.component_to_column[s_id];
            if s_col == u32::MAX {
                continue;
            }
            let s_ptr = unsafe {
                (*arch.columns[s_col as usize].get())
                    .data
                    .as_ptr::<Settler>()
            };
            for i in 0..arch.entities.len() {
                let s = unsafe { &*s_ptr.add(i) };
                let f = s.face_index as usize;
                let cap = state.food_cap.get(f).copied().unwrap_or(0.0);
                let frac = if cap > 0.0 {
                    (state.food_stock.get(f).copied().unwrap_or(0.0) / cap).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let pop = state.face_population.get(f).copied().unwrap_or(0) as f32;
                let b = if s.cooperation >= 0.5 { 0 } else { 1 };
                buckets[b][0] += frac;
                buckets[b][1] += pop;
                buckets[b][2] += 1.0;
            }
        }
    }
    let mut out = Vec::with_capacity(8);
    for b in buckets {
        let n = b[2].max(1.0);
        out.push(b[0] / n);
        out.push(b[1] / n);
        out.push(b[2]);
    }
    out
}

#[wasm_bindgen]
pub fn wasm_bench_set_system_mask(engine: &mut WasmEngine, mask: u32) {
    let world = &mut engine.app_mut().world;
    let Some(planet) = find_planet_entity(world) else {
        return;
    };
    if let Some(state) = world.get_component_mut::<PlanetSimulationState>(planet) {
        state.bench_system_mask = mask;
    }
}

#[wasm_bindgen]
pub fn wasm_headless_tuning_names() -> String {
    components::SIM_TUNING_FIELD_NAMES.to_string()
}

#[wasm_bindgen]
pub fn wasm_headless_get_tuning(engine: &WasmEngine) -> Vec<f32> {
    let world = &engine.app().world;
    let Some(planet) = find_planet_entity(world) else {
        return vec![0.0; components::SIM_TUNING_FIELD_COUNT];
    };
    match world.get_component::<SimTuning>(planet) {
        Some(tuning) => {
            let ptr = tuning as *const SimTuning as *const f32;
            (0..components::SIM_TUNING_FIELD_COUNT)
                .map(|i| unsafe { *ptr.add(i) })
                .collect()
        }
        None => vec![0.0; components::SIM_TUNING_FIELD_COUNT],
    }
}

#[wasm_bindgen]
pub fn wasm_headless_stats(engine: &WasmEngine) -> Vec<f32> {
    let world = &engine.app().world;
    let mut alive = 0u32;
    let mut total_hunger = 0.0f32;
    let mut total_thirst = 0.0f32;
    let mut total_coop = 0.0f32;
    let mut total_agg = 0.0f32;
    let mut total_mob = 0.0f32;
    let mut tribe_sizes: std::collections::HashMap<i32, u32> = std::collections::HashMap::new();
    let mut occupied_faces: std::collections::HashSet<i32> = std::collections::HashSet::new();

    let settler_id = world.get_component_id::<Settler>();
    if let Some(s_id) = settler_id {
        for arch in &world.archetypes {
            if s_id >= arch.component_to_column.len() {
                continue;
            }
            let s_col = arch.component_to_column[s_id];
            if s_col == u32::MAX {
                continue;
            }
            let s_ptr = unsafe {
                (*arch.columns[s_col as usize].get())
                    .data
                    .as_ptr::<Settler>()
            };
            for i in 0..arch.entities.len() {
                let s = unsafe { &*s_ptr.add(i) };
                alive += 1;
                total_hunger += s.hunger;
                total_thirst += s.thirst;
                total_coop += s.cooperation;
                total_agg += s.aggression;
                total_mob += s.mobility;
                occupied_faces.insert(s.face_index as i32);
                let tid = if s.tribe_id >= 0.0 {
                    s.tribe_id as i32
                } else {
                    -1
                };
                if tid >= 0 {
                    *tribe_sizes.entry(tid).or_insert(0) += 1;
                }
            }
        }
    }

    let tribe_count = tribe_sizes.len() as u32;
    let largest = tribe_sizes.values().copied().max().unwrap_or(0);
    let avg_hunger = if alive > 0 {
        total_hunger / alive as f32
    } else {
        0.0
    };
    let avg_thirst = if alive > 0 {
        total_thirst / alive as f32
    } else {
        0.0
    };
    let avg_coop = if alive > 0 {
        total_coop / alive as f32
    } else {
        0.0
    };
    let avg_agg = if alive > 0 {
        total_agg / alive as f32
    } else {
        0.0
    };
    let avg_mob = if alive > 0 {
        total_mob / alive as f32
    } else {
        0.0
    };

    let (
        births_total,
        deaths_starved,
        deaths_aged,
        cooperation_events,
        aggression_events,
        tribe_splits,
        active_droughts,
    ) = match find_planet_state(world) {
        Some(state) => (
            state.births_total as f32,
            state.deaths_starved as f32,
            state.deaths_aged as f32,
            state.cooperation_events as f32,
            state.aggression_events as f32,
            state.tribe_splits as f32,
            state.droughts.len() as f32,
        ),
        None => (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
    };

    vec![
        alive as f32,
        tribe_count as f32,
        largest as f32,
        avg_hunger,
        avg_thirst,
        occupied_faces.len() as f32,
        births_total,
        deaths_starved,
        deaths_aged,
        cooperation_events,
        aggression_events,
        tribe_splits,
        active_droughts,
        avg_coop,
        avg_agg,
        avg_mob,
    ]
}

#[wasm_bindgen]
pub fn wasm_headless_planet_info(engine: &WasmEngine) -> Vec<f32> {
    let world = &engine.app().world;
    let Some(state) = find_planet_state(world) else {
        return vec![0.0; 5];
    };
    let num_faces = state.is_water.len();
    let mut land_faces = 0u32;
    let mut arability_sum = 0.0f32;
    let mut coastal_faces = 0u32;
    let mut dist_sum = 0.0f64;
    let mut dist_n = 0u32;
    for f in 0..num_faces {
        if state.is_water.get(f).copied().unwrap_or(1.0) > 0.5 {
            continue;
        }
        land_faces += 1;
        arability_sum += state.arability.get(f).copied().unwrap_or(0.0);
        let d = state.dist_to_water.get(f).copied().unwrap_or(u32::MAX);
        if d == 0 {
            coastal_faces += 1;
        }
        if d != u32::MAX {
            dist_sum += d as f64;
            dist_n += 1;
        }
    }
    let mean_arability = if land_faces > 0 {
        arability_sum / land_faces as f32
    } else {
        0.0
    };
    let mean_dist = if dist_n > 0 {
        (dist_sum / dist_n as f64) as f32
    } else {
        0.0
    };
    vec![
        num_faces as f32,
        land_faces as f32,
        mean_arability,
        coastal_faces as f32,
        mean_dist,
    ]
}

#[wasm_bindgen]
pub fn wasm_headless_max_seam_error(engine: &WasmEngine) -> f32 {
    use std::collections::HashMap;
    let world = &engine.app().world;
    let Some(planet) = find_planet_entity(world) else {
        return f32::INFINITY;
    };
    let Some(state) = world.get_component::<PlanetSimulationState>(planet) else {
        return f32::INFINITY;
    };
    let Some(mesh) = world.get_component::<DynamicMesh>(planet) else {
        return f32::INFINITY;
    };
    let mut first: HashMap<u32, [f32; 3]> = HashMap::new();
    let mut max_error_sq = 0.0f32;
    for (vertex, &source) in state.vertex_sources.iter().enumerate() {
        let offset = vertex * 12;
        if offset + 2 >= mesh.vertices.len() {
            break;
        }
        let position = [
            mesh.vertices[offset],
            mesh.vertices[offset + 1],
            mesh.vertices[offset + 2],
        ];
        if let Some(original) = first.get(&source) {
            let dx = position[0] - original[0];
            let dy = position[1] - original[1];
            let dz = position[2] - original[2];
            max_error_sq = max_error_sq.max(dx * dx + dy * dy + dz * dz);
        } else {
            first.insert(source, position);
        }
    }
    max_error_sq.sqrt()
}

#[wasm_bindgen]
pub fn wasm_headless_face_visual(engine: &WasmEngine, face: usize) -> Vec<f32> {
    let world = &engine.app().world;
    let Some(planet) = find_planet_entity(world) else {
        return vec![];
    };
    let Some(state) = world.get_component::<PlanetSimulationState>(planet) else {
        return vec![];
    };
    let Some(mesh) = world.get_component::<DynamicMesh>(planet) else {
        return vec![];
    };
    if face >= mesh.indices.len() / 3 {
        return vec![];
    }
    let vertex = mesh.indices[face * 3] as usize;
    let offset = vertex * 3;
    vec![
        mesh_face_average_radius(mesh, face) - 10.0,
        state.base_colors.get(offset).copied().unwrap_or(0.0),
        state.base_colors.get(offset + 1).copied().unwrap_or(0.0),
        state.base_colors.get(offset + 2).copied().unwrap_or(0.0),
    ]
}

#[wasm_bindgen]
pub fn wasm_headless_face_neighbors(engine: &WasmEngine, face: usize) -> Vec<u32> {
    let Some(state) = find_planet_state(&engine.app().world) else {
        return vec![];
    };
    if face + 1 >= state.neighbors_offsets.len() {
        return vec![];
    }
    let start = state.neighbors_offsets[face] as usize;
    let end = (state.neighbors_offsets[face + 1] as usize).min(state.neighbors_flat.len());
    state.neighbors_flat[start..end].to_vec()
}

#[wasm_bindgen]

pub fn wasm_get_face_info(engine: &WasmEngine, face_id: usize) -> Vec<f32> {
    let world = &engine.app().world;
    let mut info = vec![0.0; 13];
    if let Some(state) = find_planet_state(world) {

        info[0] = state.is_water.get(face_id).copied().unwrap_or(0.0);
        info[1] = state.elevations.get(face_id).copied().unwrap_or(0.0);
        info[2] = state.temps.get(face_id).copied().unwrap_or(0.0);
        info[3] = state.moistures.get(face_id).copied().unwrap_or(0.0);
        info[4] = state.arability.get(face_id).copied().unwrap_or(0.0);
        info[5] = state.minerals.get(face_id).copied().unwrap_or(0.0);
        info[6] = state.face_owner.get(face_id).copied().unwrap_or(-1) as f32;
        info[7] = state.face_score.get(face_id).copied().unwrap_or(0.0);
        info[8] = state.food_stock.get(face_id).copied().unwrap_or(0.0);
        info[9] = state.food_cap.get(face_id).copied().unwrap_or(0.0);
        info[10] = state.drought.get(face_id).copied().unwrap_or(0.0);
        info[11] = state.face_population.get(face_id).copied().unwrap_or(0) as f32;
        info[12] = state
            .face_dominant_tribe
            .get(face_id)
            .copied()
            .unwrap_or(-1) as f32;
    }
    info
}

fn find_planet_mut(world: &mut World) -> Option<(*mut PlanetSimulationState, *mut DynamicMesh)> {
    let state_id = world.get_component_id::<PlanetSimulationState>()?;
    let mesh_id = world.get_component_id::<DynamicMesh>()?;
    for arch in &world.archetypes {
        if arch.entities.is_empty() {
            continue;
        }
        if state_id >= arch.component_to_column.len() || mesh_id >= arch.component_to_column.len() {
            continue;
        }
        let s_col = arch.component_to_column[state_id];
        let m_col = arch.component_to_column[mesh_id];
        if s_col == u32::MAX || m_col == u32::MAX {
            continue;
        }
        let state_ptr = unsafe {
            (*arch.columns[s_col as usize].get())
                .data
                .as_ptr::<PlanetSimulationState>() as *mut PlanetSimulationState
        };
        let mesh_ptr = unsafe {
            (*arch.columns[m_col as usize].get())
                .data
                .as_ptr::<DynamicMesh>() as *mut DynamicMesh
        };
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
        if let (Some(&r), Some(&g), Some(&b)) = (
            state.base_colors.get(vi0 * 3),
            state.base_colors.get(vi0 * 3 + 1),
            state.base_colors.get(vi0 * 3 + 2),
        ) {
            set_face_color(mesh, f, [r, g, b]);
        }
    }
    if face_id >= 0 && (face_id as usize) < num_faces {
        let f = face_id as usize;
        let vi0 = mesh.indices[f * 3] as usize;
        if let (Some(&r), Some(&g), Some(&b)) = (
            state.base_colors.get(vi0 * 3),
            state.base_colors.get(vi0 * 3 + 1),
            state.base_colors.get(vi0 * 3 + 2),
        ) {
            let base = [r, g, b];
            let highlight = [0.3, 0.95, 1.0];
            let blend = 0.65;
            let color = [
                base[0] + (highlight[0] - base[0]) * blend,
                base[1] + (highlight[1] - base[1]) * blend,
                base[2] + (highlight[2] - base[2]) * blend,
            ];
            set_face_color(mesh, f, color);
        }
    }

    mesh.color_version = mesh.color_version.wrapping_add(1);
}

#[wasm_bindgen]
pub fn wasm_get_colony_stats(engine: &WasmEngine) -> Vec<f32> {
    let world = &engine.app().world;
    let mut stats = Vec::new();
    let state = match find_planet_state(world) {
        Some(s) => s,
        None => return stats,
    };
    let num_factions = state.faction_colors.len();
    let mut faction_pops = vec![0.0_f32; num_factions];
    let mut faction_nodes = vec![0.0_f32; num_factions];
    let mut faction_wealth = vec![0.0_f32; num_factions];
    if let Some(arch_id) = find_settlement_archetype(world) {
        if let Some(sets) = world.get_column_ptr::<Settlement>(arch_id) {
            let len = world.get_column_len(arch_id);
            for i in 0..len {
                let set = unsafe { &*sets.add(i) };
                let f_id = set.faction_id as usize;
                if f_id < num_factions {
                    faction_pops[f_id] += set.population;
                    faction_nodes[f_id] += 1.0;
                    faction_wealth[f_id] += set.wealth;
                }
            }
        }
    }
    for i in 0..num_factions {
        if faction_nodes[i] > 0.0 {
            stats.push(i as f32);
            stats.push(faction_pops[i]);
            stats.push(faction_nodes[i]);
            stats.push(faction_wealth[i]);
            stats.push(state.faction_tech[i]);
        }
    }
    stats
}

#[wasm_bindgen]
pub fn wasm_get_settlements_data(engine: &WasmEngine) -> Vec<f32> {
    let world = &engine.app().world;
    let mut data = Vec::new();
    let arch_id = match find_settlement_archetype(world) {
        Some(id) => id,
        None => return data,
    };
    if let (Some(sets), Some(transforms)) = (
        world.get_column_ptr::<Settlement>(arch_id),
        world.get_column_ptr::<Transform>(arch_id),
    ) {
        let len = world.get_column_len(arch_id);
        for i in 0..len {
            let set = unsafe { &*sets.add(i) };
            let transform = unsafe { &*transforms.add(i) };
            data.push(set.id);
            data.push(transform.translation[0]);
            data.push(transform.translation[1]);
            data.push(transform.translation[2]);
            data.push(set.population);
            data.push(set.faction_id);
            data.push(set.infrastructure);
            data.push(set.is_capital);
            data.push(set.name_seed);
            data.push(set.face_index);
        }
    }
    data
}

fn generate_star_layer_mesh(
    count: usize,
    size: f32,
    color_hex: u32,
    opacity: f32,
    r_min: f32,
    r_max: f32,
    rng: &mut Lcg,
) -> DynamicMesh {
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

        let uv_coords = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

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

fn generate_galaxy_ring_mesh(
    count: usize,
    size: f32,
    color_hex: u32,
    opacity: f32,
    r_min: f32,
    r_max: f32,
    thickness_spread: f32,
    rng: &mut Lcg,
) -> DynamicMesh {
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

        let uv_coords = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

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

fn generate_nebula_cloud_mesh(
    count: usize,
    size: f32,
    color_hex: u32,
    opacity: f32,
    rng: &mut Lcg,
) -> DynamicMesh {
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

        let uv_coords = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

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

pub fn sys_initialize_stars(mut q: Query<'_, (Entity, &mut DynamicMesh), Without<PlanetConfig>>) {
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
            let tilt_q =
                glam::Quat::from_rotation_x(t_val.x) * glam::Quat::from_rotation_z(t_val.z);
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
    engine.app_mut().add_system(sys_tick_drought);
    engine.app_mut().add_system(sys_tick_resources);
    engine.app_mut().add_system(sys_spawn_settlers);

    engine.app_mut().add_system(sys_tribe_dynamics);
    engine.app_mut().add_system(sys_step_settlers);
    engine.app_mut().add_system(sys_tick_face_color);

    engine
        .app_mut()
        .add_render_system(sys_snap_settler_render_height);
    engine.app_mut().add_system(sys_initialize_stars);
    engine.app_mut().add_system(sys_rotate_space_background);
    engine.app_mut().add_system(sys_rotate_nebula_clouds);
    let world = &mut engine.app_mut().world;
    register_component_schema!(
        world,
        PlanetConfig,
        seed,
        continent_scale,
        warp_amount,
        polar_land,
        water_level,
        base_height,
        hill_height,
        mountain_density,
        mountain_scale,
        mountain_height,
        global_moisture,
        latitude_bands,
        weather_warp,
        moisture_scale,
        lapse_rate,
        version
    );
    register_component_schema!(
        world,
        PlanetSimulationState,
        32,
        seed_value: 0,
        step_counter: 13,
        year_value: 14,
        run_simulation: 24,
        num_colonies: 25
    );
    register_component_schema!(
        world,
        Settlement,
        id,
        face_index,
        faction_id,
        population,
        infrastructure,
        wealth,
        name_seed,
        is_capital
    );
    register_component_schema!(
        world,
        Settler,
        id,
        face_index,
        hunger,
        thirst,
        hue,
        cooldown,
        known_water_face,
        known_food_face,
        tribe_id,
        age,
        birth_cooldown,
        cooperation,
        aggression,
        mobility,
        render_slot,
        previous_face,
        move_commitment
    );
    register_component_schema!(world, SpaceRotation, speed);
    register_component_schema!(world, SpaceRotationTilt, x, z);
    register_component_schema!(world, NebulaRotation, index, init_x, init_y, init_z);

    let hemi = world.spawn();
    world.add_component(
        hemi,
        HemisphereLight {
            sky_color: [1.0, 1.0, 1.0],
            sky_intensity: 1.0,
            ground_color: [0.13333334, 0.2, 0.26666668],
            ground_intensity: 1.0,
        },
    );
    let amb = world.spawn();
    world.add_component(
        amb,
        AmbientLight {
            color: [0.2509804, 0.27058825, 0.3137255],
            intensity: 0.4,
        },
    );
    let sun = world.spawn();
    world.add_component(
        sun,
        DirectionalLight {
            color: [1.0, 0.99215686, 0.93333333],
            intensity: 1.8,
            direction: [-40.0, -30.0, -20.0],
            pad: 0.0,
        },
    );
    let backlight = world.spawn();
    world.add_component(
        backlight,
        DirectionalLight {
            color: [0.53333336, 0.6666667, 0.8],
            intensity: 1.8,
            direction: [40.0, 30.0, 20.0],
            pad: 0.0,
        },
    );

    let cam = world.spawn();
    world.add_component(
        cam,
        Transform {
            translation: [0.0, 0.0, 100.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
    );
    world.add_component(cam, GlobalTransform::default());
    let exposure = 1.2;
    world.add_component(
        cam,
        Camera3D {
            fov: 0.785398,
            aspect: 1.777,

            near: 1.0,
            far: 2000.0,
            exposure,
            ..Default::default()
        },
    );

    let planet = world.spawn();
    world.add_component(
        planet,
        Transform {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
    );
    world.add_component(planet, GlobalTransform::default());
    world.add_component(planet, PlanetConfig::default());
    world.add_component(planet, PlanetSimulationState::default());
    world.add_component(planet, SimTuning::default());
    world.add_component(planet, DynamicMesh::default());
    world.add_component(planet, MeshBVH::default());
    world.add_component(
        planet,
        StandardMaterial {
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.0, 0.0, 0.0],
            metallic: 0.1,
            roughness: 1.0,
            pad: [0.0; 3],
        },
    );

    let inner_glow = world.spawn();
    let outer_glow = world.spawn();
    world.add_component(planet, Children(vec![inner_glow]));

    world.add_component(inner_glow, Parent(planet));
    world.add_component(
        inner_glow,
        Transform {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
    );
    world.add_component(inner_glow, GlobalTransform::default());
    let (v_inner, i_inner) = mesh_icosphere_native(10.05, 4, true);
    let mut inner_mesh = DynamicMesh::default();
    inner_mesh.vertices = v_inner;
    inner_mesh.indices = i_inner;
    inner_mesh.version = 1;
    world.add_component(inner_glow, inner_mesh);
    world.add_component(
        inner_glow,
        StandardMaterial {
            base_color: [0.1, 0.6, 0.9, 1.0],
            emissive: [0.0, 0.0, 0.0],
            metallic: 0.0,
            roughness: 1.0,
            pad: [2.0, 0.0, 1.0],
        },
    );

    world.add_component(
        outer_glow,
        Transform {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
    );
    world.add_component(outer_glow, GlobalTransform::default());
    world.add_component(outer_glow, Billboard { active: 1 });
    let mut outer_mesh = DynamicMesh::default();
    outer_mesh.vertices = vec![
        -21.5, -21.5, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 21.5, -21.5, 0.0, 0.0, 0.0,
        1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 21.5, 21.5, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        1.0, -21.5, 21.5, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0,
    ];
    outer_mesh.indices = vec![0, 1, 2, 2, 3, 0];
    outer_mesh.version = 1;
    world.add_component(outer_glow, outer_mesh);
    world.add_component(
        outer_glow,
        StandardMaterial {
            base_color: [0.1568, 0.3921, 1.0, 0.28],
            emissive: [0.0, 0.0, 0.0],
            metallic: 0.0,
            roughness: 1.0,
            pad: [2.0, 2.5, 0.0],
        },
    );

    let mut rng = Lcg::new(54321);
    let scale_factor = 0.40;

    let space_background = world.spawn();
    world.add_component(
        space_background,
        Transform {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
    );
    world.add_component(space_background, GlobalTransform::default());
    world.add_component(space_background, SpaceRotation { speed: 0.0012 });

    let mut space_children = Vec::new();

    let galaxy_ring = world.spawn();
    space_children.push(galaxy_ring);
    world.add_component(galaxy_ring, Parent(space_background));
    world.add_component(
        galaxy_ring,
        Transform {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
    );
    world.add_component(galaxy_ring, GlobalTransform::default());
    world.add_component(galaxy_ring, SpaceRotation { speed: 0.0009 });
    world.add_component(
        galaxy_ring,
        SpaceRotationTilt {
            x: std::f32::consts::PI * 0.18,
            z: std::f32::consts::PI * 0.08,
        },
    );
    let ring_mesh = generate_galaxy_ring_mesh(
        2000,
        (8.0 * scale_factor) / dpi_scale,
        0xffffff,
        0.45,
        400.0,
        800.0,
        160.0,
        &mut rng,
    );
    world.add_component(galaxy_ring, ring_mesh);
    world.add_component(
        galaxy_ring,
        StandardMaterial {
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.0, 0.0, 0.0],
            metallic: 0.0,
            roughness: 1.0,
            pad: [2.0, 2.0, 0.0],
        },
    );

    let star_configs = [
        (
            3500,
            (2.0 * scale_factor) / dpi_scale,
            0xffffff,
            0.65,
            400.0,
            1800.0,
            0.00024,
        ),
        (
            2000,
            (3.8 * scale_factor) / dpi_scale,
            0xaaccff,
            0.75,
            350.0,
            1400.0,
            0.00048,
        ),
        (
            1000,
            (6.0 * scale_factor) / dpi_scale,
            0xffeedd,
            0.60,
            300.0,
            1100.0,
            0.00072,
        ),
        (
            200,
            (11.0 * scale_factor) / dpi_scale,
            0xffffff,
            0.95,
            300.0,
            900.0,
            0.00096,
        ),
    ];
    for (count, size, color_hex, opacity, r_min, r_max, speed) in star_configs {
        let star_layer = world.spawn();
        space_children.push(star_layer);
        world.add_component(star_layer, Parent(space_background));
        world.add_component(
            star_layer,
            Transform {
                translation: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
        );
        world.add_component(star_layer, GlobalTransform::default());
        world.add_component(star_layer, SpaceRotation { speed });
        let mesh =
            generate_star_layer_mesh(count, size, color_hex, opacity, r_min, r_max, &mut rng);
        world.add_component(star_layer, mesh);
        world.add_component(
            star_layer,
            StandardMaterial {
                base_color: [1.0, 1.0, 1.0, 1.0],
                emissive: [0.0, 0.0, 0.0],
                metallic: 0.0,
                roughness: 1.0,
                pad: [2.0, 2.0, 0.0],
            },
        );
    }

    let nebula_configs = [
        (1000, 350.0, 0x051a4a, 0.045, 0.0, 0.0, 0.0),
        (700, 500.0, 0x1a054a, 0.035, 0.4, 1.2, 0.3),
        (500, 420.0, 0x054a32, 0.024, -0.3, -1.0, 0.8),
        (350, 600.0, 0x4a2a05, 0.018, 1.1, 0.2, -0.5),
    ];
    for (idx, (count, size_raw, color_hex, opacity, rx, ry, rz)) in
        nebula_configs.into_iter().enumerate()
    {
        let nebula_layer = world.spawn();
        space_children.push(nebula_layer);
        world.add_component(nebula_layer, Parent(space_background));
        world.add_component(
            nebula_layer,
            Transform {
                translation: [0.0, 0.0, 0.0],
                rotation: glam::Quat::from_euler(glam::EulerRot::XYZ, rx, ry, rz).to_array(),
                scale: [1.0, 1.0, 1.0],
            },
        );
        world.add_component(nebula_layer, GlobalTransform::default());
        world.add_component(
            nebula_layer,
            NebulaRotation {
                index: idx as f32,
                init_x: rx,
                init_y: ry,
                init_z: rz,
            },
        );
        let mesh = generate_nebula_cloud_mesh(
            count,
            (size_raw * scale_factor) / dpi_scale,
            color_hex,
            opacity,
            &mut rng,
        );
        world.add_component(nebula_layer, mesh);
        world.add_component(
            nebula_layer,
            StandardMaterial {
                base_color: [1.0, 1.0, 1.0, 1.0],
                emissive: [0.0, 0.0, 0.0],
                metallic: 0.0,
                roughness: 1.0,
                pad: [2.0, 3.0, 0.0],
            },
        );
    }

    world.add_component(space_background, Children(space_children));

    engine
}

#[wasm_bindgen]
pub fn wasm_admin_set_food_regen(engine: &mut WasmEngine, multiplier: f32) {
    let world = &mut engine.app_mut().world;
    let Some(planet) = find_planet_entity(world) else {
        return;
    };
    let scale = 0.8 * multiplier.clamp(0.0, 4.0);
    if let Some(tuning) = world.get_component_mut::<SimTuning>(planet) {
        tuning.food_regen_scale = scale;
    }
    if let Some(state) = world.get_component_mut::<PlanetSimulationState>(planet) {
        for i in 0..state.food_regen.len().min(state.arability.len()) {
            state.food_regen[i] = state.arability[i] * scale;
        }
    }
}

#[wasm_bindgen]
pub fn wasm_admin_start_drought(engine: &mut WasmEngine, face_id: usize) -> bool {
    let world = &mut engine.app_mut().world;
    let Some(planet) = find_planet_entity(world) else {
        return false;
    };
    let Some(state) = world.get_component_mut::<PlanetSimulationState>(planet) else {
        return false;
    };
    if face_id >= state.is_water.len() || state.is_water[face_id] > 0.5 {
        return false;
    }
    state.droughts.push(DroughtEvent {
        center_face: face_id as u32,
        radius: 7,
        remaining: 90.0,
        strength: 0.9,
    });
    true
}

#[wasm_bindgen]
pub fn wasm_admin_clear_droughts(engine: &mut WasmEngine) {
    let world = &mut engine.app_mut().world;
    let Some((state_ptr, mesh_ptr)) = find_planet_mut(world) else {
        return;
    };
    let state = unsafe { &mut *state_ptr };
    let mesh = unsafe { &mut *mesh_ptr };
    restore_admin_preview(state, mesh);
    let affected: Vec<usize> = (0..state.drought.len())
        .filter(|&face| state.drought[face] > 0.0)
        .collect();
    state.droughts.clear();
    state.drought.fill(0.0);
    state.admin_drought.fill(0.0);
    for face in affected {
        if face < mesh.indices.len() / 3 {
            let color = admin_display_face_color(state, mesh, face);
            set_mesh_face_color(mesh, face, color);
        }
    }
    mesh.color_version = mesh.color_version.wrapping_add(1);
}

#[wasm_bindgen]
pub fn wasm_admin_paint(
    engine: &mut WasmEngine,
    center_face: usize,
    tool: u32,
    radius: u32,
) -> Vec<u32> {
    let world = &mut engine.app_mut().world;
    let Some(planet) = find_planet_entity(world) else {
        return vec![0, 0];
    };
    let tuning = world
        .get_component::<SimTuning>(planet)
        .copied()
        .unwrap_or_default();
    let (state_ptr, mesh_ptr) = match find_planet_mut(world) {
        Some(p) => p,
        None => return vec![0, 0],
    };
    let state = unsafe { &mut *state_ptr };
    let mesh = unsafe { &mut *mesh_ptr };
    let num_faces = mesh.indices.len() / 3;
    if center_face >= num_faces
        || state.neighbors_offsets.len() < num_faces + 1
        || state.is_water.len() < num_faces
        || state.arability.len() < num_faces
        || state.moistures.len() < num_faces
        || state.temps.len() < num_faces
        || state.elevations.len() < num_faces
        || state.food_cap.len() < num_faces
        || state.inv_food_cap.len() < num_faces
        || state.food_regen.len() < num_faces
        || state.food_stock.len() < num_faces
        || state.face_centers.len() < num_faces * 3
        || state.vertex_sources.len() < mesh.vertices.len() / 12
    {
        return vec![0, 0];
    }

    restore_admin_preview(state, mesh);
    let brush_radius = radius.min(6);
    let face_steps = admin_brush_faces_with_distance(state, center_face, brush_radius, num_faces);
    let faces: Vec<usize> = face_steps.iter().map(|&(face, _)| face).collect();

    if state.admin_drought.len() < num_faces {
        state.admin_drought.resize(num_faces, 0.0);
    }
    if state.drought.len() < num_faces {
        state.drought.resize(num_faces, 0.0);
    }
    let mut changed = 0u32;
    let mut skipped = 0u32;
    let mut terrain_changed = false;
    let mut edited_terrain_faces = Vec::new();
    let mut new_people = Vec::new();
    let brush_tribe = (tool == 4).then(|| {
        faces
            .iter()
            .filter_map(|&face| {
                let own = state.face_dominant_tribe.get(face).copied().unwrap_or(-1);
                if own >= 0 {
                    return Some(own as f32);
                }
                let start = state.neighbors_offsets[face] as usize;
                let end =
                    (state.neighbors_offsets[face + 1] as usize).min(state.neighbors_flat.len());
                (start..end)
                    .map(|i| state.neighbors_flat[i] as usize)
                    .find_map(|neighbor| {
                        let tribe = state
                            .face_dominant_tribe
                            .get(neighbor)
                            .copied()
                            .unwrap_or(-1);
                        (tribe >= 0).then_some(tribe as f32)
                    })
            })
            .next()
            .unwrap_or_else(|| {
                let tribe = state.next_tribe_id;
                state.next_tribe_id += 1.0;
                tribe
            })
    });

    for (face, distance) in face_steps {
        let is_water = state.is_water.get(face).copied().unwrap_or(1.0) > 0.5;
        match tool {
            0 if !is_water => {
                state.admin_drought[face] = 0.95;
                state.drought[face] = state.drought[face].max(0.95);
                let color = admin_display_face_color(state, mesh, face);
                set_admin_face_color(state, mesh, face, color, false);
                changed += 1;
            }
            1 if !is_water => {
                state.food_stock[face] = state.food_cap[face];
                let color = admin_display_face_color(state, mesh, face);
                set_admin_face_color(state, mesh, face, color, false);
                changed += 1;
            }
            2 if is_water => {
                let (mut elevation, color, temp, moisture, arability, minerals) =
                    adaptive_terrain_values(state, mesh, face, false);
                let strength = smooth_admin_brush_strength(distance, brush_radius);
                let peak = elevation.max(0.045).min(0.14);
                elevation = 0.009 + (peak - 0.009) * strength;
                state.is_water[face] = 0.0;
                state.arability[face] = arability;
                state.moistures[face] = moisture;
                state.temps[face] = temp;
                state.minerals[face] = minerals;
                state.elevations[face] = elevation;
                let cap = state.arability[face] * tuning.food_cap_scale;
                state.food_cap[face] = cap;
                state.inv_food_cap[face] = 1.0 / cap.max(0.001);
                state.food_regen[face] = state.arability[face] * tuning.food_regen_scale;
                state.food_stock[face] = cap;
                set_admin_face_color(state, mesh, face, color, true);
                edited_terrain_faces.push(face);
                changed += 1;
                terrain_changed = true;
            }
            3 if !is_water => {
                if state.face_population.get(face).copied().unwrap_or(0) > 0 {
                    skipped += 1;
                    continue;
                }
                let (mut elevation, color, _, _, _, _) =
                    adaptive_terrain_values(state, mesh, face, true);
                let strength = smooth_admin_brush_strength(distance, brush_radius);
                let depth = (-elevation).max(0.045).min(0.16);
                elevation = -0.008 - (depth - 0.008) * strength;
                state.is_water[face] = 1.0;
                state.arability[face] = 0.0;
                state.minerals[face] = 0.0;
                state.elevations[face] = elevation;
                state.food_cap[face] = 0.0;
                state.inv_food_cap[face] = 0.0;
                state.food_regen[face] = 0.0;
                state.food_stock[face] = 0.0;
                state.admin_drought[face] = 0.0;
                state.drought[face] = 0.0;
                set_admin_face_color(state, mesh, face, color, true);
                edited_terrain_faces.push(face);
                changed += 1;
                terrain_changed = true;
            }
            4 if !is_water => {
                if state.face_population.get(face).copied().unwrap_or(0) > 0 {
                    skipped += 1;
                    continue;
                }
                let id = state.next_settler_id;
                state.next_settler_id += 1.0;
                let center = [
                    state.face_centers[face * 3],
                    state.face_centers[face * 3 + 1],
                    state.face_centers[face * 3 + 2],
                ];
                let tribe = brush_tribe.unwrap_or(0.0);
                new_people.push((id, face, tribe, center));
                if let Some(population) = state.face_population.get_mut(face) {
                    *population = 1;
                }
                if let Some(dominant) = state.face_dominant_tribe.get_mut(face) {
                    *dominant = tribe as i32;
                }
                changed += 1;
            }
            _ => {}
        }
    }

    if terrain_changed {
        rebuild_admin_terrain_geometry(state, mesh, &edited_terrain_faces, tool == 2);
        apply_admin_beaches(state, mesh, &edited_terrain_faces);
        state.dist_to_water = compute_distance_to_water(
            num_faces,
            &state.is_water,
            &state.neighbors_offsets,
            &state.neighbors_flat,
        );
        state.has_adjacent_water.resize(num_faces, 0);
        for face in 0..num_faces {
            if state.is_water[face] > 0.5 {
                state.has_adjacent_water[face] = 1;
                continue;
            }
            let start = state.neighbors_offsets[face] as usize;
            let end = (state.neighbors_offsets[face + 1] as usize).min(state.neighbors_flat.len());
            state.has_adjacent_water[face] =
                (start..end).any(|i| state.is_water[state.neighbors_flat[i] as usize] > 0.5) as u8;
        }
    }
    if changed > 0 {
        if terrain_changed {

            mesh.version = mesh.version.wrapping_add(1);
        } else {
            mesh.color_version = mesh.color_version.wrapping_add(1);
        }
        state.face_color_tick_accum = 0.0;
    }
    let settler_mesh_id = state.settler_mesh_id;
    let geometry_version = terrain_changed.then_some(mesh.version);
    for (id, face, tribe, center) in new_people {
        spawn_admin_settler(world, id, face, tribe, center, settler_mesh_id);
    }
    if let Some(version) = geometry_version {

        if let Some(config) = world.get_component_mut::<PlanetConfig>(planet) {
            config.version = version as f32;
        }
    }
    vec![changed, skipped]
}

#[wasm_bindgen]
pub fn wasm_admin_preview(engine: &mut WasmEngine, center_face: i32, tool: u32, radius: u32) {
    let world = &mut engine.app_mut().world;
    let (state_ptr, mesh_ptr) = match find_planet_mut(world) {
        Some(p) => p,
        None => return,
    };
    let state = unsafe { &mut *state_ptr };
    let mesh = unsafe { &mut *mesh_ptr };
    let restored = restore_admin_preview(state, mesh);
    let num_faces = mesh.indices.len() / 3;
    if center_face < 0 || center_face as usize >= num_faces {
        if restored {
            mesh.color_version = mesh.color_version.wrapping_add(1);
        }
        return;
    }
    let preview_color = match tool {
        0 => hex_to_linear_rgb(0xffa22e),
        1 => hex_to_linear_rgb(0x65ff72),
        2 => hex_to_linear_rgb(0x9dff69),
        3 => hex_to_linear_rgb(0x35cfff),
        4 => hex_to_linear_rgb(0xff70d2),
        _ => [1.0, 1.0, 1.0],
    };
    let preview_radius = radius.min(6);
    let faces =
        admin_brush_faces_with_distance(state, center_face as usize, preview_radius, num_faces);
    for (face, distance) in faces {
        let water = state.is_water.get(face).copied().unwrap_or(1.0) > 0.5;
        let occupied = state.face_population.get(face).copied().unwrap_or(0) > 0;
        let effective = match tool {
            0 | 1 => !water,
            2 => water,
            3 => !water && !occupied,
            4 => !water && !occupied,
            _ => false,
        };
        if !effective {
            continue;
        }
        let base = admin_display_face_color(state, mesh, face);
        let strength = smooth_admin_brush_strength(distance, preview_radius);
        let mix = 0.28 + strength * 0.44;
        let color = [
            (base[0] * (1.0 - mix) + preview_color[0] * mix).min(1.0),
            (base[1] * (1.0 - mix) + preview_color[1] * mix).min(1.0),
            (base[2] * (1.0 - mix) + preview_color[2] * mix).min(1.0),
        ];
        set_mesh_face_color(mesh, face, color);
        state.admin_preview_faces.push(face as u32);
    }
    mesh.color_version = mesh.color_version.wrapping_add(1);
}

fn admin_brush_faces_with_distance(
    state: &PlanetSimulationState,
    center_face: usize,
    radius: u32,
    num_faces: usize,
) -> Vec<(usize, u32)> {
    use std::collections::{HashSet, VecDeque};
    let mut faces = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    seen.insert(center_face);
    queue.push_back((center_face, 0));
    while let Some((face, distance)) = queue.pop_front() {
        faces.push((face, distance));
        if distance >= radius || face + 1 >= state.neighbors_offsets.len() {
            continue;
        }
        let start = state.neighbors_offsets[face] as usize;
        let end = (state.neighbors_offsets[face + 1] as usize).min(state.neighbors_flat.len());
        for index in start..end {
            let next = state.neighbors_flat[index] as usize;
            if next < num_faces && seen.insert(next) {
                queue.push_back((next, distance + 1));
            }
        }
    }
    faces
}

fn smooth_admin_brush_strength(distance: u32, radius: u32) -> f32 {
    if radius == 0 {
        return 1.0;
    }
    let t = (1.0 - distance as f32 / (radius + 1) as f32).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn restore_admin_preview(state: &mut PlanetSimulationState, mesh: &mut DynamicMesh) -> bool {
    let previous = std::mem::take(&mut state.admin_preview_faces);
    let had_preview = !previous.is_empty();
    for face in previous {
        let face = face as usize;
        if face < mesh.indices.len() / 3 {
            let color = admin_display_face_color(state, mesh, face);
            set_mesh_face_color(mesh, face, color);
        }
    }
    had_preview
}

fn admin_display_face_color(
    state: &PlanetSimulationState,
    mesh: &DynamicMesh,
    face: usize,
) -> [f32; 3] {
    let vertex = mesh.indices[face * 3] as usize;
    let offset = vertex * 3;
    let mut color = [
        state.base_colors.get(offset).copied().unwrap_or(0.2),
        state.base_colors.get(offset + 1).copied().unwrap_or(0.2),
        state.base_colors.get(offset + 2).copied().unwrap_or(0.2),
    ];
    if state.is_water.get(face).copied().unwrap_or(1.0) < 0.5 {
        let drought = state
            .drought
            .get(face)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        if drought > 0.0 {
            let dry = hex_to_linear_rgb(0x8d6238);
            let mix = drought * 0.72;
            for channel in 0..3 {
                color[channel] = color[channel] * (1.0 - mix) + dry[channel] * mix;
            }
        }
    }
    color
}

fn adaptive_terrain_values(
    state: &PlanetSimulationState,
    mesh: &DynamicMesh,
    face: usize,
    want_water: bool,
) -> (f32, [f32; 3], f32, f32, f32, f32) {
    let start = state.neighbors_offsets[face] as usize;
    let end = (state.neighbors_offsets[face + 1] as usize).min(state.neighbors_flat.len());
    let mut count = 0.0f32;
    let mut elevation = 0.0;
    let mut temp = 0.0;
    let mut moisture = 0.0;
    let mut arability = 0.0;
    let mut minerals = 0.0;
    let mut color = [0.0; 3];
    for index in start..end {
        let neighbor = state.neighbors_flat[index] as usize;
        let neighbor_water = state.is_water.get(neighbor).copied().unwrap_or(1.0) > 0.5;
        if neighbor_water != want_water {
            continue;
        }
        count += 1.0;
        elevation += state.elevations[neighbor];
        temp += state.temps[neighbor];
        moisture += state.moistures[neighbor];
        arability += state.arability[neighbor];
        minerals += state.minerals[neighbor];
        let vertex = mesh.indices[neighbor * 3] as usize;
        for channel in 0..3 {
            color[channel] += state.base_colors[vertex * 3 + channel];
        }
    }
    if count > 0.0 {
        elevation /= count;
        temp /= count;
        moisture /= count;
        arability /= count;
        minerals /= count;
        for channel in &mut color {
            *channel /= count;
        }
    } else {
        temp = state.temps[face].clamp(0.0, 1.0);
        moisture = state.moistures[face].clamp(0.0, 1.0);
        if want_water {
            elevation = -0.04;
            color = hex_to_linear_rgb(0x146c91);
        } else {
            elevation = 0.035;
            let comfort = (1.0 - (temp - 0.5).abs() * 2.0).max(0.0);
            arability = (comfort * moisture).powf(1.25).clamp(0.08, 0.85);
            minerals = 0.25;
            color = bilinear_interpolate_biome(moisture, temp);
        }
    }
    if want_water {
        elevation = elevation.clamp(-0.18, -0.012);
        let deep = hex_to_linear_rgb(0x0a4073);
        let shallow = hex_to_linear_rgb(0x146c91);
        let shallow_mix = (1.0 - (-elevation / 0.18)).clamp(0.0, 1.0);
        let generated = [
            deep[0] + (shallow[0] - deep[0]) * shallow_mix,
            deep[1] + (shallow[1] - deep[1]) * shallow_mix,
            deep[2] + (shallow[2] - deep[2]) * shallow_mix,
        ];
        if count > 0.0 {
            for channel in 0..3 {
                color[channel] = color[channel] * 0.62 + generated[channel] * 0.38;
            }
        } else {
            color = generated;
        }
        apply_admin_color_variation(state.seed_value, face, &mut color, 0.022);
        (elevation, color, temp, moisture, 0.0, 0.0)
    } else {
        elevation = elevation.clamp(0.012, 0.22);
        arability = arability.clamp(0.08, 0.9);
        let generated = bilinear_interpolate_biome(moisture, temp);
        if count > 0.0 {
            for channel in 0..3 {
                color[channel] = color[channel] * 0.68 + generated[channel] * 0.32;
            }
        } else {
            color = generated;
        }
        apply_admin_color_variation(state.seed_value, face, &mut color, 0.065);
        (elevation, color, temp, moisture, arability, minerals)
    }
}

fn apply_admin_color_variation(seed: u32, face: usize, color: &mut [f32; 3], amount: f32) {
    let hash =
        (((face as f32 + 1.0) * 12.9898 + seed as f32 * 0.000_137).sin() * 43_758.547).fract();
    let shade = hash * amount;
    for channel in color {
        *channel = (*channel + shade).clamp(0.0, 1.0);
    }
}

fn rebuild_admin_terrain_geometry(
    state: &mut PlanetSimulationState,
    mesh: &mut DynamicMesh,
    edited_faces: &[usize],
    raising_land: bool,
) {
    use std::collections::{HashMap, HashSet};

    let mut touched_sources = HashSet::new();
    for &face in edited_faces {
        for k in 0..3 {
            if let Some(&source) = state.vertex_sources.get(face * 3 + k) {
                touched_sources.insert(source);
            }
        }
    }
    if touched_sources.is_empty() {
        return;
    }

    let edited: HashSet<usize> = edited_faces.iter().copied().collect();

    let edited_weight = if raising_land { 3.0 } else { 9.0 };
    let mut source_accum: HashMap<u32, (f32, f32, u32, u32)> = HashMap::new();
    for (flat_vertex, &source) in state.vertex_sources.iter().enumerate() {
        if !touched_sources.contains(&source) {
            continue;
        }
        let face = flat_vertex / 3;
        let Some(&height) = state.elevations.get(face) else {
            continue;
        };
        let is_edited = edited.contains(&face);
        let weight = if is_edited { edited_weight } else { 1.0 };
        let entry = source_accum.entry(source).or_insert((0.0, 0.0, 0, 0));
        entry.0 += height * weight;
        entry.1 += weight;
        if is_edited {
            entry.2 += 1;
        } else {
            entry.3 += 1;
        }
    }

    for (flat_vertex, &source) in state.vertex_sources.iter().enumerate() {
        let Some(&(height_sum, weight_sum, edited_count, untouched_count)) =
            source_accum.get(&source)
        else {
            continue;
        };
        let mut height = height_sum / weight_sum.max(0.001);
        if edited_count > 0 && untouched_count > 0 {

            height = if raising_land {
                height.min(0.0)
            } else {
                height.clamp(-0.006, 0.006)
            };
        }
        let offset = flat_vertex * 12;
        let x = mesh.vertices[offset];
        let y = mesh.vertices[offset + 1];
        let z = mesh.vertices[offset + 2];
        let length = (x * x + y * y + z * z).sqrt().max(0.001);
        let radius = 10.0 + height;
        mesh.vertices[offset] = x / length * radius;
        mesh.vertices[offset + 1] = y / length * radius;
        mesh.vertices[offset + 2] = z / length * radius;
    }

    artisan::engine::mesh::recalculate_normals_raw(&mut mesh.vertices, &mesh.indices);
    let num_faces = mesh.indices.len() / 3;
    state.face_centers.resize(num_faces * 3, 0.0);
    for face in 0..num_faces {
        for channel in 0..3 {
            let mut center = 0.0;
            for k in 0..3 {
                let vertex = mesh.indices[face * 3 + k] as usize;
                center += mesh.vertices[vertex * 12 + channel];
            }
            state.face_centers[face * 3 + channel] = center / 3.0;
        }
    }
}

fn apply_admin_beaches(
    state: &mut PlanetSimulationState,
    mesh: &mut DynamicMesh,
    edited_faces: &[usize],
) {
    use std::collections::HashSet;
    let num_faces = mesh.indices.len() / 3;
    let mut candidates: HashSet<usize> = edited_faces.iter().copied().collect();
    for &face in edited_faces {
        if face + 1 >= state.neighbors_offsets.len() {
            continue;
        }
        let start = state.neighbors_offsets[face] as usize;
        let end = (state.neighbors_offsets[face + 1] as usize).min(state.neighbors_flat.len());
        for index in start..end {
            let neighbor = state.neighbors_flat[index] as usize;
            if neighbor < num_faces {
                candidates.insert(neighbor);
            }
        }
    }

    for face in candidates {
        if state.is_water.get(face).copied().unwrap_or(1.0) > 0.5
            || state.temps.get(face).copied().unwrap_or(0.0) < 0.2
        {
            continue;
        }
        let rendered_height = mesh_face_average_radius(mesh, face) - 10.0;

        let t = ((0.055 - rendered_height) / 0.045).clamp(0.0, 1.0);
        let beach_mix = t * t * (3.0 - 2.0 * t);
        if beach_mix <= 0.01 {
            continue;
        }
        let vertex = mesh.indices[face * 3] as usize;
        let offset = vertex * 3;
        let base = [
            state.base_colors.get(offset).copied().unwrap_or(0.3),
            state.base_colors.get(offset + 1).copied().unwrap_or(0.3),
            state.base_colors.get(offset + 2).copied().unwrap_or(0.3),
        ];
        let mut sand = hex_to_linear_rgb(0xe0cd94);
        apply_admin_color_variation(state.seed_value ^ 0x9e37_79b9, face, &mut sand, 0.045);
        let color = [
            base[0] + (sand[0] - base[0]) * beach_mix,
            base[1] + (sand[1] - base[1]) * beach_mix,
            base[2] + (sand[2] - base[2]) * beach_mix,
        ];
        set_admin_face_color(state, mesh, face, color, true);
    }
}

fn mesh_face_average_radius(mesh: &DynamicMesh, face: usize) -> f32 {
    let mut radius = 0.0;
    for k in 0..3 {
        let vertex = mesh.indices[face * 3 + k] as usize;
        let offset = vertex * 12;
        let x = mesh.vertices[offset];
        let y = mesh.vertices[offset + 1];
        let z = mesh.vertices[offset + 2];
        radius += (x * x + y * y + z * z).sqrt();
    }
    radius / 3.0
}

fn set_admin_face_color(
    state: &mut PlanetSimulationState,
    mesh: &mut DynamicMesh,
    face: usize,
    color: [f32; 3],
    update_base: bool,
) {
    for k in 0..3 {
        let vertex = mesh.indices[face * 3 + k] as usize;
        let mesh_offset = vertex * 12;
        let base_offset = vertex * 3;
        if mesh_offset + 10 < mesh.vertices.len() && base_offset + 2 < state.base_colors.len() {
            mesh.vertices[mesh_offset + 8..mesh_offset + 11].copy_from_slice(&color);
            if update_base {
                state.base_colors[base_offset..base_offset + 3].copy_from_slice(&color);
            }
        }
    }
}

fn set_mesh_face_color(mesh: &mut DynamicMesh, face: usize, color: [f32; 3]) {
    for k in 0..3 {
        let vertex = mesh.indices[face * 3 + k] as usize;
        let offset = vertex * 12;
        if offset + 10 < mesh.vertices.len() {
            mesh.vertices[offset + 8..offset + 11].copy_from_slice(&color);
        }
    }
}

fn spawn_admin_settler(
    world: &mut World,
    id: f32,
    face: usize,
    tribe: f32,
    center: [f32; 3],
    settler_mesh_id: f32,
) {
    let length = (center[0] * center[0] + center[1] * center[1] + center[2] * center[2])
        .sqrt()
        .max(0.001);
    let position = [
        center[0] * (1.0 + 0.12 / length),
        center[1] * (1.0 + 0.12 / length),
        center[2] * (1.0 + 0.12 / length),
    ];
    let (hue, color) = apply_tribe_color(tribe);
    let entity = world.spawn();
    world.add_component(
        entity,
        Transform {
            translation: position,
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [0.018, 0.018, 0.018],
        },
    );
    world.add_component(entity, GlobalTransform::default());
    world.add_component(entity, Billboard { active: 1 });
    world.add_component(
        entity,
        Settler {
            id,
            face_index: face as f32,
            hunger: 100.0,
            thirst: 100.0,
            hue,
            cooldown: 0.0,
            known_water_face: -1.0,
            known_food_face: -1.0,
            tribe_id: tribe,
            age: 18.0,
            birth_cooldown: 20.0,
            cooperation: 0.55,
            aggression: 0.32,
            mobility: 0.58,
            render_slot: (id * 0.6180339).fract().abs() * 20000.0,
            previous_face: -1.0,
            move_commitment: 0.0,
        },
    );
    world.add_component(
        entity,
        StandardMaterial {
            base_color: color,
            roughness: 0.6,
            metallic: 0.1,
            ..Default::default()
        },
    );
    if settler_mesh_id >= 0.0 {
        world.add_component(
            entity,
            MeshHandle {
                id: settler_mesh_id,
            },
        );
    } else {
        let (vertices, indices) = mesh_circle_marker(10, 1.0);
        world.add_component(
            entity,
            DynamicMesh {
                vertices,
                indices,
                version: 1,
                color_version: 0,
            },
        );
    }
}

#[wasm_bindgen]
pub fn wasm_admin_refill_face(engine: &mut WasmEngine, face_id: usize) -> bool {
    let world = &mut engine.app_mut().world;
    let Some(planet) = find_planet_entity(world) else {
        return false;
    };
    let Some(state) = world.get_component_mut::<PlanetSimulationState>(planet) else {
        return false;
    };
    let Some(cap) = state.food_cap.get(face_id).copied() else {
        return false;
    };
    if cap <= 0.0 {
        return false;
    }
    if let Some(stock) = state.food_stock.get_mut(face_id) {
        *stock = cap;
        true
    } else {
        false
    }
}

#[wasm_bindgen]
pub fn wasm_admin_enrich_face(engine: &mut WasmEngine, face_id: usize) -> bool {
    let world = &mut engine.app_mut().world;
    let Some(planet) = find_planet_entity(world) else {
        return false;
    };
    let scale = world
        .get_component::<SimTuning>(planet)
        .map(|t| (t.food_cap_scale, t.food_regen_scale))
        .unwrap_or((120.0, 8.0));
    let Some(state) = world.get_component_mut::<PlanetSimulationState>(planet) else {
        return false;
    };
    if face_id >= state.is_water.len() || state.is_water[face_id] > 0.5 {
        return false;
    }
    state.arability[face_id] = 1.0;
    state.food_cap[face_id] = scale.0;
    state.inv_food_cap[face_id] = 1.0 / scale.0.max(0.001);
    state.food_regen[face_id] = scale.1;
    state.food_stock[face_id] = scale.0;
    true
}

#[wasm_bindgen]
pub fn wasm_active_drought_count(engine: &WasmEngine) -> u32 {
    find_planet_state(&engine.app().world)
        .map(|state| state.droughts.len() as u32)
        .unwrap_or(0)
}

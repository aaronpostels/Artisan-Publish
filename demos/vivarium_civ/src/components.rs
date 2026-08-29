#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct PlanetConfig {
    pub seed: f32,
    pub continent_scale: f32,
    pub warp_amount: f32,
    pub polar_land: f32,
    pub water_level: f32,
    pub base_height: f32,
    pub hill_height: f32,
    pub mountain_density: f32,
    pub mountain_scale: f32,
    pub mountain_height: f32,
    pub global_moisture: f32,
    pub latitude_bands: f32,
    pub weather_warp: f32,
    pub moisture_scale: f32,
    pub lapse_rate: f32,
    pub version: f32,
}

impl Default for PlanetConfig {
    fn default() -> Self {
        Self {
            seed: 12345.0,
            continent_scale: 0.9,
            warp_amount: 0.3,
            polar_land: 0.2,
            water_level: 0.0,
            base_height: 0.15,
            hill_height: 0.15,
            mountain_density: 0.6,
            mountain_scale: 2.3,
            mountain_height: 1.7,
            global_moisture: 0.5,
            latitude_bands: 0.6,
            weather_warp: 1.2,
            moisture_scale: 1.5,
            lapse_rate: 0.4,
            version: 1.0,
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct PlanetSimulationState {
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

    pub dist_to_water: Vec<u32>,

    pub face_centers: Vec<f32>,

    pub food_cap: Vec<f32>,
    pub food_regen: Vec<f32>,
    pub food_stock: Vec<f32>,

    pub resource_tick_accum: f32,

    pub next_settler_id: f32,

    pub births_total: u32,
    pub deaths_starved: u32,
    pub deaths_aged: u32,

    pub settler_mesh_id: f32,

    pub house_face: Vec<u8>,
    pub house_colony_of_face: Vec<i32>,
    pub house_face_list: Vec<u32>,
    pub has_house_buff: Vec<u8>,

    pub face_colony: Vec<i32>,

    pub colony_population: Vec<u32>,
    pub colony_houses: Vec<u32>,

    pub colony_best_face: Vec<i32>,
    pub house_tick_accum: f32,
    pub territory_tick_accum: f32,
    pub houses_total: u32,

    pub colony_territory_faces: Vec<u32>,

    pub drought: Vec<f32>,
    pub droughts: Vec<DroughtEvent>,
    pub drought_tick_accum: f32,

    pub face_population: Vec<u32>,
    pub face_dominant_tribe: Vec<i32>,

    pub next_tribe_id: f32,

    pub cooperation_events: u32,
    pub aggression_events: u32,
    pub tribe_splits: u32,

    pub face_color_tick_accum: f32,

    pub tribe_dynamics_tick_accum: f32,

    pub bench_system_mask: u32,

    pub has_adjacent_water: Vec<u8>,

    pub inv_food_cap: Vec<f32>,

    pub admin_drought: Vec<f32>,

    pub admin_preview_faces: Vec<u32>,

    pub vertex_sources: Vec<u32>,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct DroughtEvent {
    pub center_face: u32,
    pub radius: u32,
    pub remaining: f32,
    pub strength: f32,
}

impl Default for PlanetSimulationState {
    fn default() -> Self {
        Self {
            seed_value: 12345,
            face_owner: Vec::new(),
            face_score: Vec::new(),
            faction_colors: Vec::new(),
            faction_tech: Vec::new(),
            step_counter: 0,
            year_value: 1,
            neighbors_flat: Vec::new(),
            neighbors_offsets: Vec::new(),
            base_colors: Vec::new(),
            run_simulation: 0.0,
            num_colonies: 10.0,
            is_water: Vec::new(),
            arability: Vec::new(),
            minerals: Vec::new(),
            temps: Vec::new(),
            moistures: Vec::new(),
            elevations: Vec::new(),
            dist_to_water: Vec::new(),
            face_centers: Vec::new(),
            food_cap: Vec::new(),
            food_regen: Vec::new(),
            food_stock: Vec::new(),
            resource_tick_accum: 0.0,
            next_settler_id: 0.0,
            births_total: 0,
            deaths_starved: 0,
            deaths_aged: 0,
            settler_mesh_id: -1.0,
            house_face: Vec::new(),
            house_colony_of_face: Vec::new(),
            house_face_list: Vec::new(),
            has_house_buff: Vec::new(),
            face_colony: Vec::new(),
            colony_population: Vec::new(),
            colony_houses: Vec::new(),
            colony_best_face: Vec::new(),
            house_tick_accum: 0.0,
            territory_tick_accum: 0.0,
            houses_total: 0,
            colony_territory_faces: Vec::new(),
            drought: Vec::new(),
            droughts: Vec::new(),
            drought_tick_accum: 0.0,
            face_population: Vec::new(),
            face_dominant_tribe: Vec::new(),
            next_tribe_id: 0.0,
            cooperation_events: 0,
            aggression_events: 0,
            tribe_splits: 0,
            face_color_tick_accum: 0.0,
            tribe_dynamics_tick_accum: 0.0,
            bench_system_mask: u32::MAX,
            has_adjacent_water: Vec::new(),
            inv_food_cap: Vec::new(),
            admin_drought: Vec::new(),
            admin_preview_faces: Vec::new(),
            vertex_sources: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct SimTuning {
    pub hunger_per_sec: f32,
    pub thirst_per_sec: f32,
    pub cold_line: f32,
    pub hot_line: f32,
    pub climate_penalty_mult: f32,
    pub rest_threshold: f32,
    pub move_speed: f32,
    pub move_hunger_cost: f32,
    pub move_thirst_cost: f32,
    pub idle_wander_interval: f32,
    pub wander_cost_mult: f32,

    pub food_cap_scale: f32,
    pub food_regen_scale: f32,
    pub food_eat_rate: f32,

    pub maturity_age: f32,
    pub lifespan: f32,
    pub birth_need: f32,
    pub birth_cooldown_interval: f32,
    pub birth_food_floor: f32,
    pub birth_hunger_cost: f32,
    pub max_settlers: f32,

    pub house_pop_per_house: f32,
    pub house_min_spacing_dist: f32,
    pub house_influence_radius: f32,
    pub house_claim_radius: f32,
    pub house_farm_mult: f32,
    pub house_shelter_mult: f32,
    pub house_build_interval: f32,

    pub pioneer_density_threshold: f32,
    pub pioneer_distance: f32,

    pub interaction_chance: f32,
    pub cooperation_transfer_rate: f32,
    pub aggression_steal_rate: f32,
    pub aggression_energy_cost: f32,
    pub aggression_yield: f32,

    pub tribe_mutation_amount: f32,
    pub tribe_split_threshold: f32,
    pub tribe_split_chance: f32,

    pub social_food_weight: f32,
    pub social_crowd_weight: f32,
    pub social_cohesion_weight: f32,
    pub social_hostility_weight: f32,
    pub social_move_threshold: f32,

    pub social_scarcity_weight: f32,

    pub drought_spawn_chance: f32,
    pub drought_radius_min: f32,
    pub drought_radius_max: f32,
    pub drought_duration_min: f32,
    pub drought_duration_max: f32,
    pub drought_strength_min: f32,
    pub drought_strength_max: f32,
    pub max_droughts: f32,
    pub drought_regen_dampen: f32,

    pub initial_tribe_count: f32,

    pub cooperation_giver_cost_frac: f32,

    pub trait_reproduction_discount: f32,

    pub trait_birth_need_discount: f32,
}

pub const SIM_TUNING_FIELD_NAMES: &str = "hunger_per_sec,thirst_per_sec,cold_line,hot_line,climate_penalty_mult,rest_threshold,move_speed,move_hunger_cost,move_thirst_cost,idle_wander_interval,wander_cost_mult,food_cap_scale,food_regen_scale,food_eat_rate,maturity_age,lifespan,birth_need,birth_cooldown_interval,birth_food_floor,birth_hunger_cost,max_settlers,house_pop_per_house,house_min_spacing_dist,house_influence_radius,house_claim_radius,house_farm_mult,house_shelter_mult,house_build_interval,pioneer_density_threshold,pioneer_distance,interaction_chance,cooperation_transfer_rate,aggression_steal_rate,aggression_energy_cost,aggression_yield,tribe_mutation_amount,tribe_split_threshold,tribe_split_chance,social_food_weight,social_crowd_weight,social_cohesion_weight,social_hostility_weight,social_move_threshold,drought_spawn_chance,drought_radius_min,drought_radius_max,drought_duration_min,drought_duration_max,drought_strength_min,drought_strength_max,max_droughts,drought_regen_dampen,initial_tribe_count,cooperation_giver_cost_frac,trait_reproduction_discount,trait_birth_need_discount,social_scarcity_weight";
pub const SIM_TUNING_FIELD_COUNT: usize = 57;

impl Default for SimTuning {
    fn default() -> Self {

        Self {
            hunger_per_sec: 0.35,
            thirst_per_sec: 0.25,
            cold_line: 0.35,
            hot_line: 0.75,
            climate_penalty_mult: 2.2,

            rest_threshold: 85.0,
            move_speed: 1.0,
            move_hunger_cost: 1.0,
            move_thirst_cost: 1.3,
            idle_wander_interval: 6.0,

            wander_cost_mult: 0.12,

            food_cap_scale: 480.0,
            food_regen_scale: 8.0,
            food_eat_rate: 12.0,

            maturity_age: 120.0,
            lifespan: 3600.0,
            birth_need: 75.0,

            birth_cooldown_interval: 180.0,
            birth_food_floor: 0.3,
            birth_hunger_cost: 30.0,
            max_settlers: 60_000.0,

            house_pop_per_house: 40.0,
            house_min_spacing_dist: 0.6,
            house_influence_radius: 2.0,
            house_claim_radius: 6.0,
            house_farm_mult: 2.5,
            house_shelter_mult: 0.4,

            house_build_interval: 5.0,

            pioneer_density_threshold: 3.0,
            pioneer_distance: 15.0,

            interaction_chance: 0.45,
            cooperation_transfer_rate: 2.5,
            aggression_steal_rate: 4.0,
            aggression_energy_cost: 0.3,
            aggression_yield: 0.7,
            tribe_mutation_amount: 0.05,
            tribe_split_threshold: 0.16,
            tribe_split_chance: 0.05,
            social_food_weight: 6.0,

            social_crowd_weight: 1.5,
            social_cohesion_weight: 1.0,
            social_hostility_weight: 1.0,
            social_scarcity_weight: 8.0,
            social_move_threshold: 1.5,

            drought_spawn_chance: 0.01,
            drought_radius_min: 18.0,
            drought_radius_max: 45.0,
            drought_duration_min: 45.0,
            drought_duration_max: 120.0,
            drought_strength_min: 0.5,
            drought_strength_max: 1.0,
            max_droughts: 3.0,
            drought_regen_dampen: 0.9,
            initial_tribe_count: 8.0,

            cooperation_giver_cost_frac: 0.6,

            trait_reproduction_discount: 0.6,

            trait_birth_need_discount: 0.35,
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Settlement {
    pub id: f32,
    pub face_index: f32,
    pub faction_id: f32,
    pub population: f32,
    pub infrastructure: f32,
    pub wealth: f32,
    pub name_seed: f32,
    pub is_capital: f32,
}

impl Default for Settlement {
    fn default() -> Self {
        Self {
            id: 0.0,
            face_index: 0.0,
            faction_id: 0.0,
            population: 500.0,
            infrastructure: 0.01,
            wealth: 100.0,
            name_seed: 0.0,
            is_capital: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Settler {
    pub id: f32,
    pub face_index: f32,
    pub hunger: f32,
    pub thirst: f32,
    pub hue: f32,

    pub cooldown: f32,

    pub known_water_face: f32,
    pub known_food_face: f32,

    pub tribe_id: f32,

    pub age: f32,

    pub birth_cooldown: f32,

    pub cooperation: f32,
    pub aggression: f32,
    pub mobility: f32,

    pub render_slot: f32,

    pub previous_face: f32,
    pub move_commitment: f32,
}

impl Default for Settler {
    fn default() -> Self {
        Self {
            id: 0.0,
            face_index: 0.0,
            hunger: 70.0,
            thirst: 70.0,
            hue: 0.0,
            cooldown: 0.0,
            known_water_face: -1.0,
            known_food_face: -1.0,
            tribe_id: -1.0,
            age: 0.0,
            birth_cooldown: 0.0,
            cooperation: 0.5,
            aggression: 0.3,
            mobility: 0.5,
            render_slot: 0.0,
            previous_face: -1.0,
            move_commitment: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct SpaceRotation {
    pub speed: f32,
}

impl Default for SpaceRotation {
    fn default() -> Self {
        Self { speed: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct SpaceRotationTilt {
    pub x: f32,
    pub z: f32,
}

impl Default for SpaceRotationTilt {
    fn default() -> Self {
        Self { x: 0.0, z: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct NebulaRotation {
    pub index: f32,
    pub init_x: f32,
    pub init_y: f32,
    pub init_z: f32,
}

impl Default for NebulaRotation {
    fn default() -> Self {
        Self {
            index: 0.0,
            init_x: 0.0,
            init_y: 0.0,
            init_z: 0.0,
        }
    }
}

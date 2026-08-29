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
    pub subdivisions: f32,
    pub visualization_mode: f32,
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
            subdivisions: 6.0,
            visualization_mode: 0.0,
            version: 1.0,
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanetSimulationState {
    pub seed_value: u32,

    pub generated_version: f32,
    pub base_colors: Vec<f32>,
    pub is_water: Vec<f32>,
    pub arability: Vec<f32>,
    pub minerals: Vec<f32>,
    pub temps: Vec<f32>,
    pub moistures: Vec<f32>,
    pub elevations: Vec<f32>,
}

impl Default for PlanetSimulationState {
    fn default() -> Self {
        Self {
            seed_value: 12345,
            generated_version: -1.0,
            base_colors: Vec::new(),
            is_water: Vec::new(),
            arability: Vec::new(),
            minerals: Vec::new(),
            temps: Vec::new(),
            moistures: Vec::new(),
            elevations: Vec::new(),
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

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct AtmosphereConfig {
    pub subdivisions: f32,
    pub generated_subdivisions: f32,
    pub visible: f32,
}

impl Default for AtmosphereConfig {
    fn default() -> Self {
        Self {
            subdivisions: 6.0,
            generated_subdivisions: -1.0,
            visible: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct AtmosphereHalo {
    pub visible: f32,
}

impl Default for AtmosphereHalo {
    fn default() -> Self {
        Self { visible: 1.0 }
    }
}

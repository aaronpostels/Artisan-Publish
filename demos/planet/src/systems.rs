use artisan::ecs::Query;
use artisan::engine::component::DynamicMesh;
use artisan::engine::math::smoothstep;
use crate::components::{AtmosphereConfig, AtmosphereHalo, PlanetConfig, PlanetSimulationState};
use artisan::engine::component::Transform;
use artisan::mesh_icosphere_native;
use rayon::prelude::*;
use rayon::slice::ParallelSlice;
use rayon::iter::IntoParallelIterator;
use noise::{NoiseFn, SuperSimplex};

const BIOME_TABLE: [[u32; 4]; 4] = [
    [0xffffff, 0xe3e3e3, 0xd1d1d1, 0xc4c4c4],
    [0x8a8a8a, 0x7a8071, 0x405c3d, 0x2d4a2a],
    [0xbfb08a, 0x9ca15d, 0x5c7556, 0x2f661a],
    [0xdba258, 0xbfa854, 0x6e8c32, 0x0e360a]
];

fn lerp_color(c1: &mut [f32; 3], c2: &[f32; 3], t: f32) {
    c1[0] += (c2[0] - c1[0]) * t;
    c1[1] += (c2[1] - c1[1]) * t;
    c1[2] += (c2[2] - c1[2]) * t;
}

fn bilinear_interpolate_biome(moisture: f32, temp: f32) -> [f32; 3] {
    let cx = (moisture * 3.0).clamp(0.0, 3.0);
    let cy = (temp * 3.0).clamp(0.0, 3.0);
    let x0 = cx.floor() as usize;
    let x1 = (x0 + 1).min(3);
    let y0 = cy.floor() as usize;
    let y1 = (y0 + 1).min(3);
    let tx = cx - x0 as f32;
    let ty = cy - y0 as f32;
    let c00 = hex_to_linear_rgb(BIOME_TABLE[y0][x0]);
    let c10 = hex_to_linear_rgb(BIOME_TABLE[y0][x1]);
    let c01 = hex_to_linear_rgb(BIOME_TABLE[y1][x0]);
    let c11 = hex_to_linear_rgb(BIOME_TABLE[y1][x1]);
    let mut top = c00;
    lerp_color(&mut top, &c10, tx);
    let mut bottom = c01;
    lerp_color(&mut bottom, &c11, tx);
    lerp_color(&mut top, &bottom, ty);
    top
}

fn hex_to_linear_rgb(hex: u32) -> [f32; 3] {
    let r = ((hex >> 16) & 0xFF) as f32 / 255.0;
    let g = ((hex >> 8) & 0xFF) as f32 / 255.0;
    let b = (hex & 0xFF) as f32 / 255.0;
    let to_linear = |c: f32| {
        if c < 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    [to_linear(r), to_linear(g), to_linear(b)]
}

fn gradient(low: u32, high: u32, value: f32) -> [f32; 3] {
    let mut color = hex_to_linear_rgb(low);
    lerp_color(&mut color, &hex_to_linear_rgb(high), value.clamp(0.0, 1.0));
    color
}

fn heatmap(value: f32) -> [f32; 3] {
    let value = value.clamp(0.0, 1.0);
    if value < 0.5 {
        gradient(0x2244cc, 0xf2e85c, value * 2.0)
    } else {
        gradient(0xf2e85c, 0xd92323, (value - 0.5) * 2.0)
    }
}

pub fn visualization_color(
    mode: u32,
    elevation: f32,
    temperature: f32,
    moisture: f32,
    is_water: f32,
    arability: f32,
    minerals: f32,
    biome: [f32; 3],
) -> [f32; 3] {
    match mode {
        1 => heatmap(((elevation + 0.05) / 2.5).clamp(0.0, 1.0)),
        2 => heatmap(temperature),
        3 => gradient(0x6b3f24, 0x32b7ff, moisture),
        4 => if is_water > 0.5 { hex_to_linear_rgb(0x2389da) } else { hex_to_linear_rgb(0xd7d7d7) },
        5 => gradient(0x352016, 0x70e34b, arability),
        6 => gradient(0x17191d, 0xe8d36b, minerals),
        _ => biome,
    }
}

fn fbm(noise: &SuperSimplex, x: f32, y: f32, z: f32, octaves: usize, persistence: f32, lacunarity: f32, scale: f32) -> f32 {
    let mut total = 0.0_f64;
    let mut frequency = scale as f64;
    let mut amplitude = 1.0_f64;
    let mut max_value = 0.0_f64;
    for _ in 0..octaves {
        total += noise.get([x as f64 * frequency, y as f64 * frequency, z as f64 * frequency]) * amplitude;
        max_value += amplitude;
        amplitude *= persistence as f64;
        frequency *= lacunarity as f64;
    }
    ((total / max_value) * 1.25) as f32
}

fn ridged_fbm(noise: &SuperSimplex, x: f32, y: f32, z: f32, octaves: usize, persistence: f32, lacunarity: f32, scale: f32) -> f32 {
    let mut total = 0.0_f64;
    let mut frequency = scale as f64;
    let mut amplitude = 1.0_f64;
    let mut weight = 1.0_f64;
    let mut max_value = 0.0_f64;
    for _ in 0..octaves {
        let v = noise.get([x as f64 * frequency, y as f64 * frequency, z as f64 * frequency]);
        let mut n = 1.0_f64 - v.abs();
        n = n * n;
        n *= weight;
        weight = (n * 2.0_f64).clamp(0.1_f64, 1.0_f64);
        total += n * amplitude;
        max_value += amplitude;
        amplitude *= persistence as f64;
        frequency *= lacunarity as f64;
    }
    ((total / max_value) * 1.1) as f32
}

pub fn sys_generate_planet_mesh(
    mut q: Query<'_, (&PlanetConfig, &mut PlanetSimulationState, &mut DynamicMesh)>,
) {
    q.for_each(|(config, state, mesh)| {
        if config.version != state.generated_version {
            let subdivisions = config.subdivisions.round().clamp(0.0, 8.0) as u32;
            let (mut v, i) = mesh_icosphere_native(10.0, subdivisions, true);
            let num_faces = i.len() / 3;

            let num_verts = v.len() / 12;
            let mut elevations = vec![0.0; num_verts];
            let mut depths = vec![0.0; num_verts];
            let mut temps = vec![0.0; num_verts];
            let mut moistures = vec![0.0; num_verts];

            let seed = config.seed.to_bits();
            state.seed_value = seed;
            let noise_gen = SuperSimplex::new(seed);
            let continent_scale = config.continent_scale;
            let warp_amount = config.warp_amount;
            let polar_land = config.polar_land;
            let water_level = config.water_level;
            let base_height = config.base_height;
            let hill_height = config.hill_height;
            let mountain_density = config.mountain_density;
            let mountain_scale = config.mountain_scale;
            let mountain_height = config.mountain_height;
            let global_moisture = config.global_moisture;
            let latitude_bands = config.latitude_bands;
            let weather_warp = config.weather_warp;
            let moisture_scale = config.moisture_scale;
            let lapse_rate = config.lapse_rate;
            let visualization_mode = config.visualization_mode.round() as u32;

            let results: Vec<(f32, f32, f32, f32, f32, f32, f32)> = v.par_chunks_exact(12)
                .map(|chunk| {
                    let px = chunk[0];
                    let py = chunk[1];
                    let pz = chunk[2];
                    let len = (px * px + py * py + pz * pz).sqrt();
                    let dx = px / len;
                    let dy = py / len;
                    let dz = pz / len;
                    let qx = fbm(&noise_gen, dx, dy, dz, 2, 0.5, 2.0, continent_scale * 0.5);
                    let qy = fbm(&noise_gen, dx + 5.2, dy + 1.3, dz - 2.8, 2, 0.5, 2.0, continent_scale * 0.5);
                    let qz = fbm(&noise_gen, dx - 1.2, dy - 4.3, dz + 5.5, 2, 0.5, 2.0, continent_scale * 0.5);
                    let wx = dx + qx * warp_amount * 0.5;
                    let wy = dy + qy * warp_amount * 0.5;
                    let wz = dz + qz * warp_amount * 0.5;
                    let mut continent_noise = fbm(&noise_gen, wx, wy, wz, 4, 0.5, 2.0, continent_scale) as f32;
                    let polar_mask = smoothstep(0.85, 1.0, dy.abs());
                    let fill_noise = fbm(&noise_gen, wx * 3.0, wy * 3.0, wz * 3.0, 2, 0.5, 2.0, 3.0) as f32;
                    continent_noise += polar_mask * polar_land * (0.7 + 0.3 * fill_noise);
                    if continent_noise < 0.0 {
                        continent_noise *= 0.6;
                    }
                    let base_elevation = continent_noise - water_level;
                    let elevation;
                    let mut depth = 0.0;
                    if base_elevation <= 0.0 {
                        depth = (-base_elevation / 0.8).min(1.0);
                        elevation = fbm(&noise_gen, wx * 2.0, wy * 2.0, wz * 2.0, 2, 0.5, 2.0, 15.0) * 0.01 - 0.015;
                    } else {
                        let coast_mask = smoothstep(0.0, 0.25, base_elevation);
                        let mountain_coast_mask = smoothstep(0.1, 0.4, base_elevation);
                        let plains = base_elevation * base_height * coast_mask;
                        let hill_noise = fbm(&noise_gen, wx * 4.0, wy * 4.0, wz * 4.0, 3, 0.5, 2.0, continent_scale * 4.0);
                        let hills = hill_noise.max(0.0) * hill_height * coast_mask * 0.4;
                        let mut m_dist = fbm(&noise_gen, wx + 10.0, wy + 20.0, wz + 30.0, 3, 0.5, 2.0, continent_scale * 1.5);
                        m_dist = (m_dist + 1.0) * 0.5;
                        let threshold = 1.0 - mountain_density;
                        let mountain_mask = smoothstep(threshold - 0.2, threshold + 0.2, m_dist) * mountain_coast_mask;
                        let m_ridged = ridged_fbm(&noise_gen, wx, wy, wz, 5, 0.5, 2.0, mountain_scale);
                        let m_bulk = fbm(&noise_gen, wx, wy, wz, 3, 0.5, 2.0, mountain_scale * 0.6);
                        let mut m_shape = m_ridged * 0.6 + m_bulk * 0.4;
                        m_shape = m_shape.max(0.0).powf(1.3);
                        let mountains = m_shape * mountain_height * mountain_mask;
                        elevation = plains + hills + mountains;
                    }
                    (elevation, depth, temp_snow_cal(dy, elevation, lapse_rate, &noise_gen, dx, dz, weather_warp), moisture_cal(dx, dy, dz, &noise_gen, moisture_scale, latitude_bands, global_moisture, elevation), dx, dy, dz)
                })
                .collect();

            fn temp_snow_cal(dy: f32, elevation: f32, lapse_rate: f32, noise_gen: &SuperSimplex, dx: f32, dz: f32, weather_warp: f32) -> f32 {
                let lat = dy.abs();
                let weather_warp_noise = fbm(noise_gen, dx, dy, dz, 2, 0.5, 2.0, 0.8) * weather_warp;
                let warped_lat = (lat + weather_warp_noise * 0.1).clamp(0.0, 1.0);
                let mut temp = 1.0 - warped_lat;
                temp -= elevation * lapse_rate;
                temp.clamp(0.0, 1.0)
            }

            fn moisture_cal(dx: f32, dy: f32, dz: f32, noise_gen: &SuperSimplex, moisture_scale: f32, latitude_bands: f32, global_moisture: f32, elevation: f32) -> f32 {
                let lat = dy.abs();
                let mut moist_noise = fbm(noise_gen, dx + 50.0, dy + 50.0, dz + 50.0, 4, 0.5, 2.0, moisture_scale);
                moist_noise = (moist_noise + 1.0) * 0.5;
                let mut lat_profile = (lat * std::f32::consts::PI * 2.8).cos();
                lat_profile = lat_profile * 0.5 + 0.5;
                let mut moisture = moist_noise * (1.0 - latitude_bands) + lat_profile * latitude_bands;
                moisture *= global_moisture * 1.8;
                moisture -= elevation * 0.25;
                moisture.clamp(0.0, 1.0)
            }

            for (idx, (elevation, depth, temp, moisture, dx, dy, dz)) in results.into_iter().enumerate() {
                elevations[idx] = elevation;
                depths[idx] = depth;
                temps[idx] = temp;
                moistures[idx] = moisture;

                let offset = idx * 12;
                let final_radius = 10.0 + elevation;
                v[offset] = dx * final_radius;
                v[offset + 1] = dy * final_radius;
                v[offset + 2] = dz * final_radius;
            }

            artisan::engine::mesh::recalculate_normals_raw(&mut v, &i);

            state.base_colors.resize(num_verts * 3, 0.0);
            state.is_water.resize(num_faces, 0.0);
            state.arability.resize(num_faces, 0.0);
            state.minerals.resize(num_faces, 0.0);
            state.temps.resize(num_faces, 0.0);
            state.moistures.resize(num_faces, 0.0);
            state.elevations.resize(num_faces, 0.0);

            let color_results: Vec<([f32; 3], [f32; 3], f32, f32, f32, f32, f32)> = (0..num_faces).into_par_iter()
                .map(|f| {
                    let ix0 = f * 3;
                    let i0 = i[ix0] as usize;
                    let i1 = i[ix0 + 1] as usize;
                    let i2 = i[ix0 + 2] as usize;

                    let px0 = i0 * 12;
                    let px1 = i1 * 12;
                    let px2 = i2 * 12;

                    let cx = (v[px0] + v[px1] + v[px2]) / 3.0;
                    let cy = (v[px0 + 1] + v[px1 + 1] + v[px2 + 1]) / 3.0;
                    let cz = (v[px0 + 2] + v[px1 + 2] + v[px2 + 2]) / 3.0;

                    let mut dir = [cx, cy, cz];
                    let dir_len = (cx*cx + cy*cy + cz*cz).sqrt();
                    if dir_len > 0.0 {
                        dir[0] /= dir_len; dir[1] /= dir_len; dir[2] /= dir_len;
                    }

                    let nx = v[px0 + 3];
                    let ny = v[px0 + 4];
                    let nz = v[px0 + 5];
                    let steepness = 1.0 - (nx * dir[0] + ny * dir[1] + nz * dir[2]).max(0.0);

                    let avg_elev = (elevations[i0] + elevations[i1] + elevations[i2]) / 3.0;
                    let avg_depth = (depths[i0] + depths[i1] + depths[i2]) / 3.0;
                    let avg_temp = (temps[i0] + temps[i1] + temps[i2]) / 3.0;
                    let avg_moist = (moistures[i0] + moistures[i1] + moistures[i2]) / 3.0;

                    let is_ocean = avg_elev <= 0.0;
                    let mut target_color;
                    let arability;
                    let minerals;
                    let is_w;

                    if is_ocean {
                        is_w = 1.0;
                        arability = 0.0;
                        minerals = 0.0;
                        let base_ocean = hex_to_linear_rgb(0x0a4073);
                        let shallow_ocean = hex_to_linear_rgb(0x146c91);
                        target_color = base_ocean;
                        let border_mask = 1.0 - smoothstep(0.0, 0.25, avg_depth);
                        let mut dest = target_color;
                        lerp_color(&mut dest, &shallow_ocean, border_mask);
                        target_color = dest;
                        let n = fbm(&noise_gen, dir[0], dir[1], dir[2], 2, 0.5, 2.0, 45.0);
                        let shade = n * 0.015;
                        target_color[0] = (target_color[0] + shade).clamp(0.0, 1.0);
                        target_color[1] = (target_color[1] + shade).clamp(0.0, 1.0);
                        target_color[2] = (target_color[2] + shade).clamp(0.0, 1.0);
                    } else {
                        is_w = 0.0;
                        let temp_score = 1.0 - ((avg_temp - 0.5).abs() * 2.0);
                        let organic_potential = (temp_score * avg_moist).powf(1.5) * (1.0 - avg_elev).max(0.0);
                        arability = (organic_potential * (1.0 - smoothstep(0.1, 0.4, steepness))).clamp(0.0, 1.0);
                        let n_min = fbm(&noise_gen, dir[0], dir[1], dir[2], 3, 0.5, 2.0, 1.0);
                        minerals = (avg_elev * 0.8 + n_min.abs() * 0.5).clamp(0.0, 1.0);

                        target_color = bilinear_interpolate_biome(avg_moist, avg_temp);
                        let n = fbm(&noise_gen, dir[0], dir[1], dir[2], 2, 0.5, 2.0, 120.0);
                        let shade = n * 0.12;
                        target_color[0] = (target_color[0] + shade).clamp(0.0, 1.0);
                        target_color[1] = (target_color[1] + shade).clamp(0.0, 1.0);
                        target_color[2] = (target_color[2] + shade).clamp(0.0, 1.0);
                        let beach_mask = smoothstep(0.006, 0.0, avg_elev) * smoothstep(0.2, 0.3, avg_temp);
                        if beach_mask > 0.01 {
                            let mut dest = target_color;
                            lerp_color(&mut dest, &hex_to_linear_rgb(0xe0cd94), beach_mask);
                            target_color = dest;
                            let n_b = fbm(&noise_gen, dir[0], dir[1], dir[2], 2, 0.5, 2.0, 180.0);
                            let shade_b = n_b * 0.05;
                            target_color[0] = (target_color[0] + shade_b).clamp(0.0, 1.0);
                            target_color[1] = (target_color[1] + shade_b).clamp(0.0, 1.0);
                            target_color[2] = (target_color[2] + shade_b).clamp(0.0, 1.0);
                        }
                        let steepness_mask = smoothstep(0.12, 0.35, steepness);
                        let mountain_base_mask = smoothstep(0.2, 0.9, avg_elev);
                        let rock_mask = (steepness_mask + mountain_base_mask * 0.6).min(1.0);
                        let mut dest = target_color;
                        lerp_color(&mut dest, &hex_to_linear_rgb(0x524e4c), rock_mask);
                        target_color = dest;
                        let temp_snow = smoothstep(0.25, 0.0, avg_temp);
                        let altitude_snow = smoothstep(1.5, 3.0, avg_elev);
                        let hot_mask = smoothstep(0.8, 0.6, avg_temp);
                        let snow_coverage = temp_snow.max(altitude_snow * hot_mask);
                        let snow_stickiness = 1.0 - smoothstep(0.3, 0.7, steepness);
                        let final_snow = (snow_coverage * snow_stickiness).min(1.0);
                        let mut dest = target_color;
                        lerp_color(&mut dest, &hex_to_linear_rgb(0xffffff), final_snow);
                        target_color = dest;
                    }
                    let biome_color = target_color;
                    target_color = visualization_color(
                        visualization_mode, avg_elev, avg_temp, avg_moist,
                        is_w, arability, minerals, biome_color,
                    );

                    (target_color, biome_color, arability, minerals, is_w, avg_temp, avg_moist)
                })
                .collect();

            for (f, (target_color, biome_color, arability, minerals, is_w, avg_temp, avg_moist)) in color_results.into_iter().enumerate() {
                let ix0 = f * 3;
                let i0 = i[ix0] as usize;
                let i1 = i[ix0 + 1] as usize;
                let i2 = i[ix0 + 2] as usize;

                let px0 = i0 * 12;
                let px1 = i1 * 12;
                let px2 = i2 * 12;

                for &v_idx in &[px0, px1, px2] {
                    v[v_idx + 8] = target_color[0];
                    v[v_idx + 9] = target_color[1];
                    v[v_idx + 10] = target_color[2];
                    v[v_idx + 11] = 1.0;
                }

                state.base_colors[i0 * 3] = biome_color[0];
                state.base_colors[i0 * 3 + 1] = biome_color[1];
                state.base_colors[i0 * 3 + 2] = biome_color[2];
                state.base_colors[i1 * 3] = biome_color[0];
                state.base_colors[i1 * 3 + 1] = biome_color[1];
                state.base_colors[i1 * 3 + 2] = biome_color[2];
                state.base_colors[i2 * 3] = biome_color[0];
                state.base_colors[i2 * 3 + 1] = biome_color[1];
                state.base_colors[i2 * 3 + 2] = biome_color[2];

                state.is_water[f] = is_w;
                state.arability[f] = arability;
                state.minerals[f] = minerals;
                state.temps[f] = avg_temp;
                state.moistures[f] = avg_moist;
                state.elevations[f] = (elevations[i0] + elevations[i1] + elevations[i2]) / 3.0;
            }

            mesh.vertices = v;
            mesh.indices = i;
            mesh.version = mesh.version.wrapping_add(1);
            state.generated_version = config.version;
        }
    });
}

pub fn sys_generate_atmosphere_mesh(
    mut q: Query<'_, (&mut AtmosphereConfig, &mut DynamicMesh, &mut Transform)>,
) {
    q.for_each(|(config, mesh, transform)| {
        let scale = if config.visible > 0.5 { 1.0 } else { 0.0 };
        transform.scale = [scale, scale, scale];
        let subdivisions = config.subdivisions.round().clamp(0.0, 6.0);
        if subdivisions == config.generated_subdivisions {
            return;
        }

        let (vertices, indices) = mesh_icosphere_native(10.05, subdivisions as u32, true);
        mesh.vertices = vertices;
        mesh.indices = indices;
        mesh.version = mesh.version.wrapping_add(1);
        config.generated_subdivisions = subdivisions;
    });
}

pub fn sys_update_atmosphere_halo(
    mut q: Query<'_, (&AtmosphereHalo, &mut Transform)>,
) {
    q.for_each(|(halo, transform)| {
        let scale = if halo.visible > 0.5 { 1.0 } else { 0.0 };
        transform.scale = [scale, scale, scale];
    });
}

use crate::components::{
    DroughtEvent, PlanetConfig, PlanetSimulationState, Settlement, Settler, SimTuning,
};
use artisan::ecs::{Commands, Entity, Query, Res};
use artisan::engine::Time;
use artisan::engine::component::{
    Billboard, DynamicMesh, GPUInstanceTransform, GlobalTransform, MeshHandle, StandardMaterial,
    Transform,
};
use artisan::engine::math::{SeededRng, smoothstep};
use artisan::mesh_icosphere_native;
use noise::{NoiseFn, SuperSimplex};
use rayon::iter::IntoParallelIterator;
use rayon::prelude::*;
use rayon::slice::ParallelSlice;

const BIOME_TABLE: [[u32; 4]; 4] = [
    [0xffffff, 0xe3e3e3, 0xd1d1d1, 0xc4c4c4],
    [0x8a8a8a, 0x7a8071, 0x405c3d, 0x2d4a2a],
    [0xbfb08a, 0x9ca15d, 0x5c7556, 0x2f661a],
    [0xdba258, 0xbfa854, 0x6e8c32, 0x0e360a],
];

fn lerp_color(c1: &mut [f32; 3], c2: &[f32; 3], t: f32) {
    c1[0] += (c2[0] - c1[0]) * t;
    c1[1] += (c2[1] - c1[1]) * t;
    c1[2] += (c2[2] - c1[2]) * t;
}

pub(crate) fn bilinear_interpolate_biome(moisture: f32, temp: f32) -> [f32; 3] {
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

fn fbm(
    noise: &SuperSimplex,
    x: f32,
    y: f32,
    z: f32,
    octaves: usize,
    persistence: f32,
    lacunarity: f32,
    scale: f32,
) -> f32 {
    let mut total = 0.0_f64;
    let mut frequency = scale as f64;
    let mut amplitude = 1.0_f64;
    let mut max_value = 0.0_f64;
    for _ in 0..octaves {
        total += noise.get([
            x as f64 * frequency,
            y as f64 * frequency,
            z as f64 * frequency,
        ]) * amplitude;
        max_value += amplitude;
        amplitude *= persistence as f64;
        frequency *= lacunarity as f64;
    }
    ((total / max_value) * 1.25) as f32
}

fn ridged_fbm(
    noise: &SuperSimplex,
    x: f32,
    y: f32,
    z: f32,
    octaves: usize,
    persistence: f32,
    lacunarity: f32,
    scale: f32,
) -> f32 {
    let mut total = 0.0_f64;
    let mut frequency = scale as f64;
    let mut amplitude = 1.0_f64;
    let mut weight = 1.0_f64;
    let mut max_value = 0.0_f64;
    for _ in 0..octaves {
        let v = noise.get([
            x as f64 * frequency,
            y as f64 * frequency,
            z as f64 * frequency,
        ]);
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
    mut q: Query<
        '_,
        (
            &PlanetConfig,
            &mut PlanetSimulationState,
            &mut DynamicMesh,
            &SimTuning,
        ),
    >,
    mut q_settlements: Query<'_, (Entity, &Settlement)>,
    mut commands: Commands,
) {
    q.for_each(|(config, state, mesh, tuning)| {
        if config.version != mesh.version as f32 {
            q_settlements.for_each(|(ent, _)| {
                commands.despawn(ent);
            });
            state.faction_colors.clear();
            state.faction_tech.clear();
            state.face_owner.clear();
            state.face_score.clear();

            let (v_indexed, i_indexed) = mesh_icosphere_native(10.0, 6, false);
            let num_faces = i_indexed.len() / 3;
            let mut v = Vec::with_capacity(num_faces * 3 * 12);
            let mut i = Vec::with_capacity(num_faces * 3);
            let mut vertex_sources = Vec::with_capacity(num_faces * 3);

            for f in 0..num_faces {
                let idx0 = i_indexed[f * 3] as usize * 12;
                let idx1 = i_indexed[f * 3 + 1] as usize * 12;
                let idx2 = i_indexed[f * 3 + 2] as usize * 12;

                v.extend_from_slice(&v_indexed[idx0..idx0 + 12]);
                v.extend_from_slice(&v_indexed[idx1..idx1 + 12]);
                v.extend_from_slice(&v_indexed[idx2..idx2 + 12]);

                let start_idx = (f * 3) as u32;
                i.push(start_idx);
                i.push(start_idx + 1);
                i.push(start_idx + 2);
                vertex_sources.push(i_indexed[f * 3]);
                vertex_sources.push(i_indexed[f * 3 + 1]);
                vertex_sources.push(i_indexed[f * 3 + 2]);
            }
            state.vertex_sources = vertex_sources;

            let num_verts = v.len() / 12;
            let mut elevations = vec![0.0; num_verts];
            let mut depths = vec![0.0; num_verts];
            let mut temps = vec![0.0; num_verts];
            let mut moistures = vec![0.0; num_verts];
            state.face_owner.resize(num_faces, -1);
            state.face_score.resize(num_faces, 0.0);

            if state.neighbors_offsets.is_empty() {
                let neighbors = artisan::engine::mesh::build_face_adjacency_native(&i_indexed);
                let mut flat = Vec::new();
                let mut offsets = Vec::new();
                let mut current_offset = 0;
                offsets.push(0);
                for list in neighbors {
                    flat.extend_from_slice(&list);
                    current_offset += list.len() as u32;
                    offsets.push(current_offset);
                }
                state.neighbors_flat = flat;
                state.neighbors_offsets = offsets;
            }

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

            let results: Vec<(f32, f32, f32, f32, f32, f32, f32)> = v
                .par_chunks_exact(12)
                .map(|chunk| {
                    let px = chunk[0];
                    let py = chunk[1];
                    let pz = chunk[2];
                    let len = (px * px + py * py + pz * pz).sqrt();
                    let dx = px / len;
                    let dy = py / len;
                    let dz = pz / len;
                    let qx = fbm(&noise_gen, dx, dy, dz, 2, 0.5, 2.0, continent_scale * 0.5);
                    let qy = fbm(
                        &noise_gen,
                        dx + 5.2,
                        dy + 1.3,
                        dz - 2.8,
                        2,
                        0.5,
                        2.0,
                        continent_scale * 0.5,
                    );
                    let qz = fbm(
                        &noise_gen,
                        dx - 1.2,
                        dy - 4.3,
                        dz + 5.5,
                        2,
                        0.5,
                        2.0,
                        continent_scale * 0.5,
                    );
                    let wx = dx + qx * warp_amount * 0.5;
                    let wy = dy + qy * warp_amount * 0.5;
                    let wz = dz + qz * warp_amount * 0.5;
                    let mut continent_noise =
                        fbm(&noise_gen, wx, wy, wz, 4, 0.5, 2.0, continent_scale) as f32;
                    let polar_mask = smoothstep(0.85, 1.0, dy.abs());
                    let fill_noise =
                        fbm(&noise_gen, wx * 3.0, wy * 3.0, wz * 3.0, 2, 0.5, 2.0, 3.0) as f32;
                    continent_noise += polar_mask * polar_land * (0.7 + 0.3 * fill_noise);
                    if continent_noise < 0.0 {
                        continent_noise *= 0.6;
                    }
                    let base_elevation = continent_noise - water_level;
                    let elevation;
                    let mut depth = 0.0;
                    if base_elevation <= 0.0 {
                        depth = (-base_elevation / 0.8).min(1.0);
                        elevation =
                            fbm(&noise_gen, wx * 2.0, wy * 2.0, wz * 2.0, 2, 0.5, 2.0, 15.0) * 0.01
                                - 0.015;
                    } else {
                        let coast_mask = smoothstep(0.0, 0.25, base_elevation);
                        let mountain_coast_mask = smoothstep(0.1, 0.4, base_elevation);
                        let plains = base_elevation * base_height * coast_mask;
                        let hill_noise = fbm(
                            &noise_gen,
                            wx * 4.0,
                            wy * 4.0,
                            wz * 4.0,
                            3,
                            0.5,
                            2.0,
                            continent_scale * 4.0,
                        );
                        let hills = hill_noise.max(0.0) * hill_height * coast_mask * 0.4;
                        let mut m_dist = fbm(
                            &noise_gen,
                            wx + 10.0,
                            wy + 20.0,
                            wz + 30.0,
                            3,
                            0.5,
                            2.0,
                            continent_scale * 1.5,
                        );
                        m_dist = (m_dist + 1.0) * 0.5;
                        let threshold = 1.0 - mountain_density;
                        let mountain_mask = smoothstep(threshold - 0.2, threshold + 0.2, m_dist)
                            * mountain_coast_mask;
                        let m_ridged =
                            ridged_fbm(&noise_gen, wx, wy, wz, 5, 0.5, 2.0, mountain_scale);
                        let m_bulk = fbm(&noise_gen, wx, wy, wz, 3, 0.5, 2.0, mountain_scale * 0.6);
                        let mut m_shape = m_ridged * 0.6 + m_bulk * 0.4;
                        m_shape = m_shape.max(0.0).powf(1.3);
                        let mountains = m_shape * mountain_height * mountain_mask;
                        elevation = plains + hills + mountains;
                    }
                    (
                        elevation,
                        depth,
                        temp_snow_cal(dy, elevation, lapse_rate, &noise_gen, dx, dz, weather_warp),
                        moisture_cal(
                            dx,
                            dy,
                            dz,
                            &noise_gen,
                            moisture_scale,
                            latitude_bands,
                            global_moisture,
                            elevation,
                        ),
                        dx,
                        dy,
                        dz,
                    )
                })
                .collect();

            fn temp_snow_cal(
                dy: f32,
                elevation: f32,
                lapse_rate: f32,
                noise_gen: &SuperSimplex,
                dx: f32,
                dz: f32,
                weather_warp: f32,
            ) -> f32 {
                let lat = dy.abs();
                let weather_warp_noise =
                    fbm(noise_gen, dx, dy, dz, 2, 0.5, 2.0, 0.8) * weather_warp;
                let warped_lat = (lat + weather_warp_noise * 0.1).clamp(0.0, 1.0);
                let mut temp = 1.0 - warped_lat;
                temp -= elevation * lapse_rate;
                temp.clamp(0.0, 1.0)
            }

            fn moisture_cal(
                dx: f32,
                dy: f32,
                dz: f32,
                noise_gen: &SuperSimplex,
                moisture_scale: f32,
                latitude_bands: f32,
                global_moisture: f32,
                elevation: f32,
            ) -> f32 {
                let lat = dy.abs();
                let mut moist_noise = fbm(
                    noise_gen,
                    dx + 50.0,
                    dy + 50.0,
                    dz + 50.0,
                    4,
                    0.5,
                    2.0,
                    moisture_scale,
                );
                moist_noise = (moist_noise + 1.0) * 0.5;
                let mut lat_profile = (lat * std::f32::consts::PI * 2.8).cos();
                lat_profile = lat_profile * 0.5 + 0.5;
                let mut moisture =
                    moist_noise * (1.0 - latitude_bands) + lat_profile * latitude_bands;
                moisture *= global_moisture * 1.8;
                moisture -= elevation * 0.25;
                moisture.clamp(0.0, 1.0)
            }

            for (idx, (elevation, depth, temp, moisture, dx, dy, dz)) in
                results.into_iter().enumerate()
            {
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
            state.food_cap.resize(num_faces, 0.0);
            state.inv_food_cap.resize(num_faces, 0.0);
            state.food_regen.resize(num_faces, 0.0);
            state.food_stock.resize(num_faces, 0.0);
            state.house_face.resize(num_faces, 0);
            state.house_colony_of_face.resize(num_faces, -1);
            state.house_face_list.clear();
            state.has_house_buff.resize(num_faces, 0);
            state.face_colony.resize(num_faces, -1);
            state.colony_population.resize(num_faces, 0);
            state.colony_houses.resize(num_faces, 0);
            state.colony_best_face.resize(num_faces, -1);
            state.colony_territory_faces.resize(num_faces, 0);
            state.drought.resize(num_faces, 0.0);
            state.admin_drought.resize(num_faces, 0.0);
            state.face_population.resize(num_faces, 0);
            state.face_dominant_tribe.resize(num_faces, -1);

            let color_results: Vec<([f32; 3], f32, f32, f32, f32, f32, [f32; 3])> = (0..num_faces)
                .into_par_iter()
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
                    let dir_len = (cx * cx + cy * cy + cz * cz).sqrt();
                    if dir_len > 0.0 {
                        dir[0] /= dir_len;
                        dir[1] /= dir_len;
                        dir[2] /= dir_len;
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
                        let organic_potential =
                            (temp_score * avg_moist).powf(1.5) * (1.0 - avg_elev).max(0.0);
                        arability = (organic_potential * (1.0 - smoothstep(0.1, 0.4, steepness)))
                            .clamp(0.0, 1.0);
                        let n_min = fbm(&noise_gen, dir[0], dir[1], dir[2], 3, 0.5, 2.0, 1.0);
                        minerals = (avg_elev * 0.8 + n_min.abs() * 0.5).clamp(0.0, 1.0);

                        target_color = bilinear_interpolate_biome(avg_moist, avg_temp);
                        let n = fbm(&noise_gen, dir[0], dir[1], dir[2], 2, 0.5, 2.0, 120.0);
                        let shade = n * 0.12;
                        target_color[0] = (target_color[0] + shade).clamp(0.0, 1.0);
                        target_color[1] = (target_color[1] + shade).clamp(0.0, 1.0);
                        target_color[2] = (target_color[2] + shade).clamp(0.0, 1.0);
                        let beach_mask =
                            smoothstep(0.006, 0.0, avg_elev) * smoothstep(0.2, 0.3, avg_temp);
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
                    (
                        target_color,
                        arability,
                        minerals,
                        is_w,
                        avg_temp,
                        avg_moist,
                        [cx, cy, cz],
                    )
                })
                .collect();

            state.face_centers.resize(num_faces * 3, 0.0);
            for (f, (target_color, arability, minerals, is_w, avg_temp, avg_moist, center)) in
                color_results.into_iter().enumerate()
            {
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

                state.base_colors[i0 * 3] = target_color[0];
                state.base_colors[i0 * 3 + 1] = target_color[1];
                state.base_colors[i0 * 3 + 2] = target_color[2];
                state.base_colors[i1 * 3] = target_color[0];
                state.base_colors[i1 * 3 + 1] = target_color[1];
                state.base_colors[i1 * 3 + 2] = target_color[2];
                state.base_colors[i2 * 3] = target_color[0];
                state.base_colors[i2 * 3 + 1] = target_color[1];
                state.base_colors[i2 * 3 + 2] = target_color[2];

                state.is_water[f] = is_w;
                state.arability[f] = arability;
                state.minerals[f] = minerals;
                state.temps[f] = avg_temp;
                state.moistures[f] = avg_moist;
                state.elevations[f] = (elevations[i0] + elevations[i1] + elevations[i2]) / 3.0;
                state.face_centers[f * 3] = center[0];
                state.face_centers[f * 3 + 1] = center[1];
                state.face_centers[f * 3 + 2] = center[2];
                let cap = arability * tuning.food_cap_scale;
                state.food_cap[f] = cap;
                state.inv_food_cap[f] = if cap > 0.0 { 1.0 / cap } else { 0.0 };
                state.food_regen[f] = arability * tuning.food_regen_scale;
                state.food_stock[f] = cap;
            }

            state.dist_to_water = compute_distance_to_water(
                num_faces,
                &state.is_water,
                &state.neighbors_offsets,
                &state.neighbors_flat,
            );

            state.has_adjacent_water.clear();
            state.has_adjacent_water.resize(num_faces, 0);
            for f in 0..num_faces {
                let adjacent = if state.is_water[f] > 0.5 {
                    true
                } else if f + 1 < state.neighbors_offsets.len() {
                    let s = state.neighbors_offsets[f] as usize;
                    let e =
                        (state.neighbors_offsets[f + 1] as usize).min(state.neighbors_flat.len());
                    (s..e).any(|idx| {
                        let n = state.neighbors_flat[idx] as usize;
                        n < num_faces && state.is_water[n] > 0.5
                    })
                } else {
                    false
                };
                state.has_adjacent_water[f] = adjacent as u8;
            }

            mesh.vertices = v;
            mesh.indices = i;
            mesh.version = config.version as u32;
        }
    });
}

#[allow(dead_code)]
pub fn sys_run_planet_simulation(
    mut q_planet: Query<'_, (&PlanetConfig, &mut PlanetSimulationState, &mut DynamicMesh)>,
    mut q_settlements: Query<
        '_,
        (
            Entity,
            &mut Settlement,
            &mut Transform,
            &mut StandardMaterial,
        ),
    >,
    mut commands: Commands,
) {
    q_planet.for_each(|(_config, state, mesh)| {
        if state.run_simulation > 0.0 {
            let mut rng = SeededRng::new(state.seed_value);
            let num_faces = mesh.indices.len() / 3;

            if state.faction_colors.is_empty() && num_faces > 0 && !mesh.vertices.is_empty() {
                let colonies_count = state.num_colonies as usize;
                let mut valid_faces = Vec::new();
                for f in 0..num_faces {
                    if state.is_water[f] == 0.0 && state.arability[f] > 0.5 {
                        valid_faces.push(f);
                    }
                }
                if valid_faces.is_empty() {
                    for f in 0..num_faces {
                        if state.is_water[f] == 0.0 {
                            valid_faces.push(f);
                        }
                    }
                }

                for idx in 0..colonies_count {
                    let r = (rng.next_f32() * 140.0 + 115.0) as u32;
                    let g = (rng.next_f32() * 140.0 + 115.0) as u32;
                    let b = (rng.next_f32() * 140.0 + 115.0) as u32;
                    let color = (r << 16) | (g << 8) | b;
                    state.faction_colors.push(color);
                    state.faction_tech.push(0.001);
                    if !valid_faces.is_empty() {
                        let pick = (rng.next_f32() * valid_faces.len() as f32) as usize;
                        let face_id = valid_faces[pick];
                        valid_faces.remove(pick);
                        let i0 = mesh.indices[face_id * 3] as usize * 12;
                        let i1 = mesh.indices[face_id * 3 + 1] as usize * 12;
                        let i2 = mesh.indices[face_id * 3 + 2] as usize * 12;
                        let cx = (mesh.vertices[i0] + mesh.vertices[i1] + mesh.vertices[i2]) / 3.0;
                        let cy =
                            (mesh.vertices[i0 + 1] + mesh.vertices[i1 + 1] + mesh.vertices[i2 + 1])
                                / 3.0;
                        let cz =
                            (mesh.vertices[i0 + 2] + mesh.vertices[i1 + 2] + mesh.vertices[i2 + 2])
                                / 3.0;

                        let mut trans = Transform {
                            translation: [cx, cy, cz],
                            rotation: [0.0, 0.0, 0.0, 1.0],
                            scale: [0.15, 0.3, 0.15],
                        };
                        let norm_len = (cx * cx + cy * cy + cz * cz).sqrt();
                        if norm_len > 1e-6_f32 {
                            let dx = cx / norm_len;
                            let dy = cy / norm_len;
                            let dz = cz / norm_len;
                            let qw = 1.0_f32 + dy;
                            let (qx, qy, qz): (f32, f32, f32) = if qw < 1e-6_f32 {
                                (1.0_f32, 0.0_f32, 0.0_f32)
                            } else {
                                (dz, 0.0_f32, -dx)
                            };
                            let q_len = (qx * qx + qy * qy + qz * qz + qw * qw).sqrt();
                            trans.rotation = [qx / q_len, qy / q_len, qz / q_len, qw / q_len];
                        }

                        let faction_color_arr = hex_to_linear_rgb(color);
                        commands
                            .spawn()
                            .insert(trans)
                            .insert(GlobalTransform::default())
                            .insert(Settlement {
                                id: idx as f32,
                                face_index: face_id as f32,
                                faction_id: idx as f32,
                                population: 500.0,
                                infrastructure: 0.01,
                                wealth: 100.0,
                                name_seed: rng.next_f32() * 100000.0,
                                is_capital: 1.0,
                            })
                            .insert(StandardMaterial {
                                base_color: [
                                    faction_color_arr[0],
                                    faction_color_arr[1],
                                    faction_color_arr[2],
                                    1.0,
                                ],
                                roughness: 0.5,
                                metallic: 0.5,
                                ..Default::default()
                            })
                            .insert(MeshHandle { id: 1.0 });
                    }
                }
            }

            state.year_value += 1;

            let num_factions = state.faction_colors.len();
            let mut faction_pops = vec![0.0_f32; num_factions];
            let mut faction_nodes = vec![0.0_f32; num_factions];
            let mut faction_infra = vec![0.0_f32; num_factions];
            let mut regional_load = vec![0.0_f32; num_faces];
            let mut regional_nodes = vec![0; num_faces];

            q_settlements.for_each(|(_ent, set, _trans, _mat)| {
                let f_id = set.faction_id as usize;
                if f_id < num_factions {
                    faction_pops[f_id] += set.population;
                    faction_nodes[f_id] += 1.0;
                    faction_infra[f_id] += set.infrastructure;
                }
                let face_id = set.face_index as usize;
                if face_id < num_faces {
                    regional_load[face_id] += set.population;
                    regional_nodes[face_id] += 1;
                    let start = state.neighbors_offsets[face_id] as usize;
                    let end = state.neighbors_offsets[face_id + 1] as usize;
                    for n_idx in start..end {
                        let nb = state.neighbors_flat[n_idx] as usize;
                        if nb < num_faces {
                            regional_load[nb] += set.population * 0.35;
                        }
                    }
                }
            });

            let mut base_id_counter = q_settlements.iter().count() + (rng.seed % 50000) as usize;

            struct NewSettlement {
                face_index: usize,
                faction_id: usize,
                population: f32,
                id: usize,
            }
            let mut new_settlements = Vec::new();

            q_settlements.for_each(|(ent, set, transform, mat)| {
                let s_face_index = set.face_index as usize;
                let s_faction_id = set.faction_id as usize;
                if s_face_index >= num_faces || s_faction_id >= num_factions {
                    return;
                }

                let tech = state.faction_tech[s_faction_id];
                let arab = state.arability[s_face_index];
                let min = state.minerals[s_face_index];

                let tech_efficiency = 1.0 + tech.powf(1.6) * 8.0;
                let infra_mult = 1.0 + (set.infrastructure * 4.0).sqrt();
                let base_cap = (arab * 850_000.0 * tech_efficiency * infra_mult).max(50.0);

                let load = regional_load[s_face_index];
                let mut effective_capacity = (base_cap - load * 0.75).max(50.0);
                if set.is_capital > 0.0 {
                    effective_capacity *= 1.4;
                }

                let mut pop = set.population;
                let growth_rate = (0.0006_f32 + (tech * 0.004_f32)).clamp(0.0003_f32, 0.015_f32);

                if pop <= effective_capacity {
                    pop += growth_rate * pop * (1.0 - (pop / effective_capacity));
                } else {
                    pop -= 0.006 * pop * (1.0 - (effective_capacity / pop).max(0.0));
                }

                let production = pop * min * (1.0 + tech * 2.0) * 0.0015;
                let mut wealth = set.wealth + production;
                let infra_cost = 600.0 * (1.0 + set.infrastructure * 25.0) / (1.0 + tech * 8.0);
                let mut infra = set.infrastructure;
                if wealth > infra_cost && pop > effective_capacity * 0.5 && infra < 1.0 {
                    infra = (infra + 0.012).min(1.0);
                    wealth -= infra_cost;
                }

                let nodes = faction_nodes[s_faction_id];
                let avg_pop = faction_pops[s_faction_id] / nodes.max(1.0);
                let local_density = regional_nodes[s_face_index];
                let too_dense = local_density >= 3;

                let can_spawn = pop > 1200.0 && !too_dense && nodes < 12000.0;
                let mut spawn_chance = if can_spawn {
                    let mut chance = 0.0015 * (1.0 + tech * 5.0);
                    if nodes > 10.0 && avg_pop < (800.0 + tech * 12000.0) {
                        chance *= 0.1;
                    }
                    chance
                } else {
                    0.0
                };

                if pop > effective_capacity * 0.85 {
                    spawn_chance *= 2.2;
                }

                if wealth > 200.0 && rng.next_f32() < spawn_chance {
                    let mut target_dir = [
                        rng.next_f32() - 0.5,
                        rng.next_f32() - 0.5,
                        rng.next_f32() - 0.5,
                    ];
                    let dir_len = (target_dir[0] * target_dir[0]
                        + target_dir[1] * target_dir[1]
                        + target_dir[2] * target_dir[2])
                        .sqrt();
                    if dir_len > 0.0 {
                        target_dir[0] /= dir_len;
                        target_dir[1] /= dir_len;
                        target_dir[2] /= dir_len;
                    }

                    let is_expansion = rng.next_f32() < (0.15 + tech * 0.35);
                    let spread = if is_expansion {
                        0.25 + rng.next_f32() * (0.8 + tech * 4.0)
                    } else {
                        0.04 + rng.next_f32() * 0.18
                    };

                    let s_pos = transform.translation;
                    let jump_pos = [
                        s_pos[0] + target_dir[0] * spread,
                        s_pos[1] + target_dir[1] * spread,
                        s_pos[2] + target_dir[2] * spread,
                    ];

                    let mut best_face = 0;
                    let mut min_dist = f32::MAX;
                    for f in 0..num_faces {
                        let cx = (mesh.vertices[mesh.indices[f * 3] as usize * 12]
                            + mesh.vertices[mesh.indices[f * 3 + 1] as usize * 12]
                            + mesh.vertices[mesh.indices[f * 3 + 2] as usize * 12])
                            / 3.0;
                        let cy = (mesh.vertices[mesh.indices[f * 3] as usize * 12 + 1]
                            + mesh.vertices[mesh.indices[f * 3 + 1] as usize * 12 + 1]
                            + mesh.vertices[mesh.indices[f * 3 + 2] as usize * 12 + 1])
                            / 3.0;
                        let cz = (mesh.vertices[mesh.indices[f * 3] as usize * 12 + 2]
                            + mesh.vertices[mesh.indices[f * 3 + 1] as usize * 12 + 2]
                            + mesh.vertices[mesh.indices[f * 3 + 2] as usize * 12 + 2])
                            / 3.0;
                        let dx = cx - jump_pos[0];
                        let dy = cy - jump_pos[1];
                        let dz = cz - jump_pos[2];
                        let d = dx * dx + dy * dy + dz * dz;
                        if d < min_dist {
                            min_dist = d;
                            best_face = f;
                        }
                    }

                    if state.is_water[best_face] == 0.0 && state.arability[best_face] > 0.05 {
                        let migrants = (pop * 0.06).min(4500.0).max(100.0);
                        pop -= migrants;
                        wealth -= 150.0;
                        new_settlements.push(NewSettlement {
                            face_index: best_face,
                            faction_id: s_faction_id,
                            population: migrants,
                            id: base_id_counter,
                        });
                        base_id_counter += 1;
                    }
                }

                set.population = pop;
                set.infrastructure = infra;
                set.wealth = wealth;

                if set.population < 40.0 && set.is_capital == 0.0 {
                    commands.despawn(ent);
                } else {
                    let i0 = mesh.indices[s_face_index * 3] as usize * 12;
                    let i1 = mesh.indices[s_face_index * 3 + 1] as usize * 12;
                    let i2 = mesh.indices[s_face_index * 3 + 2] as usize * 12;
                    let cx = (mesh.vertices[i0] + mesh.vertices[i1] + mesh.vertices[i2]) / 3.0;
                    let cy =
                        (mesh.vertices[i0 + 1] + mesh.vertices[i1 + 1] + mesh.vertices[i2 + 1])
                            / 3.0;
                    let cz =
                        (mesh.vertices[i0 + 2] + mesh.vertices[i1 + 2] + mesh.vertices[i2 + 2])
                            / 3.0;

                    transform.translation = [cx, cy, cz];
                    let norm_len = (cx * cx + cy * cy + cz * cz).sqrt();
                    if norm_len > 1e-6_f32 {
                        let dx = cx / norm_len;
                        let dy = cy / norm_len;
                        let dz = cz / norm_len;
                        let qw = 1.0_f32 + dy;
                        let (qx, qy, qz): (f32, f32, f32) = if qw < 1e-6_f32 {
                            (1.0_f32, 0.0_f32, 0.0_f32)
                        } else {
                            (dz, 0.0_f32, -dx)
                        };
                        let q_len = (qx * qx + qy * qy + qz * qz + qw * qw).sqrt();
                        transform.rotation = [qx / q_len, qy / q_len, qz / q_len, qw / q_len];
                    }

                    let factor = 0.15 + (set.population.log10() * 0.1);
                    transform.scale = [factor, factor * 2.0, factor];

                    let col_hex = state.faction_colors[s_faction_id];
                    let rgb = hex_to_linear_rgb(col_hex);
                    mat.base_color = [rgb[0], rgb[1], rgb[2], 1.0];
                }
            });

            for ns in new_settlements {
                let i0 = mesh.indices[ns.face_index * 3] as usize * 12;
                let i1 = mesh.indices[ns.face_index * 3 + 1] as usize * 12;
                let i2 = mesh.indices[ns.face_index * 3 + 2] as usize * 12;
                let cx = (mesh.vertices[i0] + mesh.vertices[i1] + mesh.vertices[i2]) / 3.0;
                let cy =
                    (mesh.vertices[i0 + 1] + mesh.vertices[i1 + 1] + mesh.vertices[i2 + 1]) / 3.0;
                let cz =
                    (mesh.vertices[i0 + 2] + mesh.vertices[i1 + 2] + mesh.vertices[i2 + 2]) / 3.0;

                let mut trans = Transform {
                    translation: [cx, cy, cz],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [0.15, 0.3, 0.15],
                };
                let norm_len = (cx * cx + cy * cy + cz * cz).sqrt();
                if norm_len > 1e-6_f32 {
                    let dx = cx / norm_len;
                    let dy = cy / norm_len;
                    let dz = cz / norm_len;
                    let qw = 1.0_f32 + dy;
                    let (qx, qy, qz): (f32, f32, f32) = if qw < 1e-6_f32 {
                        (1.0_f32, 0.0_f32, 0.0_f32)
                    } else {
                        (dz, 0.0_f32, -dx)
                    };
                    let q_len = (qx * qx + qy * qy + qz * qz + qw * qw).sqrt();
                    trans.rotation = [qx / q_len, qy / q_len, qz / q_len, qw / q_len];
                }

                let rgb = hex_to_linear_rgb(state.faction_colors[ns.faction_id]);
                commands
                    .spawn()
                    .insert(trans)
                    .insert(GlobalTransform::default())
                    .insert(Settlement {
                        id: ns.id as f32,
                        face_index: ns.face_index as f32,
                        faction_id: ns.faction_id as f32,
                        population: ns.population,
                        infrastructure: 0.0,
                        wealth: 20.0,
                        name_seed: rng.next_f32() * 100000.0,
                        is_capital: 0.0,
                    })
                    .insert(StandardMaterial {
                        base_color: [rgb[0], rgb[1], rgb[2], 1.0],
                        roughness: 0.5,
                        metallic: 0.5,
                        ..Default::default()
                    })
                    .insert(MeshHandle { id: 1.0 });
            }

            for i in 0..num_factions {
                if faction_pops[i] > 0.0 {
                    let n_count = faction_nodes[i].max(1.0);
                    let avg_inf = faction_infra[i] / n_count;
                    let cur_t = state.faction_tech[i];
                    let tech_gain = (faction_pops[i].powf(0.52) * 0.000_000_05 * (1.0 + avg_inf))
                        / (1.0 + cur_t * 15.0);
                    state.faction_tech[i] = (cur_t + tech_gain).min(2.5);
                }
            }

            for f in 0..num_faces {
                state.face_owner[f] = -1;
                state.face_score[f] = 0.0;
            }

            let num_factions_now = state.faction_colors.len();
            q_settlements.for_each(|(_ent, set, transform, _mat)| {
                if set.population < 150.0 {
                    return;
                }

                if set.faction_id as usize >= num_factions_now {
                    return;
                }
                let px = transform.translation[0];
                let py = transform.translation[1];
                let pz = transform.translation[2];
                let max_dist = (set.population.powf(0.32) * 0.18).clamp(0.08, 1.2);
                let max_dist_sq = max_dist * max_dist;

                for f in 0..num_faces {
                    if state.is_water[f] > 0.5 {
                        continue;
                    }
                    let i0 = mesh.indices[f * 3] as usize * 12;
                    let i1 = mesh.indices[f * 3 + 1] as usize * 12;
                    let i2 = mesh.indices[f * 3 + 2] as usize * 12;
                    let cx = (mesh.vertices[i0] + mesh.vertices[i1] + mesh.vertices[i2]) / 3.0;
                    let cy =
                        (mesh.vertices[i0 + 1] + mesh.vertices[i1 + 1] + mesh.vertices[i2 + 1])
                            / 3.0;
                    let cz =
                        (mesh.vertices[i0 + 2] + mesh.vertices[i1 + 2] + mesh.vertices[i2 + 2])
                            / 3.0;

                    let dist_sq =
                        (px - cx) * (px - cx) + (py - cy) * (py - cy) + (pz - cz) * (pz - cz);
                    if dist_sq < max_dist_sq {
                        let score = set.population / (dist_sq + 0.015);
                        if score > state.face_score[f] {
                            state.face_score[f] = score;
                            state.face_owner[f] = set.faction_id as i32;
                        }
                    }
                }
            });

            for f in 0..num_faces {
                let i0 = mesh.indices[f * 3] as usize;
                let i1 = mesh.indices[f * 3 + 1] as usize;
                let i2 = mesh.indices[f * 3 + 2] as usize;

                let owner = state.face_owner[f];
                let mut target_colors = [
                    [
                        state.base_colors[i0 * 3],
                        state.base_colors[i0 * 3 + 1],
                        state.base_colors[i0 * 3 + 2],
                    ],
                    [
                        state.base_colors[i1 * 3],
                        state.base_colors[i1 * 3 + 1],
                        state.base_colors[i1 * 3 + 2],
                    ],
                    [
                        state.base_colors[i2 * 3],
                        state.base_colors[i2 * 3 + 1],
                        state.base_colors[i2 * 3 + 2],
                    ],
                ];

                if owner != -1 && (owner as usize) < state.faction_colors.len() {
                    let f_hex = state.faction_colors[owner as usize];
                    let f_color = hex_to_linear_rgb(f_hex);
                    let score = state.face_score[f];
                    let blend = (score.log10() * 0.08 - 0.1).clamp(0.1, 0.5);

                    for tc in &mut target_colors {
                        tc[0] += (f_color[0] - tc[0]) * blend;
                        tc[1] += (f_color[1] - tc[1]) * blend;
                        tc[2] += (f_color[2] - tc[2]) * blend;
                    }
                }

                let px0 = i0 * 12;
                let px1 = i1 * 12;
                let px2 = i2 * 12;

                mesh.vertices[px0 + 8] = target_colors[0][0];
                mesh.vertices[px0 + 9] = target_colors[0][1];
                mesh.vertices[px0 + 10] = target_colors[0][2];

                mesh.vertices[px1 + 8] = target_colors[1][0];
                mesh.vertices[px1 + 9] = target_colors[1][1];
                mesh.vertices[px1 + 10] = target_colors[1][2];

                mesh.vertices[px2 + 8] = target_colors[2][0];
                mesh.vertices[px2 + 9] = target_colors[2][1];
                mesh.vertices[px2 + 10] = target_colors[2][2];
            }

            mesh.version = mesh.version.wrapping_add(1);
            state.seed_value = rng.seed;
        }
    });
}

fn face_center(state: &PlanetSimulationState, face_id: usize) -> [f32; 3] {
    let o = face_id * 3;
    if o + 2 < state.face_centers.len() {
        [
            state.face_centers[o],
            state.face_centers[o + 1],
            state.face_centers[o + 2],
        ]
    } else {
        [0.0, 0.0, 0.0]
    }
}

#[inline]
fn face_has_adjacent_water(state: &PlanetSimulationState, f: usize) -> bool {
    if let Some(&cached) = state.has_adjacent_water.get(f) {

        let moisture = state.moistures.get(f).copied().unwrap_or(0.0);
        let drought = state.drought.get(f).copied().unwrap_or(0.0);
        let effective_inland_water = moisture * (1.0 - drought * 0.75);
        return cached == 1 || effective_inland_water >= 0.48;
    }
    if f >= state.is_water.len() {
        return false;
    }
    if state.is_water[f] > 0.5 {
        return true;
    }
    if f + 1 >= state.neighbors_offsets.len() {
        return false;
    }
    let start = state.neighbors_offsets[f] as usize;
    let end = (state.neighbors_offsets[f + 1] as usize).min(state.neighbors_flat.len());
    for idx in start..end {
        let n = state.neighbors_flat[idx] as usize;
        if n < state.is_water.len() && state.is_water[n] > 0.5 {
            return true;
        }
    }
    false
}

#[inline]
fn food_fraction(state: &PlanetSimulationState, f: usize) -> f32 {

    let inv = match state.inv_food_cap.get(f) {
        Some(&v) => v,
        None => {
            let cap = state.food_cap.get(f).copied().unwrap_or(0.0);
            if cap <= 0.0 {
                return 0.0;
            }
            1.0 / cap
        }
    };
    if inv <= 0.0 {
        return 0.0;
    }
    (state.food_stock.get(f).copied().unwrap_or(0.0) * inv).clamp(0.0, 1.0)
}

pub(crate) fn compute_distance_to_water(
    num_faces: usize,
    is_water: &[f32],
    neighbors_offsets: &[u32],
    neighbors_flat: &[u32],
) -> Vec<u32> {
    let mut dist = vec![u32::MAX; num_faces];
    let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();

    for f in 0..num_faces {
        if is_water.get(f).copied().unwrap_or(0.0) > 0.5 {
            continue;
        }
        if f + 1 >= neighbors_offsets.len() {
            continue;
        }
        let start = neighbors_offsets[f] as usize;
        let end = (neighbors_offsets[f + 1] as usize).min(neighbors_flat.len());
        let adj_water = (start..end).any(|idx| {
            let n = neighbors_flat[idx] as usize;
            is_water.get(n).copied().unwrap_or(0.0) > 0.5
        });
        if adj_water {
            dist[f] = 0;
            queue.push_back(f);
        }
    }

    while let Some(f) = queue.pop_front() {
        let d = dist[f];
        if f + 1 >= neighbors_offsets.len() {
            continue;
        }
        let start = neighbors_offsets[f] as usize;
        let end = (neighbors_offsets[f + 1] as usize).min(neighbors_flat.len());
        for idx in start..end {
            let n = neighbors_flat[idx] as usize;
            if n >= num_faces || is_water.get(n).copied().unwrap_or(0.0) > 0.5 {
                continue;
            }
            if dist[n] == u32::MAX {
                dist[n] = d + 1;
                queue.push_back(n);
            }
        }
    }

    dist
}

fn bfs_within_radius(
    neighbors_offsets: &[u32],
    neighbors_flat: &[u32],
    center: usize,
    radius: u32,
    mut visit: impl FnMut(usize, u32),
) {
    use std::collections::{HashSet, VecDeque};
    let mut visited: HashSet<usize> = HashSet::new();
    let mut queue: VecDeque<(usize, u32)> = VecDeque::new();
    visited.insert(center);
    queue.push_back((center, 0));
    while let Some((f, d)) = queue.pop_front() {
        visit(f, d);
        if d >= radius || f + 1 >= neighbors_offsets.len() {
            continue;
        }
        let start = neighbors_offsets[f] as usize;
        let end = (neighbors_offsets[f + 1] as usize).min(neighbors_flat.len());
        for idx in start..end {
            let n = neighbors_flat[idx] as usize;
            if visited.insert(n) {
                queue.push_back((n, d + 1));
            }
        }
    }
}

pub const BENCH_SYS_DROUGHT: u32 = 1 << 0;
pub const BENCH_SYS_RESOURCES: u32 = 1 << 1;
pub const BENCH_SYS_TRIBE_DYNAMICS: u32 = 1 << 2;
pub const BENCH_SYS_STEP_SETTLERS: u32 = 1 << 3;
pub const BENCH_SYS_FACE_COLOR: u32 = 1 << 4;
pub const BENCH_SYS_RENDER_HEIGHT: u32 = 1 << 5;

const DROUGHT_TICK_INTERVAL: f32 = 1.0;

pub fn sys_tick_drought(
    mut q_planet: Query<'_, (&mut PlanetSimulationState, &DynamicMesh, &SimTuning)>,
    time: Res<Time>,
) {
    let dt = time.delta_seconds.min(1.0);
    q_planet.for_each(|(state, mesh, tuning)| {

        if state.run_simulation <= 0.0 || state.bench_system_mask & BENCH_SYS_DROUGHT == 0 {
            return;
        }
        let num_faces = mesh.indices.len() / 3;
        if !valid_planet_state(state, num_faces) {
            return;
        }
        state.drought_tick_accum += dt;
        if state.drought_tick_accum < DROUGHT_TICK_INTERVAL {
            return;
        }
        let batch_dt = state.drought_tick_accum;
        state.drought_tick_accum = 0.0;

        for d in state.droughts.iter_mut() {
            d.remaining -= batch_dt;
        }
        state.droughts.retain(|d| d.remaining > 0.0);

        if (state.droughts.len() as f32) < tuning.max_droughts {

            let t = time.elapsed_seconds;
            let roll = ((t * 12.9898 + state.seed_value as f32 * 78.233).sin() * 43758.5453)
                .fract()
                .abs();
            if roll < tuning.drought_spawn_chance {
                let pick = ((t * 37.719 + state.seed_value as f32 * 91.345).sin() * 12345.6789)
                    .fract()
                    .abs();
                let center = (pick * num_faces as f32) as usize % num_faces;
                if state.is_water.get(center).copied().unwrap_or(1.0) < 0.5 {
                    let r1 = (((t + 1.0) * 53.219).sin() * 6789.123).fract().abs();
                    let r2 = (((t + 2.0) * 17.113).sin() * 4321.987).fract().abs();
                    let r3 = (((t + 3.0) * 91.771).sin() * 8765.432).fract().abs();
                    state.droughts.push(DroughtEvent {
                        center_face: center as u32,
                        radius: (tuning.drought_radius_min
                            + r1 * (tuning.drought_radius_max - tuning.drought_radius_min))
                            as u32,
                        remaining: tuning.drought_duration_min
                            + r2 * (tuning.drought_duration_max - tuning.drought_duration_min),
                        strength: tuning.drought_strength_min
                            + r3 * (tuning.drought_strength_max - tuning.drought_strength_min),
                    });
                }
            }
        }

        if state.drought.len() < num_faces {
            state.drought.resize(num_faces, 0.0);
        }
        if state.admin_drought.len() < num_faces {
            state.admin_drought.resize(num_faces, 0.0);
        }
        for f in 0..num_faces {
            state.drought[f] = state.admin_drought[f];
        }
        let events = state.droughts.clone();
        let mut drought_effects: Vec<(usize, f32)> = Vec::new();
        for ev in &events {
            let radius = ev.radius.max(1);

            let life_frac = (ev.remaining / 20.0).min(1.0);
            bfs_within_radius(
                &state.neighbors_offsets,
                &state.neighbors_flat,
                ev.center_face as usize,
                ev.radius,
                |f, d| {
                    let q = d as f32 / radius as f32;
                    let falloff = (1.0 - q * q).max(0.0);
                    let v = (ev.strength * falloff * life_frac).min(1.0);
                    drought_effects.push((f, v));
                },
            );
        }
        for (f, v) in drought_effects {
            if f < state.drought.len() && v > state.drought[f] {
                state.drought[f] = v;
            }
        }
    });
}

fn valid_planet_state(state: &PlanetSimulationState, num_faces: usize) -> bool {
    num_faces > 0
        && state.arability.len() >= num_faces
        && state.is_water.len() >= num_faces
        && state.temps.len() >= num_faces
        && state.neighbors_offsets.len() >= num_faces + 1
}

pub fn mesh_circle_marker(segments: u32, radius: f32) -> (Vec<f32>, Vec<u32>) {
    let mut v = Vec::with_capacity((segments as usize + 1) * 12);
    let mut i = Vec::with_capacity(segments as usize * 3);
    v.extend_from_slice(&[0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0]);
    for s in 0..segments {
        let theta = (s as f32 / segments as f32) * std::f32::consts::PI * 2.0;
        let (sin_t, cos_t) = theta.sin_cos();
        v.extend_from_slice(&[
            cos_t * radius,
            sin_t * radius,
            0.0,
            0.0,
            0.0,
            1.0,
            cos_t * 0.5 + 0.5,
            0.5 - sin_t * 0.5,
            1.0,
            1.0,
            1.0,
            1.0,
        ]);
        let curr = s + 1;
        let next = if s == segments - 1 { 1 } else { s + 2 };
        i.extend_from_slice(&[0, curr, next]);
    }
    (v, i)
}

pub fn sys_spawn_settlers(
    mut q_planet: Query<'_, (&mut PlanetSimulationState, &DynamicMesh, &SimTuning)>,
    mut q_settlers: Query<'_, (Entity, &Settler)>,
    mut commands: Commands,
) {
    q_planet.for_each(|(state, mesh, tuning)| {
        if state.run_simulation <= 0.0 || state.step_counter > 0 {
            return;
        }
        if q_settlers.iter().count() > 0 {
            return;
        }
        let num_faces = mesh.indices.len() / 3;
        if !valid_planet_state(state, num_faces) || mesh.vertices.len() < num_faces * 3 * 12 {
            return;
        }

        let target_count = (state.num_colonies.max(1.0) as usize).min(tuning.max_settlers as usize);
        let mut rng = SeededRng::new(state.seed_value ^ 0x5eed_1e55);
        let mut occupied = vec![0u32; num_faces];

        let is_safe_land = |state: &PlanetSimulationState, f: usize| -> bool {
            if f >= state.is_water.len() || state.is_water[f] > 0.5 {
                return false;
            }
            let t = state.temps.get(f).copied().unwrap_or(0.5);
            if t < 0.12 || t > 0.92 {
                return false;
            }
            let water_dist = state.dist_to_water.get(f).copied().unwrap_or(u32::MAX);

            water_dist <= 12 || state.moistures.get(f).copied().unwrap_or(0.0) >= 0.48
        };

        let tribe_count = (tuning.initial_tribe_count.max(1.0) as usize).min(64);
        let mut tribe_seed_face: Vec<usize> = Vec::with_capacity(tribe_count);
        let mut tribe_traits: Vec<(f32, f32, f32)> = Vec::with_capacity(tribe_count);
        const MIN_SEED_DIST_SQ: f32 = 9.0;
        for t in 0..tribe_count {
            let mut chosen = 0usize;
            for attempt in 0..80 {
                let f = (rng.next_f32() * num_faces as f32) as usize % num_faces;
                if !is_safe_land(state, f) {
                    continue;
                }
                let p = face_center(state, f);
                let far_enough = tribe_seed_face
                    .iter()
                    .all(|&sf| dist_sq(p, face_center(state, sf)) >= MIN_SEED_DIST_SQ);
                if far_enough || attempt == 79 {
                    chosen = f;
                    break;
                }
            }
            tribe_seed_face.push(chosen);

            tribe_traits.push((
                0.24 + rng.next_f32() * 0.58,
                0.18 + rng.next_f32() * 0.58,
                0.30 + rng.next_f32() * 0.42,
            ));

            state.next_tribe_id = (t + 1) as f32;
        }

        let mut placed = 0;
        let mut tries = 0;

        let (marker_vertices, marker_indices) = mesh_circle_marker(10, 1.0);
        while placed < target_count && tries < target_count * 200 + 5000 {
            tries += 1;
            let tribe = (rng.next_f32() * tribe_count as f32) as usize % tribe_count.max(1);

            let mut cur = tribe_seed_face[tribe];
            let walk_steps = 3 + (rng.next_f32() * 40.0) as u32;
            for _ in 0..walk_steps {
                if cur + 1 >= state.neighbors_offsets.len() {
                    break;
                }
                let start = state.neighbors_offsets[cur] as usize;
                let end =
                    (state.neighbors_offsets[cur + 1] as usize).min(state.neighbors_flat.len());
                let land_neighbors: Vec<usize> = (start..end)
                    .map(|idx| state.neighbors_flat[idx] as usize)
                    .filter(|&n| n < state.is_water.len() && state.is_water[n] < 0.5)
                    .collect();
                if land_neighbors.is_empty() {
                    break;
                }
                cur = land_neighbors[(rng.next_f32() * land_neighbors.len() as f32) as usize
                    % land_neighbors.len()];
            }
            let f = cur;
            if !is_safe_land(state, f) {
                continue;
            }

            let fs = state.neighbors_offsets[f] as usize;
            let fe = (state.neighbors_offsets[f + 1] as usize).min(state.neighbors_flat.len());
            let occupied_neighbors = (fs..fe)
                .filter(|&idx| occupied[state.neighbors_flat[idx] as usize] != 0)
                .count();
            if occupied_neighbors >= 5 {
                continue;
            }
            if occupied[f] != 0 {
                continue;
            }
            occupied[f] = 1;
            let pos = face_center(state, f);
            let (base_coop, base_agg, base_mob) = tribe_traits[tribe];
            let jitter = |rng: &mut SeededRng, base: f32| {
                (base + (rng.next_f32() - 0.5) * 0.24).clamp(0.0, 1.0)
            };
            let cooperation = jitter(&mut rng, base_coop);
            let aggression = jitter(&mut rng, base_agg);
            let mobility = jitter(&mut rng, base_mob);
            let (hue, color) = apply_tribe_color(tribe as f32);
            let mut e = commands.spawn();
            e = e

                .insert(Transform {
                    translation: lift_from_surface(pos),
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [0.018, 0.018, 0.018],
                })
                .insert(GlobalTransform::default())
                .insert(Billboard { active: 1 })
                .insert(Settler {
                    id: placed as f32,
                    face_index: f as f32,
                    hunger: 100.0,
                    thirst: 100.0,
                    hue,
                    cooldown: 0.0,
                    known_water_face: -1.0,
                    known_food_face: -1.0,
                    tribe_id: tribe as f32,

                    age: rng.next_f32() * tuning.maturity_age,
                    birth_cooldown: rng.next_f32() * tuning.birth_cooldown_interval,
                    cooperation,
                    aggression,
                    mobility,

                    render_slot: hash01(placed as u32, 0x9f1a) * 20000.0,
                    previous_face: -1.0,
                    move_commitment: 0.0,
                })
                .insert(StandardMaterial {
                    base_color: color,
                    roughness: 0.6,
                    metallic: 0.1,
                    ..Default::default()
                });

            if state.settler_mesh_id >= 0.0 {
                e.insert(MeshHandle {
                    id: state.settler_mesh_id,
                });
            } else {
                e.insert(DynamicMesh {
                    vertices: marker_vertices.clone(),
                    indices: marker_indices.clone(),
                    version: 1,
                    color_version: 0,
                });
            }
            placed += 1;
        }

        state.step_counter = 1;

        state.next_settler_id = placed as f32;
    });
}

fn hue_to_rgb(hue: f32) -> [f32; 4] {
    let h = hue / 60.0;
    let c = 1.0_f32;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [0.3 + r * 0.7, 0.3 + g * 0.7, 0.3 + b * 0.7, 1.0]
}

fn nearest_empty_land_face(
    state: &PlanetSimulationState,
    origin: usize,
    occupied: &[u32],
    allow_origin: bool,
) -> Option<usize> {
    if origin >= occupied.len() {
        return None;
    }
    let mut seen = vec![false; occupied.len()];
    let mut queue = std::collections::VecDeque::new();
    seen[origin] = true;
    queue.push_back(origin);
    while let Some(face) = queue.pop_front() {
        if (allow_origin || face != origin)
            && occupied[face] == 0
            && state.is_water.get(face).copied().unwrap_or(1.0) <= 0.5
        {
            return Some(face);
        }
        if face + 1 >= state.neighbors_offsets.len() {
            continue;
        }
        let start = state.neighbors_offsets[face] as usize;
        let end = (state.neighbors_offsets[face + 1] as usize).min(state.neighbors_flat.len());
        for idx in start..end {
            let next = state.neighbors_flat[idx] as usize;
            if next < seen.len() && !seen[next] {
                seen[next] = true;
                queue.push_back(next);
            }
        }
    }
    None
}

const SETTLER_SURFACE_LIFT: f32 = 0.12;

fn lift_from_surface(pos: [f32; 3]) -> [f32; 3] {
    let len = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2]).sqrt();
    if len < 1e-6 {
        return pos;
    }
    let inv = SETTLER_SURFACE_LIFT / len;
    [
        pos[0] + pos[0] * inv,
        pos[1] + pos[1] * inv,
        pos[2] + pos[2] * inv,
    ]
}

fn jitter_target(pos: [f32; 3], id: f32) -> [f32; 3] {
    let a = hash01(id as u32, 0x4a17);
    let b = hash01(id as u32, 0x9d2b);
    let ja = a - 0.5;
    let jb = b - 0.5;
    let radius = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2]).sqrt();
    let nudged = [
        pos[0] + ja * 0.03,
        pos[1] + jb * 0.03,
        pos[2] + (ja - jb) * 0.015,
    ];
    let len = (nudged[0] * nudged[0] + nudged[1] * nudged[1] + nudged[2] * nudged[2])
        .sqrt()
        .max(1e-6);
    let scale = radius / len;
    [nudged[0] * scale, nudged[1] * scale, nudged[2] * scale]
}

#[inline(always)]
fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

#[inline(always)]
fn hash01(a: u32, b: u32) -> f32 {

    (hash_u32(a ^ hash_u32(b.wrapping_add(0x9e37_79b9))) >> 8) as f32 * (1.0 / 16_777_216.0)
}

fn dist_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

#[allow(dead_code)]
fn tangent_basis(n: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let up = if n[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let (cx, cy, cz) = (
        n[1] * up[2] - n[2] * up[1],
        n[2] * up[0] - n[0] * up[2],
        n[0] * up[1] - n[1] * up[0],
    );
    let l = (cx * cx + cy * cy + cz * cz).sqrt().max(1e-6);
    let t1 = [cx / l, cy / l, cz / l];
    let t2 = [
        n[1] * t1[2] - n[2] * t1[1],
        n[2] * t1[0] - n[0] * t1[2],
        n[0] * t1[1] - n[1] * t1[0],
    ];
    (t1, t2)
}

#[allow(dead_code)]
fn explore_heading_target(
    pos: [f32; 3],
    id: f32,
    hue: f32,
    elapsed: f32,
    epoch_secs: f32,
    distance: f32,
) -> [f32; 3] {
    let epoch = (elapsed / epoch_secs) as u32;
    let seed = (id as u32)
        .wrapping_add((hue * 97.0) as u32)
        .wrapping_add(epoch.wrapping_mul(2654435761));
    let theta = (seed as f32 / 4294967295.0) * std::f32::consts::TAU;
    let radius = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2])
        .sqrt()
        .max(1e-6);
    let n = [pos[0] / radius, pos[1] / radius, pos[2] / radius];
    let (t1, t2) = tangent_basis(n);
    let (s, c) = theta.sin_cos();
    let far = [
        pos[0] + (t1[0] * c + t2[0] * s) * distance,
        pos[1] + (t1[1] * c + t2[1] * s) * distance,
        pos[2] + (t1[2] * c + t2[2] * s) * distance,
    ];
    let len = (far[0] * far[0] + far[1] * far[1] + far[2] * far[2])
        .sqrt()
        .max(1e-6);
    let scale = radius / len;
    [far[0] * scale, far[1] * scale, far[2] * scale]
}

#[allow(clippy::too_many_arguments)]
fn resolve_seek_target(
    state: &PlanetSimulationState,
    pos: [f32; 3],
    f: usize,
    start: usize,
    end: usize,
    seeking_water: bool,
    known_target: f32,
    id: f32,
    hue: f32,
    elapsed: f32,
    occupied: &[u32],
    blocked_face: Option<usize>,
) -> [f32; 3] {
    let known_face = if known_target >= 0.0 && (known_target as usize) < occupied.len() {
        Some(known_target as usize)
    } else {
        None
    };
    let mut best = f;
    let mut best_score = f32::MIN;
    let mut found = false;
    for idx in start..end {
        let n = state.neighbors_flat[idx] as usize;
        if blocked_face == Some(n) {
            continue;
        }
        if n >= state.is_water.len() || state.is_water[n] > 0.5 {
            continue;
        }
        if occupied.get(n).copied().unwrap_or(1) != 0 {
            continue;
        }
        let score = if seeking_water {

            let base = if face_has_adjacent_water(state, n) {
                1.0
            } else {
                0.0
            };

            let water_dist = state
                .dist_to_water
                .get(n)
                .copied()
                .unwrap_or(u32::MAX)
                .min(100) as f32;
            base + state.moistures.get(n).copied().unwrap_or(0.0) * 0.3 - water_dist * 0.08
        } else {

            food_fraction(state, n)
        };

        let tie_break = (hash01(id as u32, n as u32) - 0.5) * 0.05;

        let memory_pull = known_face
            .map(|target| -dist_sq(face_center(state, n), face_center(state, target)) * 0.08)
            .unwrap_or(0.0);
        let score = score + memory_pull + tie_break;
        if score > best_score {
            best_score = score;
            best = n;
            found = true;
        }
    }
    if found {
        return face_center(state, best);
    }

    let _ = (id, hue, elapsed);
    pos
}

const RESOURCE_TICK_INTERVAL: f32 = 0.5;

pub fn sys_tick_resources(
    mut q_planet: Query<'_, (&mut PlanetSimulationState, &DynamicMesh, &SimTuning)>,
    time: Res<Time>,
) {
    let dt = time.delta_seconds.min(1.0);
    q_planet.for_each(|(state, mesh, tuning)| {

        if state.run_simulation <= 0.0 || state.bench_system_mask & BENCH_SYS_RESOURCES == 0 {
            return;
        }
        state.resource_tick_accum += dt;
        if state.resource_tick_accum < RESOURCE_TICK_INTERVAL {
            return;
        }
        let batch_dt = state.resource_tick_accum;
        state.resource_tick_accum = 0.0;

        let num_faces = mesh.indices.len() / 3;
        if state.food_stock.len() < num_faces
            || state.food_cap.len() < num_faces
            || state.food_regen.len() < num_faces
        {
            return;
        }
        let has_drought = state.drought.len() >= num_faces;
        for f in 0..num_faces {
            let cap = state.food_cap[f];
            if cap <= 0.0 {
                continue;
            }
            let stock = state.food_stock[f];

            let dampen = if has_drought {
                1.0 - state.drought[f] * tuning.drought_regen_dampen
            } else {
                1.0
            };
            let growth = state.food_regen[f] * (1.0 - stock / cap) * dampen.max(0.0) * batch_dt;
            state.food_stock[f] = (stock + growth).clamp(0.0, cap);
        }
    });
}

const FACE_COLOR_TICK_INTERVAL: f32 = 2.5;

const FACE_COLOR_CHANGE_EPS: f32 = 0.01;

pub fn sys_tick_face_color(
    mut q_planet: Query<'_, (&mut PlanetSimulationState, &mut DynamicMesh)>,
    time: Res<Time>,
) {
    let dt = time.delta_seconds.min(1.0);
    q_planet.for_each(|(state, mesh)| {
        if state.run_simulation <= 0.0 || state.bench_system_mask & BENCH_SYS_FACE_COLOR == 0 {
            return;
        }
        state.face_color_tick_accum += dt;
        if state.face_color_tick_accum < FACE_COLOR_TICK_INTERVAL {
            return;
        }
        state.face_color_tick_accum = 0.0;

        let num_faces = mesh.indices.len() / 3;
        if state.base_colors.is_empty()
            || state.food_cap.len() < num_faces
            || state.drought.len() < num_faces
            || state.face_dominant_tribe.len() < num_faces
            || state.is_water.len() < num_faces
        {
            return;
        }

        let mut any_changed = false;
        for f in 0..num_faces {
            if state.is_water[f] > 0.5 {
                continue;
            }
            let cap = state.food_cap.get(f).copied().unwrap_or(0.0);
            let food_frac = if cap > 0.0 {
                (state.food_stock.get(f).copied().unwrap_or(0.0) / cap).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let drought = state.drought[f];
            let dom = state.face_dominant_tribe[f];

            let i0 = mesh.indices[f * 3] as usize;
            let bo = i0 * 3;
            let (br, bg, bb) = (
                state.base_colors.get(bo).copied().unwrap_or(0.3),
                state.base_colors.get(bo + 1).copied().unwrap_or(0.3),
                state.base_colors.get(bo + 2).copied().unwrap_or(0.3),
            );

            const SAT_FLOOR: f32 = 0.35;
            let gray = (br + bg + bb) / 3.0;
            let sat = SAT_FLOOR + food_frac * (1.0 - SAT_FLOOR);
            let mut r = gray + (br - gray) * sat;
            let mut g = gray + (bg - gray) * sat;
            let mut b = gray + (bb - gray) * sat;

            let darken = 1.0 - (1.0 - food_frac) * 0.15;
            r *= darken;
            g *= darken;
            b *= darken;

            if drought > 0.0 {

                let t = (drought * 0.5).min(0.5);
                r += (gray * 0.85 - r) * t;
                g += (gray * 0.85 - g) * t;
                b += (gray * 0.85 - b) * t;
            }

            if dom >= 0 {

                let (_, tribe_color) = apply_tribe_color(dom as f32);
                const TRIBE_MIX: f32 = 0.08;
                r += (tribe_color[0] - r) * TRIBE_MIX;
                g += (tribe_color[1] - g) * TRIBE_MIX;
                b += (tribe_color[2] - b) * TRIBE_MIX;
            }

            for k in 0..3 {
                let vi = mesh.indices[f * 3 + k] as usize;
                let vo = vi * 12;
                if vo + 10 < mesh.vertices.len() {
                    let existing = (
                        mesh.vertices[vo + 8],
                        mesh.vertices[vo + 9],
                        mesh.vertices[vo + 10],
                    );
                    if (existing.0 - r).abs() > FACE_COLOR_CHANGE_EPS
                        || (existing.1 - g).abs() > FACE_COLOR_CHANGE_EPS
                        || (existing.2 - b).abs() > FACE_COLOR_CHANGE_EPS
                    {
                        any_changed = true;
                    }
                    mesh.vertices[vo + 8] = r;
                    mesh.vertices[vo + 9] = g;
                    mesh.vertices[vo + 10] = b;
                }
            }
        }
        if any_changed {

            mesh.color_version = mesh.color_version.wrapping_add(1);
        }
    });
}

pub fn sys_step_settlers(
    mut q_planet: Query<'_, (&mut PlanetSimulationState, &DynamicMesh, &SimTuning)>,
    mut q_settlers: Query<'_, (Entity, &mut Settler, &mut Transform)>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let dt = time.delta_seconds.min(1.0);

    q_planet.for_each(|(state, mesh, tuning)| {
        if state.bench_system_mask & BENCH_SYS_STEP_SETTLERS == 0 {
            return;
        }
        let num_faces = mesh.indices.len() / 3;
        if !valid_planet_state(state, num_faces) || mesh.vertices.len() < num_faces * 3 * 12 {
            return;
        }

        let mut population_count = q_settlers.len() as u32;

        let mut occupied = vec![0u32; num_faces];
        q_settlers.for_each(|(_, settler, _)| {
            let face = settler.face_index as usize;
            if settler.face_index >= 0.0 && face < num_faces {
                occupied[face] += 1;
            }
        });
        q_settlers.for_each(|(ent, settler, transform)| {

            if settler.face_index < 0.0 || settler.face_index as usize >= num_faces {
                commands.despawn(ent);
                return;
            }
            let mut f = settler.face_index as usize;
            settler.move_commitment = (settler.move_commitment - dt).max(0.0);
            let blocked_face = if settler.move_commitment > 0.0 && settler.previous_face >= 0.0 {
                Some(settler.previous_face as usize)
            } else {
                None
            };
            occupied[f] = occupied[f].saturating_sub(1);

            if occupied[f] > 0 {
                if let Some(free) = nearest_empty_land_face(state, f, &occupied, false) {
                    f = free;
                    settler.face_index = free as f32;
                    transform.translation = lift_from_surface(face_center(state, free));
                } else {

                    commands.despawn(ent);
                    population_count = population_count.saturating_sub(1);
                    return;
                }
            }
            occupied[f] += 1;
            let temp = state.temps[f];
            let moisture = state.moistures.get(f).copied().unwrap_or(0.0);
            let arability_here = state.arability[f];
            let f_has_water = face_has_adjacent_water(state, f);
            let climate_penalty = if temp < tuning.cold_line {
                (tuning.cold_line - temp) * tuning.climate_penalty_mult
            } else if temp > tuning.hot_line {
                (temp - tuning.hot_line) * tuning.climate_penalty_mult
            } else {
                0.0
            };

            let thirst_rate =
                (tuning.thirst_per_sec - moisture * 0.3 - if f_has_water { 0.35 } else { 0.0 })
                    .max(0.03);
            let hunger_rate = (tuning.hunger_per_sec - arability_here * 0.25).max(0.03);
            settler.hunger -= (hunger_rate + climate_penalty * 0.5) * dt;
            settler.thirst -= (thirst_rate + climate_penalty * 0.6) * dt;

            let start = state.neighbors_offsets[f] as usize;
            let end = (state.neighbors_offsets[f + 1] as usize).min(state.neighbors_flat.len());

            if f_has_water {
                settler.known_water_face = f as f32;
            }
            if food_fraction(state, f) > 0.3 {
                settler.known_food_face = f as f32;
            }

            let mut known_food_fraction = if settler.known_food_face >= 0.0
                && (settler.known_food_face as usize) < state.food_cap.len()
            {
                food_fraction(state, settler.known_food_face as usize)
            } else {
                -1.0
            };
            for idx in start..end {
                let n = state.neighbors_flat[idx] as usize;
                if n >= state.is_water.len() || state.is_water[n] > 0.5 {
                    continue;
                }
                if face_has_adjacent_water(state, n) && settler.known_water_face < 0.0 {
                    settler.known_water_face = n as f32;
                }
                let n_food_fraction = food_fraction(state, n);
                if n_food_fraction > 0.3 && n_food_fraction > known_food_fraction {
                    settler.known_food_face = n as f32;
                    known_food_fraction = n_food_fraction;
                }
            }

            if f_has_water {
                settler.thirst = (settler.thirst + 12.0 * dt).min(100.0);
            }
            let hunger_need = (100.0 - settler.hunger).max(0.0);
            if hunger_need > 0.0 {
                let food_here = state.food_stock.get(f).copied().unwrap_or(0.0);
                if food_here > 0.0 {
                    let take = (tuning.food_eat_rate * dt).min(hunger_need).min(food_here);
                    settler.hunger += take;
                    if let Some(stock) = state.food_stock.get_mut(f) {
                        *stock -= take;
                    }
                }
            }

            let mut target_pos: Option<[f32; 3]> = None;
            let thirst_first = settler.thirst <= settler.hunger;
            let elapsed = time.elapsed_seconds;

            macro_rules! seek_water {
                () => {
                    if !f_has_water && settler.thirst < tuning.rest_threshold {
                        target_pos = Some(resolve_seek_target(
                            state,
                            transform.translation,
                            f,
                            start,
                            end,
                            true,
                            settler.known_water_face,
                            settler.id,
                            settler.hue,
                            elapsed,
                            &occupied,
                            blocked_face,
                        ));
                    }
                };
            }
            macro_rules! seek_food {
                () => {
                    if food_fraction(state, f) <= 0.1 && settler.hunger < tuning.rest_threshold {
                        target_pos = Some(resolve_seek_target(
                            state,
                            transform.translation,
                            f,
                            start,
                            end,
                            false,
                            settler.known_food_face,
                            settler.id,
                            settler.hue,
                            elapsed,
                            &occupied,
                            blocked_face,
                        ));
                    }
                };
            }

            if thirst_first {
                seek_water!();
                if target_pos.is_none() {
                    seek_food!();
                }
            } else {
                seek_food!();
                if target_pos.is_none() {
                    seek_water!();
                }
            }

            let mut is_idle_wander = false;

            if target_pos.is_none() {
                let occupied_neighbors = (start..end)
                    .filter(|&idx| {
                        let n = state.neighbors_flat[idx] as usize;
                        n < occupied.len() && occupied[n] != 0
                    })
                    .count();
                if occupied_neighbors >= 4 {
                    let mut pressure_target = None;
                    let mut pressure_score = f32::MIN;
                    for idx in start..end {
                        let n = state.neighbors_flat[idx] as usize;
                        if blocked_face == Some(n) {
                            continue;
                        }
                        if n >= num_faces || state.is_water[n] > 0.5 || occupied[n] != 0 {
                            continue;
                        }
                        let inland =
                            state.dist_to_water.get(n).copied().unwrap_or(0).min(12) as f32;
                        let moisture = state.moistures.get(n).copied().unwrap_or(0.0);
                        let score = food_fraction(state, n) * 5.0
                            + moisture * 1.5
                            + inland * 0.18
                            + hash01(settler.id as u32, n as u32 ^ 0x71c3) * 0.15;
                        if score > pressure_score {
                            pressure_score = score;
                            pressure_target = Some(n);
                        }
                    }
                    if let Some(n) = pressure_target {
                        target_pos = Some(face_center(state, n));
                        is_idle_wander = true;
                        settler.cooldown = tuning.idle_wander_interval;
                    }
                }
            }
            if target_pos.is_none() {
                let my_tribe = settler.tribe_id as i32;
                let score_face = |ff: usize| -> f32 {
                    let pop = state.face_population.get(ff).copied().unwrap_or(0) as f32;
                    let dom = state.face_dominant_tribe.get(ff).copied().unwrap_or(-1);

                    let mut same = if dom == my_tribe { pop } else { 0.0 };
                    let mut foreign = if dom >= 0 && dom != my_tribe {
                        pop
                    } else {
                        0.0
                    };
                    if ff + 1 < state.neighbors_offsets.len() {
                        let ns = state.neighbors_offsets[ff] as usize;
                        let ne = (state.neighbors_offsets[ff + 1] as usize)
                            .min(state.neighbors_flat.len());
                        for ni in ns..ne {
                            let nf = state.neighbors_flat[ni] as usize;
                            let nt = state.face_dominant_tribe.get(nf).copied().unwrap_or(-1);
                            if nt == my_tribe {
                                same += 1.0;
                            } else if nt >= 0 {
                                foreign += 1.0;
                            }
                        }
                    }

                    let frac = food_fraction(state, ff);
                    frac * tuning.social_food_weight

                        - (1.0 - frac) * tuning.social_scarcity_weight

                        - (same + foreign) * tuning.social_crowd_weight * 0.12
                        + same.sqrt() * settler.cooperation * tuning.social_cohesion_weight
                        + foreign.sqrt()
                            * (settler.aggression * 2.0 - 1.0)
                            * tuning.social_hostility_weight
                };
                let here_score = score_face(f);
                let mut best_n = f;
                let mut best_score = here_score;
                for idx in start..end {
                    let n = state.neighbors_flat[idx] as usize;
                    if blocked_face == Some(n) {
                        continue;
                    }
                    if n >= state.is_water.len() || state.is_water[n] > 0.5 {
                        continue;
                    }
                    if occupied[n] != 0 {
                        continue;
                    }
                    let tie_break = (hash01(settler.id as u32, n as u32 ^ 0x5bf0) - 0.5) * 0.3;
                    let s = score_face(n) + tie_break;
                    if s > best_score {
                        best_score = s;
                        best_n = n;
                    }
                }

                if best_n != f && best_score > here_score + tuning.social_move_threshold {
                    target_pos = Some(face_center(state, best_n));
                    is_idle_wander = true;
                    settler.cooldown = tuning.idle_wander_interval;
                } else {

                    settler.cooldown -= dt;
                    if settler.cooldown <= 0.0 {
                        settler.cooldown = tuning.idle_wander_interval;

                        let mut wander_face = None;
                        let mut wander_key = -1.0f32;
                        for wi in start..end {
                            let n = state.neighbors_flat[wi] as usize;
                            if blocked_face == Some(n) {
                                continue;
                            }
                            if n >= num_faces || state.is_water[n] > 0.5 || occupied[n] != 0 {
                                continue;
                            }
                            let key = hash01(settler.id as u32 ^ elapsed as u32, n as u32 ^ 0xa731);
                            if key > wander_key {
                                wander_key = key;
                                wander_face = Some(n);
                            }
                        }
                        if let Some(n) = wander_face {
                            target_pos = Some(face_center(state, n));
                            is_idle_wander = true;
                        }
                    }
                }
            } else {

                settler.cooldown = tuning.idle_wander_interval;
            }

            if let Some(raw_target) = target_pos {
                let old_translation = transform.translation;
                let final_target = lift_from_surface(jitter_target(raw_target, settler.id));
                let dx = final_target[0] - transform.translation[0];
                let dy = final_target[1] - transform.translation[1];
                let dz = final_target[2] - transform.translation[2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist > 1e-5 {

                    let mobility_speed = tuning.move_speed * (0.5 + settler.mobility);
                    let step = (mobility_speed * dt).min(dist);
                    let inv = step / dist;
                    transform.translation[0] += dx * inv;
                    transform.translation[1] += dy * inv;
                    transform.translation[2] += dz * inv;
                    let cost_mult = if is_idle_wander {
                        tuning.wander_cost_mult
                    } else {
                        1.0
                    };
                    settler.hunger -= step * tuning.move_hunger_cost * cost_mult;
                    settler.thirst -= step * tuning.move_thirst_cost * cost_mult;
                }

                let proposed_translation = transform.translation;
                let mut nearest = f;
                let mut nearest_d = dist_sq(proposed_translation, face_center(state, f));
                for idx in start..end {
                    let n = state.neighbors_flat[idx] as usize;
                    if n >= num_faces {
                        continue;
                    }
                    let d = dist_sq(proposed_translation, face_center(state, n));
                    if d < nearest_d {
                        nearest_d = d;
                        nearest = n;
                    }
                }
                if nearest != f {
                    if occupied[nearest] == 0
                        && state.is_water.get(nearest).copied().unwrap_or(1.0) <= 0.5
                    {
                        occupied[f] = occupied[f].saturating_sub(1);
                        occupied[nearest] += 1;
                        settler.previous_face = f as f32;

                        settler.move_commitment = 1.5;
                        settler.face_index = nearest as f32;
                    } else {

                        transform.translation = old_translation;
                    }
                }

            }

            settler.age += dt;
            if settler.birth_cooldown > 0.0 {
                settler.birth_cooldown -= dt;
            }

            let trait_avg = (settler.cooperation + settler.aggression + settler.mobility) / 3.0;
            let effective_birth_need =
                tuning.birth_need * (1.0 - trait_avg * tuning.trait_birth_need_discount).max(0.4);
            if settler.age >= tuning.maturity_age
                && settler.birth_cooldown <= 0.0
                && settler.hunger >= effective_birth_need
                && settler.thirst >= effective_birth_need
                && population_count < tuning.max_settlers as u32
            {
                let parent_face = settler.face_index as usize;
                let birth_face = nearest_empty_land_face(state, parent_face, &occupied, false);
                if let Some(birth_face) =
                    birth_face.filter(|&face| food_fraction(state, face) >= tuning.birth_food_floor)
                {
                    settler.birth_cooldown = tuning.birth_cooldown_interval;

                    let trait_avg =
                        (settler.cooperation + settler.aggression + settler.mobility) / 3.0;
                    let discount = (trait_avg * tuning.trait_reproduction_discount).min(0.85);
                    let effective_birth_cost = tuning.birth_hunger_cost * (1.0 - discount);
                    settler.hunger -= effective_birth_cost;
                    if let Some(stock) = state.food_stock.get_mut(birth_face) {
                        *stock = (*stock - effective_birth_cost).max(0.0);
                    }
                    let child_id = state.next_settler_id;
                    state.next_settler_id += 1.0;
                    state.births_total += 1;
                    occupied[birth_face] += 1;

                    let mutate = |seed: f32, base: f32| -> f32 {
                        let j = ((seed.sin() * 43758.5453).fract() - 0.5)
                            * 2.0
                            * tuning.tribe_mutation_amount;
                        (base + j).clamp(0.0, 1.0)
                    };
                    let child_coop = mutate(child_id * 12.9898 + 1.0, settler.cooperation);
                    let child_agg = mutate(child_id * 12.9898 + 2.0, settler.aggression);
                    let child_mob = mutate(child_id * 12.9898 + 3.0, settler.mobility);

                    let cultural_distance = ((child_coop - settler.cooperation).powi(2)
                        + (child_agg - settler.aggression).powi(2)
                        + (child_mob - settler.mobility).powi(2))
                    .sqrt();
                    let split_roll = ((child_id * 53.219).sin() * 6789.123).fract().abs();
                    let child_tribe = if cultural_distance > tuning.tribe_split_threshold
                        && split_roll < tuning.tribe_split_chance
                    {
                        let new_tribe = state.next_tribe_id;
                        state.next_tribe_id += 1.0;
                        state.tribe_splits += 1;
                        new_tribe
                    } else {
                        settler.tribe_id
                    };
                    let (child_hue, child_color) = if child_tribe != settler.tribe_id {
                        apply_tribe_color(child_tribe)
                    } else {
                        (settler.hue, hue_to_rgb(settler.hue))
                    };

                    let mut e = commands.spawn();
                    e = e
                        .insert(Transform {
                            translation: lift_from_surface(face_center(state, birth_face)),
                            rotation: [0.0, 0.0, 0.0, 1.0],
                            scale: [0.018, 0.018, 0.018],
                        })
                        .insert(GlobalTransform::default())
                        .insert(Billboard { active: 1 })
                        .insert(Settler {
                            id: child_id,
                            face_index: birth_face as f32,
                            hunger: 60.0,
                            thirst: 60.0,
                            hue: child_hue,
                            cooldown: 0.0,
                            known_water_face: settler.known_water_face,
                            known_food_face: settler.known_food_face,
                            tribe_id: child_tribe,
                            age: 0.0,
                            birth_cooldown: tuning.birth_cooldown_interval,
                            cooperation: child_coop,
                            aggression: child_agg,
                            mobility: child_mob,

                            render_slot: hash01(child_id as u32, 0x9f1a) * 20000.0,
                            previous_face: -1.0,
                            move_commitment: 0.0,
                        })
                        .insert(StandardMaterial {
                            base_color: child_color,
                            roughness: 0.6,
                            metallic: 0.1,
                            ..Default::default()
                        });

                    if state.settler_mesh_id >= 0.0 {
                        e.insert(MeshHandle {
                            id: state.settler_mesh_id,
                        });
                    } else {
                        let (marker_vertices, marker_indices) = mesh_circle_marker(10, 1.0);
                        e.insert(DynamicMesh {
                            vertices: marker_vertices,
                            indices: marker_indices,
                            version: 1,
                            color_version: 0,
                        });
                    }
                    population_count += 1;
                }
            }

            if settler.age >= tuning.lifespan {
                state.deaths_aged += 1;
                commands.despawn(ent);
            } else if settler.hunger <= 0.0 || settler.thirst <= 0.0 {
                state.deaths_starved += 1;
                commands.despawn(ent);
            }
        });
    });
}

pub fn sys_snap_settler_render_height(
    mut q_planet: Query<'_, &PlanetSimulationState>,
    mut q_settlers: Query<'_, (&Settler, &Transform, &mut GPUInstanceTransform)>,
) {
    let state = match q_planet.iter().next() {
        Some(s) => s,
        None => return,
    };
    if state.bench_system_mask & BENCH_SYS_RENDER_HEIGHT == 0 {
        return;
    }
    q_settlers.par_for_each(|(settler, transform, gpu_t)| {
        let f = settler.face_index as usize;
        let local = face_center(state, f);
        let local_radius = (local[0] * local[0] + local[1] * local[1] + local[2] * local[2]).sqrt();
        if local_radius < 1e-6 {
            return;
        }
        let walk = transform.translation;
        let walk_radius = (walk[0] * walk[0] + walk[1] * walk[1] + walk[2] * walk[2]).sqrt();

        let direction = if walk_radius > 1e-6 { walk } else { local };
        let direction_len = if walk_radius > 1e-6 {
            walk_radius
        } else {
            local_radius
        };
        let scale = (local_radius + SETTLER_SURFACE_LIFT) / direction_len;
        gpu_t.translation = [
            direction[0] * scale,
            direction[1] * scale,
            direction[2] * scale,
        ];
    });
}

pub(crate) fn apply_tribe_color(tribe_id: f32) -> (f32, [f32; 4]) {
    let hue = (tribe_id * 137.508).rem_euclid(360.0);
    (hue, hue_to_rgb(hue))
}

const TRIBE_DYNAMICS_TICK_INTERVAL: f32 = 0.3;

pub fn sys_tribe_dynamics(
    mut q_planet: Query<'_, (&mut PlanetSimulationState, &SimTuning)>,
    mut q_settlers: Query<'_, (Entity, &mut Settler)>,
    time: Res<Time>,
) {
    let elapsed = time.elapsed_seconds;
    let dt = time.delta_seconds.min(1.0);
    q_planet.for_each(|(state, tuning)| {

        if state.run_simulation <= 0.0 || state.bench_system_mask & BENCH_SYS_TRIBE_DYNAMICS == 0 {
            return;
        }

        state.tribe_dynamics_tick_accum += dt;
        if state.tribe_dynamics_tick_accum < TRIBE_DYNAMICS_TICK_INTERVAL {
            return;
        }
        state.tribe_dynamics_tick_accum = 0.0;
        if state.face_population.is_empty() || state.face_dominant_tribe.is_empty() {
            return;
        }
        for p in state.face_population.iter_mut() {
            *p = 0;
        }
        for t in state.face_dominant_tribe.iter_mut() {
            *t = -1;
        }

        let mut by_face: std::collections::HashMap<u32, Vec<(Entity, f32, f32, f32, f32)>> =
            std::collections::HashMap::new();
        q_settlers.for_each(|(ent, settler)| {
            by_face.entry(settler.face_index as u32).or_default().push((
                ent,
                settler.hunger,
                settler.tribe_id,
                settler.cooperation,
                settler.aggression,
            ));
        });

        let mut deltas: std::collections::HashMap<Entity, f32> = std::collections::HashMap::new();
        let mut cooperation_events = 0u32;
        let mut aggression_events = 0u32;

        for (&f, group) in by_face.iter() {
            let fi = f as usize;
            if fi < state.face_population.len() {
                state.face_population[fi] = group.len() as u32;
            }

            if fi < state.face_dominant_tribe.len() {

                let mut best_tribe = -1i32;
                let mut best_count = 0u32;
                for (i, &(_, _, tribe_i, _, _)) in group.iter().enumerate() {
                    let t = tribe_i as i32;

                    if group[..i]
                        .iter()
                        .any(|&(_, _, prev, _, _)| prev as i32 == t)
                    {
                        continue;
                    }
                    let mut c = 0u32;
                    for &(_, _, tribe_j, _, _) in group.iter() {
                        if tribe_j as i32 == t {
                            c += 1;
                        }
                    }
                    if c > best_count {
                        best_count = c;
                        best_tribe = t;
                    }
                }
                if best_tribe >= 0 {
                    state.face_dominant_tribe[fi] = best_tribe;
                }
            }

            if group.len() < 2 {
                continue;
            }

            let mut idx = 0;
            while idx + 1 < group.len() {
                let (ea, hunger_a, tribe_a, coop_a, agg_a) = group[idx];
                let (eb, hunger_b, tribe_b, coop_b, agg_b) = group[idx + 1];
                idx += 2;

                let gate = hash01(
                    ea.id ^ eb.id.wrapping_mul(3),
                    f ^ (elapsed as u32).wrapping_mul(17),
                );
                if gate >= tuning.interaction_chance {
                    continue;
                }

                if tribe_a == tribe_b {

                    let (giver, receiver, g_hunger, r_hunger, g_coop) = if hunger_a >= hunger_b {
                        (ea, eb, hunger_a, hunger_b, coop_a)
                    } else {
                        (eb, ea, hunger_b, hunger_a, coop_b)
                    };
                    if g_hunger - r_hunger > 10.0 && g_hunger > 30.0 && g_coop > 0.05 {
                        let amount = (tuning.cooperation_transfer_rate * g_coop)
                            .min((g_hunger - r_hunger) * 0.5);
                        if amount > 0.0 {

                            *deltas.entry(giver).or_insert(0.0) -=
                                amount * tuning.cooperation_giver_cost_frac;
                            *deltas.entry(receiver).or_insert(0.0) += amount;
                            cooperation_events += 1;
                        }
                    }
                } else {

                    let (attacker, victim, a_agg, v_hunger) = if agg_a >= agg_b {
                        (ea, eb, agg_a, hunger_b)
                    } else {
                        (eb, ea, agg_b, hunger_a)
                    };
                    let attack_roll = hash01(
                        attacker.id.wrapping_mul(7) ^ victim.id,
                        f ^ (elapsed as u32).wrapping_mul(23) ^ 0x2f1d,
                    );
                    if a_agg > 0.15 && attack_roll < a_agg && v_hunger > 0.0 {
                        let steal = tuning.aggression_steal_rate.min(v_hunger);

                        if steal > 0.0
                            && steal * tuning.aggression_yield > tuning.aggression_energy_cost
                        {
                            *deltas.entry(attacker).or_insert(0.0) +=
                                steal * tuning.aggression_yield - tuning.aggression_energy_cost;
                            *deltas.entry(victim).or_insert(0.0) -= steal;
                            aggression_events += 1;
                        }
                    }
                }
            }
        }

        for (&f, group) in by_face.iter() {
            let Some(&(ea, hunger_a, tribe_a, coop_a, agg_a)) = group.first() else {
                continue;
            };
            let fi = f as usize;
            if fi + 1 >= state.neighbors_offsets.len() {
                continue;
            }
            let ns = state.neighbors_offsets[fi] as usize;
            let ne = (state.neighbors_offsets[fi + 1] as usize).min(state.neighbors_flat.len());
            for ni in ns..ne {
                let nf = state.neighbors_flat[ni];
                let Some(other_group) = by_face.get(&nf) else {
                    continue;
                };
                let Some(&(eb, hunger_b, tribe_b, coop_b, agg_b)) = other_group.first() else {
                    continue;
                };
                if ea.id >= eb.id {
                    continue;
                }

                let gate = hash01(
                    ea.id ^ eb.id.wrapping_mul(3),
                    f ^ nf ^ (elapsed as u32).wrapping_mul(17),
                );
                if gate >= tuning.interaction_chance / 3.0 {
                    continue;
                }

                if tribe_a == tribe_b {
                    let (giver, receiver, g_hunger, r_hunger, g_coop) = if hunger_a >= hunger_b {
                        (ea, eb, hunger_a, hunger_b, coop_a)
                    } else {
                        (eb, ea, hunger_b, hunger_a, coop_b)
                    };
                    if g_hunger - r_hunger > 10.0 && g_hunger > 30.0 && g_coop > 0.05 {
                        let amount = (tuning.cooperation_transfer_rate * g_coop)
                            .min((g_hunger - r_hunger) * 0.5);
                        if amount > 0.0 {
                            *deltas.entry(giver).or_insert(0.0) -=
                                amount * tuning.cooperation_giver_cost_frac;
                            *deltas.entry(receiver).or_insert(0.0) += amount;
                            cooperation_events += 1;
                        }
                    }
                } else {
                    let (attacker, victim, a_agg, v_hunger) = if agg_a >= agg_b {
                        (ea, eb, agg_a, hunger_b)
                    } else {
                        (eb, ea, agg_b, hunger_a)
                    };
                    let attack_roll = hash01(
                        attacker.id.wrapping_mul(7) ^ victim.id,
                        f ^ nf ^ (elapsed as u32).wrapping_mul(23) ^ 0x2f1d,
                    );
                    if a_agg > 0.15 && attack_roll < a_agg && v_hunger > 0.0 {
                        let steal = tuning.aggression_steal_rate.min(v_hunger);
                        if steal * tuning.aggression_yield > tuning.aggression_energy_cost {
                            *deltas.entry(attacker).or_insert(0.0) +=
                                steal * tuning.aggression_yield - tuning.aggression_energy_cost;
                            *deltas.entry(victim).or_insert(0.0) -= steal;
                            aggression_events += 1;
                        }
                    }
                }
            }
        }

        state.cooperation_events += cooperation_events;
        state.aggression_events += aggression_events;

        if !deltas.is_empty() {
            q_settlers.for_each(|(ent, settler)| {
                if let Some(&d) = deltas.get(&ent) {
                    settler.hunger = (settler.hunger + d).clamp(0.0, 100.0);
                }
            });
        }
    });
}

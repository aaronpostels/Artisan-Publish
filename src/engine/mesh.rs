use wasm_bindgen::prelude::*;
use js_sys::{Float32Array, Uint32Array, Object, Reflect};

pub fn recalculate_normals_raw(vertices: &mut [f32], indices: &[u32]) {
    let num_verts = vertices.len() / 12;
    for idx in 0..num_verts {
        let offset = idx * 12;
        vertices[offset + 3] = 0.0;
        vertices[offset + 4] = 0.0;
        vertices[offset + 5] = 0.0;
    }

    let num_tris = indices.len() / 3;
    for t in 0..num_tris {
        let i0 = indices[t * 3] as usize;
        let i1 = indices[t * 3 + 1] as usize;
        let i2 = indices[t * 3 + 2] as usize;

        let o0 = i0 * 12;
        let o1 = i1 * 12;
        let o2 = i2 * 12;

        let ax = vertices[o1] - vertices[o0];
        let ay = vertices[o1 + 1] - vertices[o0 + 1];
        let az = vertices[o1 + 2] - vertices[o0 + 2];

        let bx = vertices[o2] - vertices[o0];
        let by = vertices[o2 + 1] - vertices[o0 + 1];
        let bz = vertices[o2 + 2] - vertices[o0 + 2];

        let nx = ay * bz - az * by;
        let ny = az * bx - ax * bz;
        let nz = ax * by - ay * bx;

        vertices[o0 + 3] += nx;
        vertices[o0 + 4] += ny;
        vertices[o0 + 5] += nz;

        vertices[o1 + 3] += nx;
        vertices[o1 + 4] += ny;
        vertices[o1 + 5] += nz;

        vertices[o2 + 3] += nx;
        vertices[o2 + 4] += ny;
        vertices[o2 + 5] += nz;
    }

    for idx in 0..num_verts {
        let offset = idx * 12;
        let nx = vertices[offset + 3];
        let ny = vertices[offset + 4];
        let nz = vertices[offset + 5];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len > 1e-6 {
            vertices[offset + 3] = nx / len;
            vertices[offset + 4] = ny / len;
            vertices[offset + 5] = nz / len;
        }
    }
}

#[wasm_bindgen]
pub fn recalculate_normals(vertices: &mut [f32], indices: &[u32]) {
    recalculate_normals_raw(vertices, indices);
}

pub fn mesh_sphere_native(radius: f32, rings: u32, sectors: u32) -> (Vec<f32>, Vec<u32>) {
    let mut v: Vec<f32> = Vec::new();
    let mut i: Vec<u32> = Vec::new();
    let r_recip = 1.0 / (rings as f32);
    let s_recip = 1.0 / (sectors as f32);

    for r in 0..=rings {
        let ry = r as f32 * r_recip;
        let theta = ry * std::f32::consts::PI;
        let (sin_theta, cos_theta) = theta.sin_cos();
        for s in 0..=sectors {
            let sx = s as f32 * s_recip;
            let phi = sx * std::f32::consts::PI * 2.0;
            let (sin_phi, cos_phi) = phi.sin_cos();
            let nx = cos_phi * sin_theta;
            let ny = cos_theta;
            let nz = sin_phi * sin_theta;
            v.extend_from_slice(&[
                nx * radius, ny * radius, nz * radius,
                nx, ny, nz,
                sx, ry,
                1.0, 1.0, 1.0, 1.0
            ]);
        }
    }

    for r in 0..rings {
        for s in 0..sectors {
            let current = (r * (sectors + 1) + s) as u32;
            let next = current + sectors as u32 + 1;
            i.extend_from_slice(&[current, current + 1, next, current + 1, next + 1, next]);
        }
    }

    (v, i)
}

pub fn mesh_icosphere_native(radius: f32, subdivisions: u32, flat_shaded: bool) -> (Vec<f32>, Vec<u32>) {
    let t = (1.0 + 5.0f32.sqrt()) / 2.0;

    let mut verts = vec![
        glam::Vec3::new(-1.0, t, 0.0).normalize(),
        glam::Vec3::new(1.0, t, 0.0).normalize(),
        glam::Vec3::new(-1.0, -t, 0.0).normalize(),
        glam::Vec3::new(1.0, -t, 0.0).normalize(),
        glam::Vec3::new(0.0, -1.0, t).normalize(),
        glam::Vec3::new(0.0, 1.0, t).normalize(),
        glam::Vec3::new(0.0, -1.0, -t).normalize(),
        glam::Vec3::new(0.0, 1.0, -t).normalize(),
        glam::Vec3::new(t, 0.0, -1.0).normalize(),
        glam::Vec3::new(t, 0.0, 1.0).normalize(),
        glam::Vec3::new(-t, 0.0, -1.0).normalize(),
        glam::Vec3::new(-t, 0.0, 1.0).normalize(),
    ];

    let mut faces = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
        [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
        [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];

    let mut midpoint_cache = std::collections::HashMap::new();

    for _ in 0..subdivisions {
        let mut new_faces = Vec::with_capacity(faces.len() * 4);
        for &[v0, v1, v2] in &faces {
            let get_midpoint = |a: usize, b: usize, verts: &mut Vec<glam::Vec3>, cache: &mut std::collections::HashMap<(usize, usize), usize>| -> usize {
                let key = if a < b { (a, b) } else { (b, a) };
                if let Some(&idx) = cache.get(&key) {
                    idx
                } else {
                    let p = (verts[a] + verts[b]) * 0.5;
                    verts.push(p.normalize());
                    let idx = verts.len() - 1;
                    cache.insert(key, idx);
                    idx
                }
            };

            let m01 = get_midpoint(v0, v1, &mut verts, &mut midpoint_cache);
            let m12 = get_midpoint(v1, v2, &mut verts, &mut midpoint_cache);
            let m20 = get_midpoint(v2, v0, &mut verts, &mut midpoint_cache);

            new_faces.push([v0, m01, m20]);
            new_faces.push([v1, m12, m01]);
            new_faces.push([v2, m20, m12]);
            new_faces.push([m01, m12, m20]);
        }
        faces = new_faces;
    }

    if flat_shaded {
        let mut flat_v = Vec::with_capacity(faces.len() * 3 * 12);
        let mut flat_i = Vec::with_capacity(faces.len() * 3);
        let mut v_idx = 0;
        for &[v0, v1, v2] in &faces {
            let p0 = verts[v0] * radius;
            let p1 = verts[v1] * radius;
            let p2 = verts[v2] * radius;

            let d0 = p1 - p0;
            let d1 = p2 - p0;
            let n = d0.cross(d1).normalize();

            let uvs = [
                [0.0, 0.0],
                [1.0, 0.0],
                [0.5, 1.0],
            ];

            for (idx, p) in [p0, p1, p2].iter().enumerate() {
                flat_v.extend_from_slice(&[
                    p.x, p.y, p.z,
                    n.x, n.y, n.z,
                    uvs[idx][0], uvs[idx][1],
                    1.0, 1.0, 1.0, 1.0
                ]);
            }

            flat_i.push(v_idx);
            flat_i.push(v_idx + 1);
            flat_i.push(v_idx + 2);
            v_idx += 3;
        }
        (flat_v, flat_i)
    } else {
        let mut out_v = Vec::with_capacity(verts.len() * 12);
        let mut out_i = Vec::with_capacity(faces.len() * 3);
        for p in &verts {
            let p_scaled = *p * radius;
            out_v.extend_from_slice(&[
                p_scaled.x, p_scaled.y, p_scaled.z,
                p.x, p.y, p.z,
                0.0, 0.0,
                1.0, 1.0, 1.0, 1.0
            ]);
        }
        for &[v0, v1, v2] in &faces {
            out_i.push(v0 as u32);
            out_i.push(v1 as u32);
            out_i.push(v2 as u32);
        }
        (out_v, out_i)
    }
}

#[wasm_bindgen]
pub fn mesh_icosphere(radius: f32, subdivisions: u32, flat_shaded: bool) -> Object {
    let (v, i) = mesh_icosphere_native(radius, subdivisions, flat_shaded);
    let obj = Object::new();
    Reflect::set(&obj, &"vertices".into(), &Float32Array::from(v.as_slice())).unwrap();
    Reflect::set(&obj, &"indices".into(), &Uint32Array::from(i.as_slice())).unwrap();
    Reflect::set(&obj, &"aabb_min".into(), &Float32Array::from([-radius, -radius, -radius].as_slice())).unwrap();
    Reflect::set(&obj, &"aabb_max".into(), &Float32Array::from([radius, radius, radius].as_slice())).unwrap();
    obj
}

#[wasm_bindgen]
pub fn mesh_cube(size: f32) -> Object {
    let s = size * 0.5;
    let mut v: Vec<f32> = Vec::with_capacity(24 * 12);
    let mut i: Vec<u32> = Vec::with_capacity(36);

    let faces = [
        ([-s,-s, s], [ s,-s, s], [ s, s, s], [-s, s, s], [ 0., 0., 1.]),
        ([ s,-s,-s], [-s,-s,-s], [-s, s,-s], [ s, s,-s], [ 0., 0.,-1.]),
        ([ s,-s, s], [ s,-s,-s], [ s, s,-s], [ s, s, s], [ 1., 0., 0.]),
        ([-s,-s,-s], [-s,-s, s], [-s, s, s], [-s, s,-s], [-1., 0., 0.]),
        ([-s, s, s], [ s, s, s], [ s, s,-s], [-s, s,-s], [ 0., 1., 0.]),
        ([-s,-s,-s], [ s,-s,-s], [ s,-s, s], [-s,-s, s], [ 0.,-1., 0.]),
    ];

    let mut offset = 0;
    for (p0, p1, p2, p3, n) in faces {
        v.extend_from_slice(&[p0[0], p0[1], p0[2], n[0], n[1], n[2], 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
        v.extend_from_slice(&[p1[0], p1[1], p1[2], n[0], n[1], n[2], 1.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
        v.extend_from_slice(&[p2[0], p2[1], p2[2], n[0], n[1], n[2], 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
        v.extend_from_slice(&[p3[0], p3[1], p3[2], n[0], n[1], n[2], 0.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
        i.extend_from_slice(&[offset, offset + 1, offset + 2, offset + 2, offset + 3, offset]);
        offset += 4;
    }

    let obj = Object::new();
    Reflect::set(&obj, &"vertices".into(), &Float32Array::from(v.as_slice())).unwrap();
    Reflect::set(&obj, &"indices".into(), &Uint32Array::from(i.as_slice())).unwrap();
    Reflect::set(&obj, &"aabb_min".into(), &Float32Array::from([-s, -s, -s].as_slice())).unwrap();
    Reflect::set(&obj, &"aabb_max".into(), &Float32Array::from([s, s, s].as_slice())).unwrap();
    obj
}

#[wasm_bindgen]
pub fn mesh_plane(size: f32) -> Object {
    let s = size * 0.5;
    let v: Vec<f32> = vec![
        -s, 0.0,  s,  0.0, 1.0, 0.0,  0.0, 0.0,  1.0, 1.0, 1.0, 1.0,
         s, 0.0,  s,  0.0, 1.0, 0.0,  1.0, 0.0,  1.0, 1.0, 1.0, 1.0,
         s, 0.0, -s,  0.0, 1.0, 0.0,  1.0, 1.0,  1.0, 1.0, 1.0, 1.0,
        -s, 0.0, -s,  0.0, 1.0, 0.0,  0.0, 1.0,  1.0, 1.0, 1.0, 1.0,
    ];
    let i: Vec<u32> = vec![0, 1, 2, 2, 3, 0];
    let obj = Object::new();
    Reflect::set(&obj, &"vertices".into(), &Float32Array::from(v.as_slice())).unwrap();
    Reflect::set(&obj, &"indices".into(), &Uint32Array::from(i.as_slice())).unwrap();
    Reflect::set(&obj, &"aabb_min".into(), &Float32Array::from([-s, 0.0, -s].as_slice())).unwrap();
    Reflect::set(&obj, &"aabb_max".into(), &Float32Array::from([s, 0.0, s].as_slice())).unwrap();
    obj
}

#[wasm_bindgen]
pub fn mesh_sphere(radius: f32, rings: u32, sectors: u32) -> Object {
    let (v, i) = mesh_sphere_native(radius, rings, sectors);
    let obj = Object::new();
    Reflect::set(&obj, &"vertices".into(), &Float32Array::from(v.as_slice())).unwrap();
    Reflect::set(&obj, &"indices".into(), &Uint32Array::from(i.as_slice())).unwrap();
    Reflect::set(&obj, &"aabb_min".into(), &Float32Array::from([-radius, -radius, -radius].as_slice())).unwrap();
    Reflect::set(&obj, &"aabb_max".into(), &Float32Array::from([radius, radius, radius].as_slice())).unwrap();
    obj
}

pub fn mesh_cylinder_native(radius_top: f32, radius_bottom: f32, height: f32, radial_segments: u32) -> (Vec<f32>, Vec<u32>) {
    let mut v = Vec::new();
    let mut i = Vec::new();
    let half_h = height * 0.5;

    for r in 0..=1 {
        let y = if r == 0 { half_h } else { -half_h };
        let radius = if r == 0 { radius_top } else { radius_bottom };
        for s in 0..=radial_segments {
            let theta = (s as f32 / radial_segments as f32) * std::f32::consts::PI * 2.0;
            let (sin_t, cos_t) = theta.sin_cos();
            let px = cos_t * radius;
            let pz = sin_t * radius;

            let mut nx = cos_t;
            let mut nz = sin_t;
            let ny = (radius_bottom - radius_top) / height;
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            if len > 1e-6 {
                nx /= len;
                nz /= len;
            }

            v.extend_from_slice(&[
                px, y, pz,
                nx, ny, nz,
                s as f32 / radial_segments as f32, r as f32,
                1.0, 1.0, 1.0, 1.0
            ]);
        }
    }

    for s in 0..radial_segments {
        let current = s as u32;
        let next = current + 1;
        let current_bottom = current + radial_segments as u32 + 1;
        let next_bottom = next + radial_segments as u32 + 1;

        i.extend_from_slice(&[current, next_bottom, next, current, current_bottom, next_bottom]);
    }

    let cap_base_top = v.len() / 12;
    v.extend_from_slice(&[0.0, half_h, 0.0, 0.0, 1.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0]);
    for s in 0..radial_segments {
        let theta = (s as f32 / radial_segments as f32) * std::f32::consts::PI * 2.0;
        let (sin_t, cos_t) = theta.sin_cos();
        v.extend_from_slice(&[
            cos_t * radius_top, half_h, sin_t * radius_top,
            0.0, 1.0, 0.0,
            0.5 + cos_t * 0.5, 0.5 + sin_t * 0.5,
            1.0, 1.0, 1.0, 1.0
        ]);
    }
    for s in 0..radial_segments {
        let s_usize = s as usize;
        let radial_segments_usize = radial_segments as usize;
        let curr = (cap_base_top + 1 + s_usize) as u32;
        let next = (cap_base_top + 1 + ((s_usize + 1) % radial_segments_usize)) as u32;
        i.extend_from_slice(&[cap_base_top as u32, curr, next]);
    }

    let cap_base_bottom = v.len() / 12;
    v.extend_from_slice(&[0.0, -half_h, 0.0, 0.0, -1.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0]);
    for s in 0..radial_segments {
        let theta = (s as f32 / radial_segments as f32) * std::f32::consts::PI * 2.0;
        let (sin_t, cos_t) = theta.sin_cos();
        v.extend_from_slice(&[
            cos_t * radius_bottom, -half_h, sin_t * radius_bottom,
            0.0, -1.0, 0.0,
            0.5 + cos_t * 0.5, 0.5 + sin_t * 0.5,
            1.0, 1.0, 1.0, 1.0
        ]);
    }
    for s in 0..radial_segments {
        let s_usize = s as usize;
        let radial_segments_usize = radial_segments as usize;
        let curr = (cap_base_bottom + 1 + s_usize) as u32;
        let next = (cap_base_bottom + 1 + ((s_usize + 1) % radial_segments_usize)) as u32;
        i.extend_from_slice(&[cap_base_bottom as u32, next, curr]);
    }

    (v, i)
}

#[wasm_bindgen]
pub fn mesh_cylinder(radius_top: f32, radius_bottom: f32, height: f32, radial_segments: u32) -> Object {
    let (v, i) = mesh_cylinder_native(radius_top, radius_bottom, height, radial_segments);
    let obj = Object::new();
    Reflect::set(&obj, &"vertices".into(), &Float32Array::from(v.as_slice())).unwrap();
    Reflect::set(&obj, &"indices".into(), &Uint32Array::from(i.as_slice())).unwrap();
    let max_radius = radius_top.max(radius_bottom);
    Reflect::set(&obj, &"aabb_min".into(), &Float32Array::from([-max_radius, -height * 0.5, -max_radius].as_slice())).unwrap();
    Reflect::set(&obj, &"aabb_max".into(), &Float32Array::from([max_radius, height * 0.5, max_radius].as_slice())).unwrap();
    obj
}

pub fn build_face_adjacency_native(indices: &[u32]) -> Vec<Vec<u32>> {
    let num_faces = indices.len() / 3;
    let mut neighbors = vec![Vec::new(); num_faces];
    let mut edge_map = std::collections::HashMap::with_capacity(indices.len());
    for f in 0..num_faces {
        let i0 = indices[f * 3] as u32;
        let i1 = indices[f * 3 + 1] as u32;
        let i2 = indices[f * 3 + 2] as u32;
        let edges = [
            if i0 < i1 { (i0, i1) } else { (i1, i0) },
            if i1 < i2 { (i1, i2) } else { (i2, i1) },
            if i2 < i0 { (i2, i0) } else { (i0, i2) },
        ];
        for edge in edges {
            edge_map.entry(edge).or_insert_with(Vec::new).push(f as u32);
        }
    }
    for faces in edge_map.values() {
        if faces.len() == 2 {
            neighbors[faces[0] as usize].push(faces[1]);
            neighbors[faces[1] as usize].push(faces[0]);
        }
    }
    neighbors
}

#[wasm_bindgen]
pub fn mesh_quad_2d(width: f32, height: f32) -> Object {
    let hw = width * 0.5;
    let hh = height * 0.5;
    let v: Vec<f32> = vec![
        -hw, -hh, 0.0, 0.0, 1.0,
         hw, -hh, 0.0, 1.0, 1.0,
        -hw,  hh, 0.0, 0.0, 0.0,
         hw,  hh, 0.0, 1.0, 0.0,
    ];
    let i: Vec<u32> = vec![0, 1, 2, 2, 1, 3];
    let obj = Object::new();
    Reflect::set(&obj, &"vertices".into(), &Float32Array::from(v.as_slice())).unwrap();
    Reflect::set(&obj, &"indices".into(), &Uint32Array::from(i.as_slice())).unwrap();
    obj
}

#[wasm_bindgen]
pub fn mesh_circle_2d(segments: u32) -> Object {
    let mut v: Vec<f32> = Vec::with_capacity((segments as usize + 1) * 5);
    let mut i: Vec<u32> = Vec::with_capacity(segments as usize * 3);

    v.extend_from_slice(&[0.0, 0.0, 0.0, 0.5, 0.5]);

    for s in 0..segments {
        let theta = (s as f32 / segments as f32) * std::f32::consts::PI * 2.0;
        let (sin_t, cos_t) = theta.sin_cos();
        v.extend_from_slice(&[cos_t * 0.5, sin_t * 0.5, 0.0, cos_t * 0.5 + 0.5, 0.5 - sin_t * 0.5]);

        let curr = s + 1;
        let next = if s == segments - 1 { 1 } else { s + 2 };
        i.extend_from_slice(&[0, curr, next]);
    }

    let obj = Object::new();
    Reflect::set(&obj, &"vertices".into(), &Float32Array::from(v.as_slice())).unwrap();
    Reflect::set(&obj, &"indices".into(), &Uint32Array::from(i.as_slice())).unwrap();
    obj
}

#[wasm_bindgen]
pub fn mesh_ring_2d(inner_radius: f32, outer_radius: f32, segments: u32) -> Object {
    let mut v = Vec::with_capacity((segments as usize) * 10);
    let mut i = Vec::with_capacity((segments as usize) * 6);

    for s in 0..segments {
        let theta = (s as f32 / segments as f32) * std::f32::consts::PI * 2.0;
        let (sin_t, cos_t) = theta.sin_cos();

        v.extend_from_slice(&[cos_t * inner_radius, sin_t * inner_radius, 0.0, cos_t * 0.5 + 0.5, 0.5 - sin_t * 0.5]);

        v.extend_from_slice(&[cos_t * outer_radius, sin_t * outer_radius, 0.0, cos_t * 0.5 + 0.5, 0.5 - sin_t * 0.5]);

        let curr_inner = s * 2;
        let curr_outer = s * 2 + 1;
        let next_inner = if s == segments - 1 { 0 } else { (s + 1) * 2 };
        let next_outer = if s == segments - 1 { 1 } else { (s + 1) * 2 + 1 };

        i.extend_from_slice(&[curr_inner, curr_outer, next_outer, curr_inner, next_outer, next_inner]);
    }

    let obj = Object::new();
    Reflect::set(&obj, &"vertices".into(), &Float32Array::from(v.as_slice())).unwrap();
    Reflect::set(&obj, &"indices".into(), &Uint32Array::from(i.as_slice())).unwrap();
    obj
}

#[wasm_bindgen]
pub fn mesh_capsule_2d(width: f32, height: f32, segments: u32) -> Object {
    let radius = width * 0.5;
    let half_straight = (height - width).max(0.0) * 0.5;
    let mut v = Vec::new();
    let mut i = Vec::new();

    v.extend_from_slice(&[0.0, half_straight, 0.0, 0.5, 0.5 - half_straight / height]);
    let top_center_idx = 0;

    let half_segs = segments / 2;

    for s in 0..=half_segs {
        let theta = (s as f32 / half_segs as f32) * std::f32::consts::PI;
        let (sin_t, cos_t) = theta.sin_cos();
        let px = cos_t * radius;
        let py = half_straight + sin_t * radius;
        v.extend_from_slice(&[px, py, 0.0, px / width + 0.5, 0.5 - py / height]);

        if s > 0 {
            let curr = 1 + s;
            let prev = curr - 1;
            i.extend_from_slice(&[top_center_idx, prev, curr]);
        }
    }

    let bottom_center_idx = v.len() as u32 / 5;
    v.extend_from_slice(&[0.0, -half_straight, 0.0, 0.5, 0.5 + half_straight / height]);

    let bottom_start_idx = v.len() as u32 / 5;
    for s in 0..=half_segs {
        let theta = std::f32::consts::PI + (s as f32 / half_segs as f32) * std::f32::consts::PI;
        let (sin_t, cos_t) = theta.sin_cos();
        let px = cos_t * radius;
        let py = -half_straight + sin_t * radius;
        v.extend_from_slice(&[px, py, 0.0, px / width + 0.5, 0.5 - py / height]);

        if s > 0 {
            let curr = bottom_start_idx + s;
            let prev = curr - 1;
            i.extend_from_slice(&[bottom_center_idx, prev, curr]);
        }
    }

    let tr = 1;
    let tl = 1 + half_segs;
    let bl = bottom_start_idx;
    let br = bottom_start_idx + half_segs;

    i.extend_from_slice(&[tr, tl, bl, bl, br, tr]);

    let obj = Object::new();
    Reflect::set(&obj, &"vertices".into(), &Float32Array::from(v.as_slice())).unwrap();
    Reflect::set(&obj, &"indices".into(), &Uint32Array::from(i.as_slice())).unwrap();
    obj
}

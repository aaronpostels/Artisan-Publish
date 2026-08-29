use glam::{Vec3, Mat4};

const GRAD3: [[f32; 3]; 12] = [
    [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0], [1.0, -1.0, 0.0], [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0], [-1.0, 0.0, 1.0], [1.0, 0.0, -1.0], [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0], [0.0, -1.0, 1.0], [0.0, 1.0, -1.0], [0.0, -1.0, -1.0]
];

#[derive(Clone, Copy, Debug)]
pub struct SimplexNoise {
    pub perm: [u8; 512],
    pub perm_mod12: [u8; 512],
}

impl SimplexNoise {
    pub fn new(seed: u32) -> Self {
        let mut lcg = seed;
        let mut next_random = move || -> f32 {
            lcg = lcg.wrapping_add(0x6d2b79f5);
            let mut t = lcg;
            t = (t ^ (t >> 15)).wrapping_mul(t | 1);
            t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
            (t ^ (t >> 14)) as f32 / 4294967296.0
        };
        let mut p = [0u8; 256];
        for i in 0..256 {
            p[i] = i as u8;
        }
        for i in (1..256).rev() {
            let j = (next_random() * (i + 1) as f32).floor() as usize;
            p.swap(i, j);
        }
        let mut perm = [0u8; 512];
        let mut perm_mod12 = [0u8; 512];
        for i in 0..512 {
            perm[i] = p[i & 255];
            perm_mod12[i] = perm[i] % 12;
        }
        Self { perm, perm_mod12 }
    }

    pub fn noise_3d(&self, xin: f32, yin: f32, zin: f32) -> f32 {
        let s = (xin + yin + zin) * 0.333333333;
        let i = (xin + s).floor() as i32;
        let j = (yin + s).floor() as i32;
        let k = (zin + s).floor() as i32;
        let t = (i + j + k) as f32 * 0.166666667;
        let x0 = xin - (i as f32 - t);
        let y0 = yin - (j as f32 - t);
        let z0 = zin - (k as f32 - t);
        let (i1, j1, k1);
        let (i2, j2, k2);
        if x0 >= y0 {
            if y0 >= z0 { i1 = 1; j1 = 0; k1 = 0; i2 = 1; j2 = 1; k2 = 0; }
            else if x0 >= z0 { i1 = 1; j1 = 0; k1 = 0; i2 = 1; j2 = 0; k2 = 1; }
            else { i1 = 0; j1 = 0; k1 = 1; i2 = 1; j2 = 0; k2 = 1; }
        } else {
            if y0 < z0 { i1 = 0; j1 = 0; k1 = 1; i2 = 0; j2 = 1; k2 = 1; }
            else if x0 < z0 { i1 = 0; j1 = 1; k1 = 0; i2 = 0; j2 = 1; k2 = 1; }
            else { i1 = 0; j1 = 1; k1 = 0; i2 = 1; j2 = 1; k2 = 0; }
        }
        let x1 = x0 - i1 as f32 + 0.166666667;
        let y1 = y0 - j1 as f32 + 0.166666667;
        let z1 = z0 - k1 as f32 + 0.166666667;
        let x2 = x0 - i2 as f32 + 0.333333333;
        let y2 = y0 - j2 as f32 + 0.333333333;
        let z2 = z0 - k2 as f32 + 0.333333333;
        let x3 = x0 - 1.0 + 0.5;
        let y3 = y0 - 1.0 + 0.5;
        let z3 = z0 - 1.0 + 0.5;
        let ii = (i & 255) as usize;
        let jj = (j & 255) as usize;
        let kk = (k & 255) as usize;
        let mut n0 = 0.0;
        let t0 = 0.6 - x0 * x0 - y0 * y0 - z0 * z0;
        if t0 >= 0.0 {
            let gi0 = self.perm_mod12[ii + self.perm[jj + self.perm[kk] as usize] as usize] as usize;
            let g = GRAD3[gi0];
            n0 = t0 * t0 * t0 * t0 * (g[0] * x0 + g[1] * y0 + g[2] * z0);
        }
        let mut n1 = 0.0;
        let t1 = 0.6 - x1 * x1 - y1 * y1 - z1 * z1;
        if t1 >= 0.0 {
            let gi1 = self.perm_mod12[ii + i1 as usize + self.perm[jj + j1 as usize + self.perm[kk + k1 as usize] as usize] as usize] as usize;
            let g = GRAD3[gi1];
            n1 = t1 * t1 * t1 * t1 * (g[0] * x1 + g[1] * y1 + g[2] * z1);
        }
        let mut n2 = 0.0;
        let t2 = 0.6 - x2 * x2 - y2 * y2 - z2 * z2;
        if t2 >= 0.0 {
            let gi2 = self.perm_mod12[ii + i2 as usize + self.perm[jj + j2 as usize + self.perm[kk + k2 as usize] as usize] as usize] as usize;
            let g = GRAD3[gi2];
            n2 = t2 * t2 * t2 * t2 * (g[0] * x2 + g[1] * y2 + g[2] * z2);
        }
        let mut n3 = 0.0;
        let t3 = 0.6 - x3 * x3 - y3 * y3 - z3 * z3;
        if t3 >= 0.0 {
            let gi3 = self.perm_mod12[ii + 1 + self.perm[jj + 1 + self.perm[kk + 1] as usize] as usize] as usize;
            let g = GRAD3[gi3];
            n3 = t3 * t3 * t3 * t3 * (g[0] * x3 + g[1] * y3 + g[2] * z3);
        }
        32.0 * (n0 + n1 + n2 + n3)
    }

    pub fn fbm3d(&self, x: f32, y: f32, z: f32, octaves: usize, persistence: f32, lacunarity: f32, scale: f32) -> f32 {
        let mut total = 0.0;
        let mut frequency = scale;
        let mut amplitude = 1.0;
        let mut max_value = 0.0;
        for _ in 0..octaves {
            total += self.noise_3d(x * frequency, y * frequency, z * frequency) * amplitude;
            max_value += amplitude;
            amplitude *= persistence;
            frequency *= lacunarity;
        }
        (total / max_value) * 1.25
    }

    pub fn ridged_fbm3d(&self, x: f32, y: f32, z: f32, octaves: usize, persistence: f32, lacunarity: f32, scale: f32) -> f32 {
        let mut total = 0.0;
        let mut frequency = scale;
        let mut amplitude = 1.0;
        let mut weight = 1.0;
        let mut max_value = 0.0;
        for _ in 0..octaves {
            let v = self.noise_3d(x * frequency, y * frequency, z * frequency);
            let mut n = 1.0 - v.abs();
            n = n * n;
            n *= weight;
            weight = (n * 2.0).clamp(0.1, 1.0);
            total += n * amplitude;
            max_value += amplitude;
            amplitude *= persistence;
            frequency *= lacunarity;
        }
        (total / max_value) * 1.1
    }
}

pub fn normalize(x: f32, y: f32) -> (f32, f32) {
    let len = (x * x + y * y).sqrt();
    if len > 0.0 {
        (x / len, y / len)
    } else {
        (0.0, 0.0)
    }
}

pub fn normalize_3d(x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    let len = (x * x + y * y + z * z).sqrt();
    if len > 0.0 {
        (x / len, y / len, z / len)
    } else {
        (0.0, 0.0, 0.0)
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn distance_sq(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    dx * dx + dy * dy
}

pub fn distance(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    distance_sq(x1, y1, x2, y2).sqrt()
}

pub fn aabb_penetration(
    pos_a: (f32, f32),
    half_a: (f32, f32),
    pos_b: (f32, f32),
    half_b: (f32, f32),
) -> Option<(f32, f32)> {
    let dx = pos_a.0 - pos_b.0;
    let px = (half_a.0 + half_b.0) - dx.abs();
    if px <= 0.0 {
        return None;
    }
    let dy = pos_a.1 - pos_b.1;
    let py = (half_a.1 + half_b.1) - dy.abs();
    if py <= 0.0 {
        return None;
    }
    if px < py {
        Some((if dx > 0.0 { px } else { -px }, 0.0))
    } else {
        Some((0.0, if dy > 0.0 { py } else { -py }))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Plane {
    pub normal: Vec3,
    pub d: f32,
}

impl Plane {
    pub fn from_vector4(v: glam::Vec4) -> Self {
        let normal = Vec3::new(v.x, v.y, v.z);
        let len = normal.length();
        Self {
            normal: normal / len,
            d: v.w / len,
        }
    }

    pub fn dot_point(&self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.d
    }
}

pub struct Frustum {
    pub planes: [Plane; 6],
}

impl Frustum {
    pub fn from_matrix(m: Mat4) -> Self {
        let row4 = m.row(3);
        let row1 = m.row(0);
        let row2 = m.row(1);
        let row3 = m.row(2);
        Self {
            planes: [
                Plane::from_vector4(row4 + row1),
                Plane::from_vector4(row4 - row1),
                Plane::from_vector4(row4 + row2),
                Plane::from_vector4(row4 - row2),
                Plane::from_vector4(row4 + row3),
                Plane::from_vector4(row4 - row3),
            ],
        }
    }

    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        for plane in &self.planes {
            let mut p = min;
            if plane.normal.x >= 0.0 { p.x = max.x; }
            if plane.normal.y >= 0.0 { p.y = max.y; }
            if plane.normal.z >= 0.0 { p.z = max.z; }
            if plane.dot_point(p) < 0.0 {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
    pub inv_direction: Vec3,
}

impl Ray {
    #[inline(always)]
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self {
            origin,
            direction,
            inv_direction: Vec3::new(1.0 / direction.x, 1.0 / direction.y, 1.0 / direction.z),
        }
    }

    #[inline(always)]
    pub fn intersect_aabb(&self, min: Vec3, max: Vec3) -> f32 {
        let t1 = (min - self.origin) * self.inv_direction;
        let t2 = (max - self.origin) * self.inv_direction;
        let tmin = t1.min(t2);
        let tmax = t1.max(t2);
        let tnear = tmin.max_element();
        let tfar = tmax.min_element();
        if tfar >= tnear && tfar >= 0.0 {
            tnear.max(0.0)
        } else {
            f32::MAX
        }
    }

    #[inline(always)]
    pub fn intersect_triangle(&self, v0: Vec3, v1: Vec3, v2: Vec3) -> f32 {
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let h = self.direction.cross(edge2);
        let a = edge1.dot(h);
        if a > -1e-6 && a < 1e-6 { return f32::MAX; }
        let f = 1.0 / a;
        let s = self.origin - v0;
        let u = f * s.dot(h);
        if !(0.0..=1.0).contains(&u) { return f32::MAX; }
        let q = s.cross(edge1);
        let v = f * self.direction.dot(q);
        if v < 0.0 || u + v > 1.0 { return f32::MAX; }
        let t = f * edge2.dot(q);
        if t > 1e-6 { t } else { f32::MAX }
    }
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 == edge1 {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 17;
    x = x.wrapping_mul(0xed5ad4bb);
    x ^= x >> 11;
    x = x.wrapping_mul(0xac4c1b51);
    x ^= x >> 15;
    x = x.wrapping_mul(0x31848bab);
    x ^= x >> 14;
    x
}

pub fn hash3d_int(x: i32, y: i32, z: i32) -> f32 {
    let u = (x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663) ^ (z as u32).wrapping_mul(83492791);
    (hash_u32(u) & 0xffffff) as f32 / 16777216.0
}

pub fn noise3d_int(x: f32, y: f32, z: f32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let iz = z.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let fz = z - z.floor();
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let uz = fz * fz * (3.0 - 2.0 * fz);
    let n000 = hash3d_int(ix, iy, iz);
    let n100 = hash3d_int(ix + 1, iy, iz);
    let n010 = hash3d_int(ix, iy + 1, iz);
    let n110 = hash3d_int(ix + 1, iy + 1, iz);
    let n001 = hash3d_int(ix, iy, iz + 1);
    let n101 = hash3d_int(ix + 1, iy, iz + 1);
    let n011 = hash3d_int(ix, iy + 1, iz + 1);
    let n111 = hash3d_int(ix + 1, iy + 1, iz + 1);
    let n0 = n000 + ux * (n100 - n000);
    let n1 = n010 + ux * (n110 - n010);
    let n2 = n001 + ux * (n101 - n001);
    let n3 = n011 + ux * (n111 - n011);
    let n_y0 = n0 + uy * (n1 - n0);
    let n_y1 = n2 + uy * (n3 - n2);
    (n_y0 + uz * (n_y1 - n_y0)) * 2.0 - 1.0
}

pub fn fbm3d_int(x: f32, y: f32, z: f32, octaves: usize, persistence: f32, lacunarity: f32, scale: f32) -> f32 {
    let mut total = 0.0;
    let mut frequency = scale;
    let mut amplitude = 1.0;
    let mut max_value = 0.0;
    for _ in 0..octaves {
        total += noise3d_int(x * frequency, y * frequency, z * frequency) * amplitude;
        max_value += amplitude;
        amplitude *= persistence;
        frequency *= lacunarity;
    }
    total / max_value
}

pub fn ridged_fbm3d_int(x: f32, y: f32, z: f32, octaves: usize, persistence: f32, lacunarity: f32, scale: f32) -> f32 {
    let mut total = 0.0;
    let mut frequency = scale;
    let mut amplitude = 1.0;
    let mut weight = 1.0;
    let mut max_value = 0.0;
    for _ in 0..octaves {
        let v = noise3d_int(x * frequency, y * frequency, z * frequency);
        let mut n = 1.0 - v.abs();
        n = n * n;
        n *= weight;
        weight = (n * 2.0).clamp(0.1, 1.0);
        total += n * amplitude;
        max_value += amplitude;
        amplitude *= persistence;
        frequency *= lacunarity;
    }
    total / max_value
}

pub fn bilinear_interpolate_f32(c00: f32, c10: f32, c01: f32, c11: f32, tx: f32, ty: f32) -> f32 {
    let top = lerp(c00, c10, tx);
    let bottom = lerp(c01, c11, tx);
    lerp(top, bottom, ty)
}

pub fn bilinear_interpolate_color(c00: [f32; 3], c10: [f32; 3], c01: [f32; 3], c11: [f32; 3], tx: f32, ty: f32) -> [f32; 3] {
    let r = bilinear_interpolate_f32(c00[0], c10[0], c01[0], c11[0], tx, ty);
    let g = bilinear_interpolate_f32(c00[1], c10[1], c01[1], c11[1], tx, ty);
    let b = bilinear_interpolate_f32(c00[2], c10[2], c01[2], c11[2], tx, ty);
    [r, g, b]
}

pub fn hex_to_linear_rgb(hex: u32) -> [f32; 3] {
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

#[derive(Clone, Debug)]
pub struct SeededRng {
    pub seed: u32,
}

impl SeededRng {
    pub fn new(seed: u32) -> Self {
        Self { seed }
    }

    pub fn next_f32(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.seed as f32) / (u32::MAX as f32)
    }
}

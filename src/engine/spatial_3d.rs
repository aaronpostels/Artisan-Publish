use crate::ecs::{Entity, Query, World};
use crate::engine::spatial::FastMap;
use crate::engine::component::{DynamicMesh, MeshBVH, GlobalTransform};
use crate::engine::math::Ray;
use glam::Vec3;

pub struct SpatialGrid3D {
    pub cell_size: f32,
    pub cells: FastMap<u64, Vec<Entity>>,
    pub pool: Vec<Vec<Entity>>,
}

impl Default for SpatialGrid3D {
    fn default() -> Self {
        Self::new(10.0)
    }
}

impl SpatialGrid3D {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size,
            cells: FastMap::default(),
            pool: Vec::new(),
        }
    }

    #[inline(always)]
    fn pack(x: i32, y: i32, z: i32) -> u64 {
        let ux = (x as u32 as u64) & 0x1FFFFF;
        let uy = (y as u32 as u64) & 0x1FFFFF;
        let uz = (z as u32 as u64) & 0x3FFFFF;
        (ux << 43) | (uy << 22) | uz
    }

    pub fn clear(&mut self) {
        for (_, mut vec) in self.cells.drain() {
            vec.clear();
            self.pool.push(vec);
        }
    }

    pub fn insert(&mut self, x: f32, y: f32, z: f32, entity: Entity) {
        let gx = (x / self.cell_size).floor() as i32;
        let gy = (y / self.cell_size).floor() as i32;
        let gz = (z / self.cell_size).floor() as i32;
        let key = Self::pack(gx, gy, gz);
        self.cells.entry(key)
            .or_insert_with(|| self.pool.pop().unwrap_or_default())
            .push(entity);
    }

    pub fn query_radius(&self, x: f32, y: f32, z: f32, radius: f32, results: &mut Vec<Entity>) {
        results.clear();
        let min_x = ((x - radius) / self.cell_size).floor() as i32;
        let max_x = ((x + radius) / self.cell_size).floor() as i32;
        let min_y = ((y - radius) / self.cell_size).floor() as i32;
        let max_y = ((y + radius) / self.cell_size).floor() as i32;
        let min_z = ((z - radius) / self.cell_size).floor() as i32;
        let max_z = ((z + radius) / self.cell_size).floor() as i32;

        for gx in min_x..=max_x {
            for gy in min_y..=max_y {
                for gz in min_z..=max_z {
                    if let Some(entities) = self.cells.get(&Self::pack(gx, gy, gz)) {
                        results.extend_from_slice(entities);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct BVHNode {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub left_first: u32,
    pub tri_count: u32,
}

pub fn sys_build_bvh(mut q: Query<(&DynamicMesh, &mut MeshBVH)>) {
    q.par_for_each(|(mesh, bvh)| {
        if bvh.version != mesh.version && !mesh.indices.is_empty() {
            build_bvh(mesh, bvh);
        }
    });
}

pub fn build_bvh(mesh: &DynamicMesh, bvh: &mut MeshBVH) {
    let tri_count = (mesh.indices.len() / 3) as u32;
    if tri_count == 0 { return; }

    bvh.tri_indices.clear();
    bvh.tri_indices.extend(0..tri_count);
    bvh.nodes.clear();
    bvh.nodes.push(BVHNode::default());

    let mut centroids = Vec::with_capacity(tri_count as usize);
    for i in 0..tri_count {
        let i0 = mesh.indices[(i * 3) as usize] as usize * 12;
        let i1 = mesh.indices[(i * 3 + 1) as usize] as usize * 12;
        let i2 = mesh.indices[(i * 3 + 2) as usize] as usize * 12;
        let v0 = Vec3::new(mesh.vertices[i0], mesh.vertices[i0+1], mesh.vertices[i0+2]);
        let v1 = Vec3::new(mesh.vertices[i1], mesh.vertices[i1+1], mesh.vertices[i1+2]);
        let v2 = Vec3::new(mesh.vertices[i2], mesh.vertices[i2+1], mesh.vertices[i2+2]);
        centroids.push((v0 + v1 + v2) / 3.0);
    }

    bvh.nodes[0].tri_count = tri_count;
    update_node_bounds(0, mesh, bvh);
    subdivide(0, mesh, bvh, &centroids);
    bvh.version = mesh.version;
}

fn update_node_bounds(node_idx: usize, mesh: &DynamicMesh, bvh: &mut MeshBVH) {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    let node = bvh.nodes[node_idx];

    for i in 0..node.tri_count {
        let tri_idx = bvh.tri_indices[(node.left_first + i) as usize];
        for j in 0..3 {
            let v_idx = mesh.indices[(tri_idx * 3 + j) as usize] as usize * 12;
            let v = Vec3::new(mesh.vertices[v_idx], mesh.vertices[v_idx+1], mesh.vertices[v_idx+2]);
            min = min.min(v);
            max = max.max(v);
        }
    }
    bvh.nodes[node_idx].min = min.to_array();
    bvh.nodes[node_idx].max = max.to_array();
}

fn subdivide(node_idx: usize, mesh: &DynamicMesh, bvh: &mut MeshBVH, centroids: &[Vec3]) {
    let node = bvh.nodes[node_idx];
    if node.tri_count <= 4 { return; }

    let extent = Vec3::from_array(node.max) - Vec3::from_array(node.min);
    let mut axis = 0;
    if extent.y > extent.x { axis = 1; }
    if extent.z > extent[axis] { axis = 2; }

    let split_pos = node.min[axis] + extent[axis] * 0.5;
    let mut i = node.left_first as usize;
    let mut j = (node.left_first + node.tri_count - 1) as usize;

    while i <= j {
        let tri_idx = bvh.tri_indices[i];
        if centroids[tri_idx as usize][axis] < split_pos {
            i += 1;
        } else {
            bvh.tri_indices.swap(i, j);
            if j == 0 { break; }
            j -= 1;
        }
    }

    let left_count = (i as u32) - node.left_first;
    if left_count == 0 || left_count == node.tri_count { return; }

    let left_child_idx = bvh.nodes.len() as u32;
    bvh.nodes.push(BVHNode { left_first: node.left_first, tri_count: left_count, ..Default::default() });
    bvh.nodes.push(BVHNode { left_first: i as u32, tri_count: node.tri_count - left_count, ..Default::default() });

    bvh.nodes[node_idx].left_first = left_child_idx;
    bvh.nodes[node_idx].tri_count = 0;

    update_node_bounds(left_child_idx as usize, mesh, bvh);
    update_node_bounds((left_child_idx + 1) as usize, mesh, bvh);

    subdivide(left_child_idx as usize, mesh, bvh, centroids);
    subdivide((left_child_idx + 1) as usize, mesh, bvh, centroids);
}

#[inline(always)]
pub fn bvh_raycast(bvh: &MeshBVH, mesh: &DynamicMesh, ray: &Ray) -> Option<(f32, u32)> {
    if bvh.nodes.is_empty() { return None; }
    let mut closest_t = f32::MAX;
    let mut closest_tri = u32::MAX;

    let mut stack = [0u32; 64];
    stack[0] = 0;
    let mut stack_ptr = 1;

    let nodes = bvh.nodes.as_ptr();
    let tri_indices = bvh.tri_indices.as_ptr();
    let vertices = mesh.vertices.as_ptr();
    let indices = mesh.indices.as_ptr();

    while stack_ptr > 0 {
        stack_ptr -= 1;
        let node_idx = stack[stack_ptr] as usize;
        let node = unsafe { &*nodes.add(node_idx) };

        if ray.intersect_aabb(Vec3::from_array(node.min), Vec3::from_array(node.max)) >= closest_t {
            continue;
        }

        if node.tri_count > 0 {
            for i in 0..node.tri_count {
                let tri_idx = unsafe { *tri_indices.add((node.left_first + i) as usize) };
                let idx_base = (tri_idx * 3) as usize;

                let i0 = unsafe { *indices.add(idx_base) } as usize * 12;
                let i1 = unsafe { *indices.add(idx_base + 1) } as usize * 12;
                let i2 = unsafe { *indices.add(idx_base + 2) } as usize * 12;

                let v0 = unsafe { Vec3::new(*vertices.add(i0), *vertices.add(i0+1), *vertices.add(i0+2)) };
                let v1 = unsafe { Vec3::new(*vertices.add(i1), *vertices.add(i1+1), *vertices.add(i1+2)) };
                let v2 = unsafe { Vec3::new(*vertices.add(i2), *vertices.add(i2+1), *vertices.add(i2+2)) };

                let t = ray.intersect_triangle(v0, v1, v2);
                if t < closest_t {
                    closest_t = t;
                    closest_tri = tri_idx;
                }
            }
        } else {
            let left = node.left_first;
            let right = left + 1;

            stack[stack_ptr] = right; stack_ptr += 1;
            stack[stack_ptr] = left; stack_ptr += 1;
        }
    }

    if closest_t < f32::MAX { Some((closest_t, closest_tri)) } else { None }
}

#[inline(always)]
pub fn world_raycast(world: &World, ray: &Ray) -> Option<(Entity, f32, u32)> {
    let gt_id = match world.get_component_id::<GlobalTransform>() { Some(id) => id, None => return None };
    let mesh_id = match world.get_component_id::<DynamicMesh>() { Some(id) => id, None => return None };
    let bvh_id = match world.get_component_id::<MeshBVH>() { Some(id) => id, None => return None };

    let mut closest_t = f32::MAX;
    let mut closest_hit = None;

    for arch in &world.archetypes {
        if arch.entities.is_empty() { continue; }
        if gt_id >= arch.component_to_column.len() || arch.component_to_column[gt_id] == u32::MAX { continue; }
        if mesh_id >= arch.component_to_column.len() || arch.component_to_column[mesh_id] == u32::MAX { continue; }
        if bvh_id >= arch.component_to_column.len() || arch.component_to_column[bvh_id] == u32::MAX { continue; }

        let gt_col = unsafe { &*arch.columns[arch.component_to_column[gt_id] as usize].get() };
        let mesh_col = unsafe { &*arch.columns[arch.component_to_column[mesh_id] as usize].get() };
        let bvh_col = unsafe { &*arch.columns[arch.component_to_column[bvh_id] as usize].get() };

        let gts = gt_col.data.as_ptr::<GlobalTransform>();
        let meshes = mesh_col.data.as_ptr::<DynamicMesh>();
        let bvhs = bvh_col.data.as_ptr::<MeshBVH>();

        for i in 0..arch.entities.len() {
            let gt = unsafe { &*gts.add(i) };
            let mesh = unsafe { &*meshes.add(i) };
            let bvh = unsafe { &*bvhs.add(i) };

            if bvh.nodes.is_empty() { continue; }

            let inv_mat = gt.matrix.inverse();
            let local_origin = inv_mat.transform_point3(ray.origin);
            let local_dir_unnorm = inv_mat.transform_vector3(ray.direction);
            let local_dir = local_dir_unnorm.normalize();
            let scale_factor = local_dir_unnorm.length();

            let local_ray = Ray::new(local_origin, local_dir);

            if let Some((local_t, tri_idx)) = bvh_raycast(bvh, mesh, &local_ray) {
                let world_t = local_t / scale_factor;
                if world_t < closest_t {
                    closest_t = world_t;
                    closest_hit = Some((arch.entities[i], world_t, tri_idx));
                }
            }
        }
    }
    closest_hit
}

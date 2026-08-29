use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use crate::ecs::Entity;
use std::collections::BinaryHeap;

const K: u64 = 0x517cc1b727220a95;

pub struct FastHasher(u64);
impl Default for FastHasher {
    #[inline] fn default() -> Self { Self(0) }
}
impl Hasher for FastHasher {
    #[inline] fn write(&mut self, mut bytes: &[u8]) {
        while bytes.len() >= 8 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&bytes[..8]);
            self.write_u64(u64::from_ne_bytes(b));
            bytes = &bytes[8..];
        }
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ (b as u64)).wrapping_mul(K);
        }
    }
    #[inline] fn write_u64(&mut self, mut i: u64) {
        i ^= i >> 30;
        i = i.wrapping_mul(0xbf58476d1ce4e5b9);
        i ^= i >> 27;
        i = i.wrapping_mul(0x94d049bb133111eb);
        i ^= i >> 31;
        self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(K);
    }
    #[inline] fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }
    #[inline] fn write_i32(&mut self, i: i32) {
        self.write_u64(i as u64);
    }
    #[inline] fn write_u32(&mut self, i: u32) {
        self.write_u64(i as u64);
    }
    #[inline] fn finish(&self) -> u64 { self.0 }
}

pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

pub struct SpatialGrid {
    pub cell_size: f32,
    pub cells: FastMap<u64, Vec<Entity>>,
    pub pool: Vec<Vec<Entity>>,
}

impl Default for SpatialGrid {
    fn default() -> Self {
        Self::new(64.0)
    }
}

impl SpatialGrid {
    pub fn new(cell_size: f32) -> Self {
        Self { cell_size, cells: FastMap::default(), pool: Vec::new() }
    }
    #[inline(always)]
    fn pack(x: i32, y: i32) -> u64 {
        ((x as u32 as u64) << 32) | (y as u32 as u64)
    }
    pub fn clear(&mut self) {
        for (_, mut vec) in self.cells.drain() {
            vec.clear();
            self.pool.push(vec);
        }
    }
    pub fn insert(&mut self, x: f32, y: f32, entity: Entity) {
        let gx = (x / self.cell_size).floor() as i32;
        let gy = (y / self.cell_size).floor() as i32;
        let key = Self::pack(gx, gy);
        self.cells.entry(key)
            .or_insert_with(|| self.pool.pop().unwrap_or_default())
            .push(entity);
    }
    pub fn query_radius(&self, x: f32, y: f32, radius: f32, results: &mut Vec<Entity>) {
        results.clear();
        let min_x = ((x - radius) / self.cell_size).floor() as i32;
        let max_x = ((x + radius) / self.cell_size).floor() as i32;
        let min_y = ((y - radius) / self.cell_size).floor() as i32;
        let max_y = ((y + radius) / self.cell_size).floor() as i32;
        for gx in min_x..=max_x {
            for gy in min_y..=max_y {
                if let Some(entities) = self.cells.get(&Self::pack(gx, gy)) {
                    results.extend_from_slice(entities);
                }
            }
        }
    }
}

pub const TILE_GRID_LAYERS: usize = 8;

pub struct TileGrid {
    pub cell_size: f32,
    pub cells: FastMap<u64, [Entity; TILE_GRID_LAYERS]>,
}

impl Default for TileGrid {
    fn default() -> Self {
        Self::new(32.0)
    }
}

impl TileGrid {
    pub fn new(cell_size: f32) -> Self {
        Self { cell_size, cells: FastMap::default() }
    }
    #[inline(always)]
    fn pack(x: i32, y: i32) -> u64 {
        ((x as u32 as u64) << 32) | (y as u32 as u64)
    }
    #[inline(always)]
    pub fn get_entity_at_tile(&self, x: i32, y: i32, layer: u8) -> Option<Entity> {
        self.cells.get(&Self::pack(x, y)).and_then(|layers| {
            let e = layers[(layer as usize) % TILE_GRID_LAYERS];
            if e.id != u32::MAX { Some(e) } else { None }
        })
    }
    #[inline(always)]
    pub fn set_entity_at_tile(&mut self, x: i32, y: i32, layer: u8, entity: Entity) {
        let layers = self.cells.entry(Self::pack(x, y)).or_insert([crate::ecs::Entity { id: u32::MAX, generation: 0 }; TILE_GRID_LAYERS]);
        layers[(layer as usize) % TILE_GRID_LAYERS] = entity;
    }
    #[inline(always)]
    pub fn remove_entity_at_tile(&mut self, x: i32, y: i32, layer: u8) {
        if let Some(layers) = self.cells.get_mut(&Self::pack(x, y)) {
            layers[(layer as usize) % TILE_GRID_LAYERS] = crate::ecs::Entity { id: u32::MAX, generation: 0 };
        }
    }
}

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct SurfaceGraph {
    pub adjacency: Vec<Vec<u32>>,
}

impl SurfaceGraph {
    pub fn set_neighbors(&mut self, idx: u32, neighbors: Vec<u32>) {
        let idx = idx as usize;
        if idx >= self.adjacency.len() {
            self.adjacency.resize(idx + 1, Vec::new());
        }
        self.adjacency[idx] = neighbors;
    }
    pub fn get_neighbors(&self, idx: u32) -> Option<&Vec<u32>> {
        self.adjacency.get(idx as usize)
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct AStarNode {
    pos: (i32, i32),
    f_score: i32,
}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.f_score.cmp(&self.f_score)
    }
}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub fn find_path_astar(
    world: &crate::ecs::World,
    start: (i32, i32),
    end: (i32, i32),
    diagonal: bool,
    max_iterations: usize,
) -> Option<Vec<(i32, i32)>> {
    if start == end {
        return Some(vec![start]);
    }

    let is_walkable = |x: i32, y: i32| -> bool {
        if let Some(grid) = world.get_resource::<TileGrid>() {
            for layer in 0..TILE_GRID_LAYERS as u8 {
                if let Some(entity) = grid.get_entity_at_tile(x, y, layer) {
                    let obs_id = world.get_component_id_by_type_id(std::any::TypeId::of::<crate::engine::component::Obstacle>());
                    let col_id = world.get_component_id_by_type_id(std::any::TypeId::of::<crate::engine::component::Collider>());
                    let box_id = world.get_component_id_by_type_id(std::any::TypeId::of::<crate::engine::component::BoxCollider2D>());

                    let mut is_blocked = false;
                    if let Some(oid) = obs_id {
                        if world.get_component_by_id::<crate::engine::component::Obstacle>(entity, oid).is_some() {
                            is_blocked = true;
                        }
                    }
                    if !is_blocked {
                        if let Some(cid) = col_id {
                            if world.get_component_by_id::<crate::engine::component::Collider>(entity, cid).is_some() {
                                is_blocked = true;
                            }
                        }
                    }
                    if !is_blocked {
                        if let Some(bid) = box_id {
                            if world.get_component_by_id::<crate::engine::component::BoxCollider2D>(entity, bid).is_some() {
                                is_blocked = true;
                            }
                        }
                    }
                    if is_blocked { return false; }
                }
            }
        }
        true
    };

    let mut open_set = BinaryHeap::new();
    let mut came_from: FastMap<(i32, i32), (i32, i32)> = FastMap::default();
    let mut g_score: FastMap<(i32, i32), i32> = FastMap::default();

    g_score.insert(start, 0);
    open_set.push(AStarNode {
        pos: start,
        f_score: (start.0 - end.0).abs() + (start.1 - end.1).abs(),
    });

    let mut iterations = 0;

    while let Some(current_node) = open_set.pop() {
        let current = current_node.pos;
        if current == end {
            let mut path = vec![current];
            let mut curr = current;
            while let Some(&prev) = came_from.get(&curr) {
                path.push(prev);
                curr = prev;
            }
            path.reverse();
            return Some(path);
        }

        iterations += 1;
        if iterations > max_iterations {
            break;
        }

        let neighbors = if diagonal {
            vec![
                (current.0 + 1, current.1),
                (current.0 - 1, current.1),
                (current.0, current.1 + 1),
                (current.0, current.1 - 1),
                (current.0 + 1, current.1 + 1),
                (current.0 + 1, current.1 - 1),
                (current.0 - 1, current.1 + 1),
                (current.0 - 1, current.1 - 1),
            ]
        } else {
            vec![
                (current.0 + 1, current.1),
                (current.0 - 1, current.1),
                (current.0, current.1 + 1),
                (current.0, current.1 - 1),
            ]
        };

        let current_g = *g_score.get(&current).unwrap_or(&i32::MAX);

        for neighbor in neighbors {
            if !is_walkable(neighbor.0, neighbor.1) && neighbor != end {
                continue;
            }

            let tentative_g = current_g + 1;
            let neighbor_g = *g_score.get(&neighbor).unwrap_or(&i32::MAX);

            if tentative_g < neighbor_g {
                came_from.insert(neighbor, current);
                g_score.insert(neighbor, tentative_g);
                let h = (neighbor.0 - end.0).abs() + (neighbor.1 - end.1).abs();
                open_set.push(AStarNode {
                    pos: neighbor,
                    f_score: tentative_g + h,
                });
            }
        }
    }

    None
}

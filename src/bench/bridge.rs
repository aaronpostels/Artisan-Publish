use crate::ecs::*;
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct BenchVec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

const COMPONENT_NAME: &str = "BenchVec3";

#[wasm_bindgen]
pub struct BridgeBench {
    world: World,
    entities: Vec<Entity>,
    comp_id: usize,

    _decoys: usize,
}

#[wasm_bindgen]
impl BridgeBench {

    #[wasm_bindgen(constructor)]
    pub fn new(n: usize, decoy_components: usize) -> BridgeBench {
        let mut world = World::new();

        for i in 0..decoy_components {
            world.schemas.push(crate::ecs::world::ComponentSchema {
                id: usize::MAX - i,
                name: format!("Decoy{i}"),
                array_type: 0,
                elements: 3,
                fields: vec!["x".into(), "y".into(), "z".into()],
            });
        }

        let comp_id = world.register::<BenchVec3>();
        world.schemas.push(crate::ecs::world::ComponentSchema {
            id: comp_id,
            name: COMPONENT_NAME.into(),
            array_type: 0,
            elements: 3,
            fields: vec!["x".into(), "y".into(), "z".into()],
        });

        let entities: Vec<Entity> = (0..n)
            .map(|i| world.spawn_with((BenchVec3 { x: i as f32, y: 0.0, z: 0.0 },)))
            .collect();

        BridgeBench { world, entities, comp_id, _decoys: decoy_components }
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn ptr_by_name(&self, index: usize, name: &str) -> u32 {
        let Some(&e) = self.entities.get(index) else { return 0 };
        let Some(cid) = self.world.schemas.iter().find(|s| s.name == name).map(|s| s.id) else {
            return 0;
        };
        self.ptr_for(e, cid)
    }

    pub fn ptr_by_id(&self, index: usize) -> u32 {
        let Some(&e) = self.entities.get(index) else { return 0 };
        self.ptr_for(e, self.comp_id)
    }

    fn ptr_for(&self, e: Entity, cid: usize) -> u32 {
        let rec = self.world.records[e.id as usize];
        let Some(arch) = self.world.archetypes.get(rec.archetype_id as usize) else { return 0 };
        let Some(&col_idx) = arch.component_to_column.get(cid) else { return 0 };
        if col_idx == u32::MAX {
            return 0;
        }
        let col = unsafe { &*arch.columns[col_idx as usize].get() };
        col.data.as_ptr::<u8>() as u32 + rec.row * col.data.item_size() as u32
    }

    pub fn marshalled(&self, index: usize) -> js_sys::Float32Array {
        let Some(&e) = self.entities.get(index) else {
            return js_sys::Float32Array::new_with_length(0);
        };
        match self.world.get_component::<BenchVec3>(e) {
            Some(v) => js_sys::Float32Array::from(&[v.x, v.y, v.z][..]),
            None => js_sys::Float32Array::new_with_length(0),
        }
    }

    pub fn marshalled_set(&mut self, index: usize, x: f32, y: f32, z: f32) {
        let Some(&e) = self.entities.get(index) else { return };
        if let Some(v) = self.world.get_component_mut::<BenchVec3>(e) {
            v.x = x;
            v.y = y;
            v.z = z;
        }
    }

    pub fn column_ptr(&self) -> u32 {
        let Some(&e) = self.entities.first() else { return 0 };
        self.ptr_for(e, self.comp_id)
    }

    pub fn structural_gen(&self) -> u32 {
        self.world.structural_gen as u32
    }

    pub fn mark_changed(&mut self, index: usize) {
        let Some(&e) = self.entities.get(index) else { return };
        let rec = self.world.records[e.id as usize];
        let tick = self.world.current_tick;
        let Some(arch) = self.world.archetypes.get_mut(rec.archetype_id as usize) else { return };
        let Some(&col_idx) = arch.component_to_column.get(self.comp_id) else { return };
        if col_idx == u32::MAX {
            return;
        }
        let col = unsafe { &mut *arch.columns[col_idx as usize].get() };
        if let Some(slot) = col.changed_ticks.get_mut(rec.row as usize) {
            *slot = tick;
        }
        col.last_changed_tick.store(tick, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn copy_out(&self) -> js_sys::Float32Array {
        let mut out = Vec::with_capacity(self.entities.len() * 3);
        for &e in &self.entities {
            if let Some(v) = self.world.get_component::<BenchVec3>(e) {
                out.push(v.x);
                out.push(v.y);
                out.push(v.z);
            }
        }
        js_sys::Float32Array::from(&out[..])
    }

    pub fn copy_in(&mut self, data: &[f32]) {
        for (i, &e) in self.entities.iter().enumerate() {
            let base = i * 3;
            if base + 2 >= data.len() {
                break;
            }
            if let Some(v) = self.world.get_component_mut::<BenchVec3>(e) {
                v.x = data[base];
                v.y = data[base + 1];
                v.z = data[base + 2];
            }
        }
    }

    pub fn checksum(&self) -> f64 {
        let mut sum = 0.0f64;
        for &e in &self.entities {
            if let Some(v) = self.world.get_component::<BenchVec3>(e) {
                sum += v.x as f64;
            }
        }
        sum
    }

    pub fn changed_row_count(&self) -> usize {
        let tick = self.world.current_tick;
        let mut count = 0;
        for &e in &self.entities {
            let rec = self.world.records[e.id as usize];
            let Some(arch) = self.world.archetypes.get(rec.archetype_id as usize) else { continue };
            let Some(&col_idx) = arch.component_to_column.get(self.comp_id) else { continue };
            if col_idx == u32::MAX {
                continue;
            }
            let col = unsafe { &*arch.columns[col_idx as usize].get() };
            if col.changed_ticks.get(rec.row as usize) == Some(&tick) {
                count += 1;
            }
        }
        count
    }

    pub fn advance_tick(&mut self) {
        self.world.current_tick += 1;
    }

    pub fn reset(&mut self) {
        for (i, &e) in self.entities.iter().enumerate() {
            if let Some(v) = self.world.get_component_mut::<BenchVec3>(e) {
                v.x = i as f32;
                v.y = 0.0;
                v.z = 0.0;
            }
        }
    }
}

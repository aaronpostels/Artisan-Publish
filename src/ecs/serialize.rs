use super::world::World;
use super::archetype::{Column, Archetype};
use super::entity::{Entity, EntityRecord};
use std::sync::atomic::Ordering;
use std::cell::UnsafeCell;
use std::sync::atomic::AtomicU32;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SerializedWorld {
    pub current_tick: u32,
    pub structural_gen: u64,
    pub next_entity_id: u32,
    pub records: Vec<EntityRecord>,
    pub archetypes: Vec<SerializedArchetype>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SerializedArchetype {
    pub id: u32,
    pub signature: Vec<usize>,
    pub entities: Vec<Entity>,
    pub columns: Vec<SerializedColumn>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SerializedColumn {
    pub component_id: usize,
    pub data: Vec<serde_json::Value>,
}

pub fn save_world_to_json(world: &World) -> String {
    let mut sw = SerializedWorld {
        current_tick: world.current_tick,
        structural_gen: world.structural_gen,
        next_entity_id: world.next_entity_id.load(Ordering::Relaxed),
        records: world.records.clone(),
        archetypes: Vec::new(),
    };
    for arch in &world.archetypes {
        let mut sa = SerializedArchetype {
            id: arch.id,
            signature: arch.signature.to_vec(),
            entities: arch.entities.clone(),
            columns: Vec::new(),
        };
        for &cid in &arch.signature[..] {
            if let Some(ser) = world.component_serializers.get(&cid) {
                let mut sc = SerializedColumn { component_id: cid, data: Vec::new() };
                let col_idx = arch.component_to_column[cid];
                let col = unsafe { &*arch.columns[col_idx as usize].get() };
                for row in 0..arch.entities.len() {
                    let ptr = unsafe { col.data.get_raw(row) };
                    sc.data.push(ser(ptr));
                }
                sa.columns.push(sc);
            }
        }
        sw.archetypes.push(sa);
    }
    serde_json::to_string(&sw).unwrap_or_else(|_| "{}".to_string())
}

pub fn load_world_from_json(world: &mut World, json: &str) {
    if let Ok(sw) = serde_json::from_str::<SerializedWorld>(json) {
        world.records = sw.records;
        world.current_tick = sw.current_tick;
        world.structural_gen = sw.structural_gen;
        world.next_entity_id.store(sw.next_entity_id, Ordering::Relaxed);

        world.archetypes.clear();
        world.active_archetypes.clear();
        world.archetype_index.clear();

        for sa in sw.archetypes {
            if !sa.entities.is_empty() {
                world.active_archetypes.push(sa.id);
            }
            world.archetype_index.insert(sa.signature.clone().into_boxed_slice(), sa.id);

            let mut columns = Vec::new();
            for &cid in &sa.signature {
                columns.push(UnsafeCell::new(Column::new(world.component_layouts[cid], world.component_drop_fns[cid], sa.entities.len().max(8))));
            }

            let mut component_to_column = vec![u32::MAX; world.component_layouts.len()];
            for (i, &cid) in sa.signature.iter().enumerate() {
                component_to_column[cid] = i as u32;
            }

            let arch = Archetype {
                id: sa.id,
                last_modified_tick: AtomicU32::new(world.current_tick),
                entities: sa.entities,
                columns: columns.into_boxed_slice(),
                signature: sa.signature.into_boxed_slice(),
                component_to_column: component_to_column.into_boxed_slice(),
                add_edges: Vec::new(),
                remove_edges: Vec::new(),
            };

            for sc in sa.columns {
                if let Some(deser) = world.component_deserializers.get(&sc.component_id) {
                    let col_idx = arch.component_to_column[sc.component_id];
                    let col = unsafe { &mut *arch.columns[col_idx as usize].get() };
                    for val in sc.data {
                        unsafe {
                            let ptr = std::alloc::alloc(col.data.item_layout());
                            deser(val, ptr);
                            col.data.push_raw(ptr);
                            col.added_ticks.push(sw.current_tick);
                            col.changed_ticks.push(sw.current_tick);
                            std::alloc::dealloc(ptr, col.data.item_layout());
                        }
                    }
                }
            }
            world.archetypes.push(arch);
        }
    }
}

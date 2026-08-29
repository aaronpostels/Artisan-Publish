#![allow(clippy::type_complexity)]

use std::alloc::Layout;
use std::any::{Any, TypeId};
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::archetype::{Archetype, Column};
use crate::ecs::component::{Component, Resource, Bundle, Children, Parent};
use super::entity::{Entity, EntityRecord};

pub type SerializerFn = fn(*const u8) -> serde_json::Value;
pub type DeserializerFn = fn(serde_json::Value, *mut u8);

pub trait AnyQueue: Send + Sync {
    fn apply(&mut self, world: &mut World);
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn merge(&mut self, other: Box<dyn AnyQueue>);
    fn is_empty(&self) -> bool;
}

pub enum Op<T> {
    Insert(Entity, T),
    Remove(Entity),
}

pub struct TypedQueue<T> {
    pub ops: Vec<Op<T>>,
}

impl<T: Component + Send + Sync + 'static> AnyQueue for TypedQueue<T> {
    fn apply(&mut self, world: &mut World) {
        for op in self.ops.drain(..) {
            match op {
                Op::Insert(e, c) => world.add_component(e, c),
                Op::Remove(e) => world.remove_component::<T>(e),
            }
        }
    }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn merge(&mut self, mut other: Box<dyn AnyQueue>) {
        if let Some(other_q) = other.as_any_mut().downcast_mut::<TypedQueue<T>>() {
            self.ops.append(&mut other_q.ops);
        }
    }
    fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

pub struct ComponentSchema {
    pub id: usize,
    pub name: String,
    pub array_type: u32,
    pub elements: u32,
    pub fields: Vec<String>,
}

pub struct World {
    pub current_tick: u32,
    pub structural_gen: u64,
    pub next_entity_id: AtomicU32,
    pub free_entities: Mutex<Vec<Entity>>,
    pub free_entities_count: AtomicU32,
    pub records: Vec<EntityRecord>,
    pub archetypes: Vec<Archetype>,
    pub active_archetypes: Vec<u32>,
    pub archetype_index: HashMap<Box<[usize]>, u32>,
    pub resource_ids: Vec<(TypeId, usize)>,
    pub resources: Vec<UnsafeCell<Option<Box<dyn Any>>>>,
    pub component_ids: Vec<(TypeId, usize)>,
    pub component_layouts: Vec<Layout>,
    pub component_drop_fns: Vec<Option<unsafe fn(*mut u8)>>,
    pub dynamic_components: HashMap<String, usize>,
    pub schemas: Vec<ComponentSchema>,
    pub component_serializers: HashMap<usize, SerializerFn>,
    pub component_deserializers: HashMap<usize, DeserializerFn>,
    pub spawn_queue: Mutex<Vec<Entity>>,
    pub despawn_queue: Mutex<Vec<Entity>>,
    pub typed_queues: Mutex<Vec<(TypeId, Box<dyn AnyQueue>)>>,
    pub bundle_cache: Vec<(TypeId, u32, Box<[usize]>)>,
}

unsafe impl Send for World {}
unsafe impl Sync for World {}

impl Default for World {
    fn default() -> Self { Self::new() }
}

impl World {
    pub fn new() -> Self {
        let mut world = Self {
            current_tick: 1,
            structural_gen: 1,
            next_entity_id: AtomicU32::new(0),
            free_entities: Mutex::new(Vec::new()),
            free_entities_count: AtomicU32::new(0),
            records: Vec::new(),
            archetypes: Vec::new(),
            active_archetypes: Vec::new(),
            archetype_index: HashMap::new(),
            resource_ids: Vec::new(),
            resources: Vec::new(),
            component_ids: Vec::new(),
            component_layouts: Vec::new(),
            component_drop_fns: Vec::new(),
            dynamic_components: HashMap::new(),
            schemas: Vec::new(),
            component_serializers: HashMap::new(),
            component_deserializers: HashMap::new(),
            spawn_queue: Mutex::new(Vec::new()),
            despawn_queue: Mutex::new(Vec::new()),
            typed_queues: Mutex::new(Vec::new()),
            bundle_cache: Vec::new(),
        };
        world.get_or_create_archetype(&[]);
        world
    }

    pub fn insert_resource<T: Resource>(&mut self, resource: T) {
        let tid = TypeId::of::<T>();
        match self.resource_ids.binary_search_by_key(&tid, |&(t, _)| t) {
            Ok(idx) => {
                let id = self.resource_ids[idx].1;
                self.resources[id] = UnsafeCell::new(Some(Box::new(resource)));
            }
            Err(insert_idx) => {
                let id = self.resources.len();
                self.resource_ids.insert(insert_idx, (tid, id));
                self.resources.push(UnsafeCell::new(Some(Box::new(resource))));
            }
        }
    }

    #[inline(always)]
    pub fn get_resource<T: Resource>(&self) -> Option<&T> {
        let tid = TypeId::of::<T>();
        if let Ok(idx) = self.resource_ids.binary_search_by_key(&tid, |&(t, _)| t) {
            let id = self.resource_ids[idx].1;
            let opt = unsafe { &*self.resources[id].get() };
            return opt.as_ref()?.downcast_ref::<T>();
        }
        None
    }

    pub fn get_resource_mut<T: Resource>(&mut self) -> Option<&mut T> {
        let tid = TypeId::of::<T>();
        if let Ok(idx) = self.resource_ids.binary_search_by_key(&tid, |&(t, _)| t) {
            let id = self.resource_ids[idx].1;
            let opt = unsafe { &mut *self.resources[id].get() };
            return opt.as_mut()?.downcast_mut::<T>();
        }
        None
    }

    #[inline(always)]
    pub fn get_resource_mut_ptr<T: Resource>(&self) -> Option<*mut T> {
        let tid = TypeId::of::<T>();
        if let Ok(idx) = self.resource_ids.binary_search_by_key(&tid, |&(t, _)| t) {
            let id = self.resource_ids[idx].1;
            let opt = unsafe { &mut *self.resources[id].get() };
            return Some(opt.as_mut()?.downcast_mut::<T>()? as *mut T);
        }
        None
    }

    pub fn remove_resource<T: Resource>(&mut self) {
        let tid = TypeId::of::<T>();
        if let Ok(idx) = self.resource_ids.binary_search_by_key(&tid, |&(t, _)| t) {
            let id = self.resource_ids[idx].1;
            self.resources[id] = UnsafeCell::new(None);
        }
    }

    pub fn register<T: Component>(&mut self) -> usize {
        let tid = TypeId::of::<T>();
        match self.component_ids.binary_search_by_key(&tid, |&(t, _)| t) {
            Ok(idx) => self.component_ids[idx].1,
            Err(insert_idx) => {
                let id = self.component_layouts.len();
                self.component_ids.insert(insert_idx, (tid, id));
                self.component_layouts.push(Layout::new::<T>());

                let drop_fn: Option<unsafe fn(*mut u8)> = if std::mem::needs_drop::<T>() {
                    Some(|ptr: *mut u8| unsafe { ptr.cast::<T>().drop_in_place() })
                } else {
                    None
                };
                self.component_drop_fns.push(drop_fn);

                for arch in &mut self.archetypes {
                    if arch.component_to_column.len() <= id {
                        let mut new_map = vec![u32::MAX; id + 1];
                        new_map[..arch.component_to_column.len()].copy_from_slice(&arch.component_to_column);
                        arch.component_to_column = new_map.into_boxed_slice();
                    }
                }
                id
            }
        }
    }

    pub fn register_serializable<T: Component + serde::Serialize + serde::de::DeserializeOwned>(&mut self) -> usize {
        let id = self.register::<T>();
        self.component_serializers.insert(id, |ptr| {
            let val = unsafe { &*(ptr as *const T) };
            serde_json::to_value(val).unwrap_or(serde_json::Value::Null)
        });
        self.component_deserializers.insert(id, |val, ptr| {
            if let Ok(deserialized) = serde_json::from_value::<T>(val) {
                unsafe { std::ptr::write(ptr as *mut T, deserialized) };
            }
        });
        id
    }

    pub fn register_schema<T: Component>(&mut self, name: &str, array_type: u32, elements: u32) -> usize {
        let id = self.register::<T>();
        if !self.schemas.iter().any(|s| s.id == id) {
            self.schemas.push(ComponentSchema {
                id,
                name: name.to_string(),
                array_type,
                elements,
                fields: Vec::new(),
            });
        }
        id
    }

    #[inline(always)]
    pub fn get_component_id<T: Component>(&self) -> Option<usize> {
        let tid = TypeId::of::<T>();
        self.component_ids.binary_search_by_key(&tid, |&(t, _)| t).ok().map(|idx| self.component_ids[idx].1)
    }

    #[inline(always)]
    pub fn get_component_id_by_type_id(&self, tid: TypeId) -> Option<usize> {
        self.component_ids.binary_search_by_key(&tid, |&(t, _)| t).ok().map(|idx| self.component_ids[idx].1)
    }

    pub fn get_or_create_archetype(&mut self, signature: &[usize]) -> u32 {
        if let Some(&id) = self.archetype_index.get(signature) {
            return id;
        }

        let id = self.archetypes.len() as u32;
        let num_comp = self.component_layouts.len();

        let columns: Box<[UnsafeCell<Column>]> = signature
            .iter()
            .map(|&cid| {
                UnsafeCell::new(Column::new(
                    self.component_layouts[cid],
                    self.component_drop_fns[cid],
                    8,
                ))
            })
            .collect();

        let mut component_to_column = vec![u32::MAX; num_comp];
        for (col_idx, &cid) in signature.iter().enumerate() {
            component_to_column[cid] = col_idx as u32;
        }

        self.archetypes.push(Archetype {
            entities: Vec::with_capacity(8),
            columns,
            id,
            last_modified_tick: AtomicU32::new(self.current_tick),
            signature: signature.to_vec().into_boxed_slice(),
            component_to_column: component_to_column.into_boxed_slice(),
            add_edges: Vec::new(),
            remove_edges: Vec::new(),
        });
        self.archetype_index.insert(signature.to_vec().into_boxed_slice(), id);
        id
    }

    #[inline(always)]
    pub fn is_alive(&self, entity: Entity) -> bool {
        let id = entity.id as usize;
        id < self.records.len() && {
            let rec = unsafe { self.records.get_unchecked(id) };
            rec.generation == entity.generation && rec.archetype_id != u32::MAX
        }
    }

    #[inline(always)]
    pub fn generation_of(&self, id: u32) -> Option<u32> {
        self.records.get(id as usize).map(|r| r.generation)
    }

    pub fn reserve_entity(&self) -> Entity {
        if self.free_entities_count.load(Ordering::Relaxed) > 0 {
            if let Ok(mut free) = self.free_entities.lock() {
                if let Some(e) = free.pop() {
                    self.free_entities_count.fetch_sub(1, Ordering::Relaxed);
                    return e;
                }
            }
        }
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        Entity { id, generation: 0 }
    }

    pub fn flush_spawn(&mut self, entity: Entity) {
        let id = entity.id as usize;
        if id >= self.records.len() {
            let new_len = (id + 1).max(self.records.len() * 2).max(128);
            self.records.resize(new_len, EntityRecord::DEAD);
        }
        let arch = &mut self.archetypes[0];
        if arch.entities.is_empty() {
            self.active_archetypes.push(0);
        }
        let row = arch.entities.len() as u32;
        arch.entities.push(entity);
        self.records[id] = EntityRecord { generation: entity.generation, archetype_id: 0, row };
        self.structural_gen += 1;
    }

    pub fn entity_count(&self) -> usize {
        self.archetypes.iter().map(|a| a.entities.len()).sum()
    }

    pub fn spawn(&mut self) -> Entity {
        let entity = if self.free_entities_count.load(Ordering::Relaxed) > 0 {
            let free = self.free_entities.get_mut().unwrap();
            if let Some(e) = free.pop() {
                self.free_entities_count.fetch_sub(1, Ordering::Relaxed);
                e
            } else {
                let id = *self.next_entity_id.get_mut();
                *self.next_entity_id.get_mut() = id + 1;
                Entity { id, generation: 0 }
            }
        } else {
            let id = *self.next_entity_id.get_mut();
            *self.next_entity_id.get_mut() = id + 1;
            Entity { id, generation: 0 }
        };
        self.flush_spawn(entity);
        entity
    }

    pub fn spawn_with<B: Bundle>(&mut self, bundle: B) -> Entity {
        let tid = TypeId::of::<B>();

        let cache_idx = match self.bundle_cache.binary_search_by_key(&tid, |&(t, _, _)| t) {
            Ok(idx) => idx,
            Err(insert_idx) => {
                let comp_ids = B::tuple_component_ids(self);
                let mut sig = comp_ids.clone();
                sig.sort_unstable();
                let arch_id = self.get_or_create_archetype(&sig);

                let arch = &self.archetypes[arch_id as usize];
                let mut col_indices = Vec::with_capacity(comp_ids.len());
                for comp_id in comp_ids {
                    col_indices.push(arch.component_to_column[comp_id] as usize);
                }

                self.bundle_cache.insert(insert_idx, (tid, arch_id, col_indices.into_boxed_slice()));
                insert_idx
            }
        };

        let entity = if let Some(e) = self.free_entities.get_mut().unwrap().pop() {
            e
        } else {
            let id = *self.next_entity_id.get_mut();
            *self.next_entity_id.get_mut() = id + 1;
            Entity { id, generation: 0 }
        };

        let id = entity.id as usize;
        if id >= self.records.len() {
            let new_len = (id + 1).max(self.records.len() * 2).max(128);
            self.records.resize(new_len, EntityRecord::DEAD);
        }

        let arch_id;
        let col_indices_ptr;
        let col_indices_len;
        {
            let cache = unsafe { self.bundle_cache.get_unchecked(cache_idx) };
            arch_id = cache.1;
            col_indices_ptr = cache.2.as_ptr();
            col_indices_len = cache.2.len();
        }

        {
            let arch = &mut self.archetypes[arch_id as usize];
            if arch.entities.is_empty() {
                self.active_archetypes.push(arch_id);
            }
            let row = arch.entities.len() as u32;
            arch.entities.push(entity);
            self.records[id] = EntityRecord { generation: entity.generation, archetype_id: arch_id, row: row };
        }

        let col_indices = unsafe { std::slice::from_raw_parts(col_indices_ptr, col_indices_len) };
        unsafe { bundle.insert_components(&mut self.archetypes[arch_id as usize], col_indices, self.current_tick); }

        self.archetypes[arch_id as usize].last_modified_tick.store(self.current_tick, Ordering::Relaxed);
        self.structural_gen += 1;
        entity
    }

    pub fn set_parent(&mut self, child: Entity, parent_opt: Option<Entity>) {
        if !self.is_alive(child) { return; }

        let mut old_parent_opt = None;
        if let Some(p_comp) = self.get_component::<Parent>(child) {
            old_parent_opt = Some(p_comp.0);
        }

        if let Some(old_parent) = old_parent_opt {
            if self.is_alive(old_parent) {
                if let Some(children) = self.get_component_mut::<Children>(old_parent) {
                    children.0.retain(|&e| e != child);
                }
            }
        }

        if let Some(new_parent) = parent_opt {
            if !self.is_alive(new_parent) { return; }

            self.add_component(child, Parent(new_parent));

            let has_children_comp = self.get_component::<Children>(new_parent).is_some();
            if has_children_comp {
                if let Some(children) = self.get_component_mut::<Children>(new_parent) {
                    if !children.0.contains(&child) {
                        children.0.push(child);
                    }
                }
            } else {
                self.add_component(new_parent, Children(vec![child]));
            }
        } else {
            self.remove_component::<Parent>(child);
        }
    }

    pub fn kill(&mut self, entity: Entity) {
        let id = entity.id as usize;
        if id >= self.records.len() { return; }
        let rec = unsafe { *self.records.get_unchecked(id) };
        if rec.generation != entity.generation || rec.archetype_id == u32::MAX { return; }

        let children_cid = self.get_component_id::<Children>();
        let children_cid = match children_cid {
            Some(cid) => cid,
            None => {
                self.kill_single_internal(entity, rec);
                return;
            }
        };

        let has_children = |world: &World, arch_id: u32| -> bool {
            let arch = unsafe { world.archetypes.get_unchecked(arch_id as usize) };
            children_cid < arch.component_to_column.len() && unsafe { *arch.component_to_column.get_unchecked(children_cid) } != u32::MAX
        };

        if !has_children(self, rec.archetype_id) {
            self.kill_single_internal(entity, rec);
            return;
        }

        let mut to_kill = vec![entity];
        while let Some(e) = to_kill.pop() {
            let eid = e.id as usize;
            if eid >= self.records.len() { continue; }
            let erec = unsafe { *self.records.get_unchecked(eid) };
            if erec.generation != e.generation || erec.archetype_id == u32::MAX { continue; }

            if has_children(self, erec.archetype_id) {
                if let Some(children) = self.get_component::<Children>(e) {
                    for &child in &children.0 { to_kill.push(child); }
                }
            }
            self.kill_single_internal(e, erec);
        }
    }

    #[inline(always)]
    fn kill_single_internal(&mut self, entity: Entity, rec: EntityRecord) {
        let new_gen = rec.generation.wrapping_add(1);
        if let Some(free) = self.free_entities.get_mut().ok() {
            free.push(Entity { id: entity.id, generation: new_gen });
            self.free_entities_count.fetch_add(1, Ordering::Relaxed);
        }
        self.records[entity.id as usize].generation = new_gen;
        self.records[entity.id as usize].archetype_id = u32::MAX;

        let arch = unsafe { self.archetypes.get_unchecked_mut(rec.archetype_id as usize) };
        arch.entities.swap_remove(rec.row as usize);

        if arch.entities.is_empty() {
            if let Some(pos) = self.active_archetypes.iter().position(|&x| x == rec.archetype_id) {
                self.active_archetypes.swap_remove(pos);
            }
        }

        if (rec.row as usize) < arch.entities.len() {
            let moved_entity_id = arch.entities[rec.row as usize].id as usize;
            self.records[moved_entity_id].row = rec.row;
        }

        for col_cell in arch.columns.iter() {
            let col = unsafe { &mut *col_cell.get() };
            unsafe { col.swap_remove_drop(rec.row as usize) };
        }
        arch.last_modified_tick.store(self.current_tick, Ordering::Relaxed);
        self.structural_gen += 1;
    }

    #[inline]
    pub fn add_component<T: Component>(&mut self, entity: Entity, component: T) {
        if !self.is_alive(entity) { return; }
        let comp_id = self.register::<T>();
        self.add_component_raw(entity, comp_id, &raw const component as *const u8);
        std::mem::forget(component);
    }

    pub fn add_component_raw(&mut self, entity: Entity, comp_id: usize, component_ptr: *const u8) {
        let rec = unsafe { *self.records.get_unchecked(entity.id as usize) };
        let old_arch_id = rec.archetype_id;

        {
            let arch = unsafe { self.archetypes.get_unchecked_mut(old_arch_id as usize) };
            if comp_id < arch.component_to_column.len() {
                let col_idx = unsafe { *arch.component_to_column.get_unchecked(comp_id) };
                if col_idx != u32::MAX {
                    let col = unsafe { &mut *arch.columns.get_unchecked(col_idx as usize).get() };
                    unsafe {
                        let slot = col.data.get_raw(rec.row as usize);
                        std::ptr::copy_nonoverlapping(component_ptr, slot, col.data.item_size());
                    }
                    col.changed_ticks[rec.row as usize] = self.current_tick;
                    col.last_changed_tick.store(self.current_tick, Ordering::Relaxed);
                    arch.last_modified_tick.store(self.current_tick, Ordering::Relaxed);
                    return;
                }
            }
        }

        let new_arch_id = {
            let edges = &mut self.archetypes[old_arch_id as usize].add_edges;
            match edges.binary_search_by_key(&comp_id, |&(k, _)| k) {
                Ok(idx) => edges[idx].1,
                Err(insert_idx) => {
                    let mut new_sig = self.archetypes[old_arch_id as usize].signature.to_vec();
                    new_sig.push(comp_id);
                    new_sig.sort_unstable();
                    let id = self.get_or_create_archetype(&new_sig);
                    self.archetypes[old_arch_id as usize].add_edges.insert(insert_idx, (comp_id, id));
                    id
                }
            }
        };

        unsafe {
            let archetypes_ptr = self.archetypes.as_mut_ptr();
            let old = &mut *archetypes_ptr.add(old_arch_id as usize);
            let new = &mut *archetypes_ptr.add(new_arch_id as usize);

            old.entities.swap_remove(rec.row as usize);
            if old.entities.is_empty() {
                if let Some(pos) = self.active_archetypes.iter().position(|&x| x == old_arch_id) {
                    self.active_archetypes.swap_remove(pos);
                }
            }
            if new.entities.is_empty() {
                self.active_archetypes.push(new_arch_id);
            }
            new.entities.push(entity);
            let new_row = (new.entities.len() - 1) as u32;

            *self.records.get_unchecked_mut(entity.id as usize) = EntityRecord { generation: entity.generation, archetype_id: new_arch_id, row: new_row };

            if (rec.row as usize) < old.entities.len() {
                self.records[old.entities[rec.row as usize].id as usize].row = rec.row;
            }

            for i in 0..old.signature.len() {
                let cid = old.signature[i];
                let dst_idx = *new.component_to_column.get_unchecked(cid) as usize;

                let old_col = &mut *old.columns.get_unchecked(i).get();
                let new_col = &mut *new.columns.get_unchecked(dst_idx).get();

                let raw_ptr = old_col.data.get_raw(rec.row as usize);
                new_col.data.push_raw(raw_ptr as *const u8);

                let (added, changed) = old_col.swap_remove_forget_no_copy(rec.row as usize);
                new_col.added_ticks.push(added);
                new_col.changed_ticks.push(changed);
            }

            let new_col_idx = *new.component_to_column.get_unchecked(comp_id) as usize;
            let new_col = &mut *new.columns.get_unchecked(new_col_idx).get();
            new_col.data.push_raw(component_ptr);
            new_col.added_ticks.push(self.current_tick);
            new_col.changed_ticks.push(self.current_tick);
            new_col.last_added_tick.store(self.current_tick, Ordering::Relaxed);
            new_col.last_changed_tick.store(self.current_tick, Ordering::Relaxed);

            new.last_modified_tick.store(self.current_tick, Ordering::Relaxed);
            old.last_modified_tick.store(self.current_tick, Ordering::Relaxed);
        }
        self.structural_gen += 1;
    }

    #[inline]
    pub fn remove_component<T: Component>(&mut self, entity: Entity) {
        if !self.is_alive(entity) { return; }
        let comp_id = match self.get_component_id_by_type_id(TypeId::of::<T>()) {
            Some(id) => id,
            None => return,
        };
        self.remove_component_raw(entity, comp_id);
    }

    pub fn remove_component_raw(&mut self, entity: Entity, comp_id: usize) {
        let rec = unsafe { *self.records.get_unchecked(entity.id as usize) };
        let old_arch_id = rec.archetype_id;

        let old_col_idx = {
            let arch = unsafe { self.archetypes.get_unchecked(old_arch_id as usize) };
            if comp_id < arch.component_to_column.len() {
                unsafe { *arch.component_to_column.get_unchecked(comp_id) }
            } else {
                u32::MAX
            }
        };
        if old_col_idx == u32::MAX { return; }

        let new_arch_id = {
            let edges = &mut self.archetypes[old_arch_id as usize].remove_edges;
            match edges.binary_search_by_key(&comp_id, |&(k, _)| k) {
                Ok(idx) => edges[idx].1,
                Err(insert_idx) => {
                    let mut new_sig = self.archetypes[old_arch_id as usize].signature.to_vec();
                    new_sig.retain(|&c| c != comp_id);
                    let id = self.get_or_create_archetype(&new_sig);
                    self.archetypes[old_arch_id as usize].remove_edges.insert(insert_idx, (comp_id, id));
                    id
                }
            }
        };

        unsafe {
            let archetypes_ptr = self.archetypes.as_mut_ptr();
            let old = &mut *archetypes_ptr.add(old_arch_id as usize);
            let new = &mut *archetypes_ptr.add(new_arch_id as usize);

            old.entities.swap_remove(rec.row as usize);
            if old.entities.is_empty() {
                if let Some(pos) = self.active_archetypes.iter().position(|&x| x == old_arch_id) {
                    self.active_archetypes.swap_remove(pos);
                }
            }
            if new.entities.is_empty() {
                self.active_archetypes.push(new_arch_id);
            }
            new.entities.push(entity);
            let new_row = (new.entities.len() - 1) as u32;

            *self.records.get_unchecked_mut(entity.id as usize) = EntityRecord { generation: entity.generation, archetype_id: new_arch_id, row: new_row };
            if (rec.row as usize) < old.entities.len() {
                self.records[old.entities[rec.row as usize].id as usize].row = rec.row;
            }

            for (new_col_idx, &cid) in new.signature.iter().enumerate() {
                let old_src_idx = *old.component_to_column.get_unchecked(cid) as usize;
                let old_col = &mut *old.columns.get_unchecked(old_src_idx).get();
                let new_col = &mut *new.columns.get_unchecked(new_col_idx).get();

                let raw_ptr = old_col.data.get_raw(rec.row as usize);
                new_col.data.push_raw(raw_ptr as *const u8);

                let (added, changed) = old_col.swap_remove_forget_no_copy(rec.row as usize);
                new_col.added_ticks.push(added);
                new_col.changed_ticks.push(changed);
            }

            let old_col = &mut *old.columns.get_unchecked(old_col_idx as usize).get();
            old_col.swap_remove_drop(rec.row as usize);

            new.last_modified_tick.store(self.current_tick, Ordering::Relaxed);
            old.last_modified_tick.store(self.current_tick, Ordering::Relaxed);
        }
        self.structural_gen += 1;
    }

    pub fn register_dynamic(&mut self, name: &str, size: usize, align: usize) -> usize {
        if let Some(&id) = self.dynamic_components.get(name) {
            return id;
        }
        let id = self.component_layouts.len();
        self.dynamic_components.insert(name.to_string(), id);
        let layout = Layout::from_size_align(size, align.max(1)).unwrap_or(Layout::from_size_align(0, 1).unwrap());
        self.component_layouts.push(layout);
        self.component_drop_fns.push(None);
        for arch in &mut self.archetypes {
            if arch.component_to_column.len() <= id {
                let mut new_map = vec![u32::MAX; id + 1];
                new_map[..arch.component_to_column.len()].copy_from_slice(&arch.component_to_column);
                arch.component_to_column = new_map.into_boxed_slice();
            }
        }
        id
    }

    #[inline(always)]
    pub fn get_component<T: Component>(&self, entity: Entity) -> Option<&T> {
        let id = entity.id as usize;
        if id >= self.records.len() { return None; }
        let rec = unsafe { self.records.get_unchecked(id) };
        if rec.generation != entity.generation || rec.archetype_id == u32::MAX { return None; }

        let comp_id = self.get_component_id::<T>()?;
        let arch = unsafe { self.archetypes.get_unchecked(rec.archetype_id as usize) };
        if comp_id >= arch.component_to_column.len() { return None; }
        let col_idx = unsafe { *arch.component_to_column.get_unchecked(comp_id) };
        if col_idx == u32::MAX { return None; }

        let col = unsafe { &*arch.columns.get_unchecked(col_idx as usize).get() };
        Some(unsafe { &*col.data.as_ptr::<T>().add(rec.row as usize) })
    }

    #[inline(always)]
    pub fn get_component_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        let id = entity.id as usize;
        if id >= self.records.len() { return None; }
        let rec = unsafe { *self.records.get_unchecked(id) };
        if rec.generation != entity.generation || rec.archetype_id == u32::MAX { return None; }

        let comp_id = self.get_component_id::<T>()?;
        let arch = unsafe { self.archetypes.get_unchecked_mut(rec.archetype_id as usize) };
        if comp_id >= arch.component_to_column.len() { return None; }
        let col_idx = unsafe { *arch.component_to_column.get_unchecked(comp_id) };
        if col_idx == u32::MAX { return None; }

        let col = unsafe { &mut *arch.columns.get_unchecked(col_idx as usize).get() };
        col.changed_ticks[rec.row as usize] = self.current_tick;
        col.last_changed_tick.store(self.current_tick, Ordering::Relaxed);
        arch.last_modified_tick.store(self.current_tick, Ordering::Relaxed);
        Some(unsafe { &mut *col.data.as_mut_ptr::<T>().add(rec.row as usize) })
    }

    #[inline(always)]
    pub fn get_component_by_id<T: Component>(&self, entity: Entity, comp_id: usize) -> Option<&T> {
        let id = entity.id as usize;
        if id >= self.records.len() { return None; }
        let rec = unsafe { self.records.get_unchecked(id) };
        if rec.generation != entity.generation || rec.archetype_id == u32::MAX { return None; }

        let arch = unsafe { self.archetypes.get_unchecked(rec.archetype_id as usize) };
        if comp_id >= arch.component_to_column.len() { return None; }
        let col_idx = unsafe { *arch.component_to_column.get_unchecked(comp_id) };
        if col_idx == u32::MAX { return None; }

        let col = unsafe { &*arch.columns.get_unchecked(col_idx as usize).get() };
        Some(unsafe { &*col.data.as_ptr::<T>().add(rec.row as usize) })
    }

    #[inline(always)]
    pub fn get_column_ptr<T: Component>(&self, archetype_id: u32) -> Option<*const T> {
        let comp_id = self.get_component_id::<T>()?;
        let arch = self.archetypes.get(archetype_id as usize)?;
        if comp_id >= arch.component_to_column.len() { return None; }
        let col_idx = arch.component_to_column[comp_id];
        if col_idx == u32::MAX { return None; }
        let col = unsafe { &*arch.columns[col_idx as usize].get() };
        Some(col.data.as_ptr::<T>())
    }

    #[inline(always)]
    pub fn get_column_mut_ptr<T: Component>(&self, archetype_id: u32) -> Option<*mut T> {
        let comp_id = self.get_component_id::<T>()?;
        let arch = self.archetypes.get(archetype_id as usize)?;
        if comp_id >= arch.component_to_column.len() { return None; }
        let col_idx = arch.component_to_column[comp_id];
        if col_idx == u32::MAX { return None; }
        let col = unsafe { &*arch.columns[col_idx as usize].get() };
        Some(unsafe { col.data.as_mut_ptr::<T>() })
    }

    #[inline(always)]
    pub fn get_column_len(&self, archetype_id: u32) -> usize {
        self.archetypes.get(archetype_id as usize).map_or(0, |a| a.entities.len())
    }

    #[inline(always)]
    pub fn get_column_capacity<T: Component>(&self, archetype_id: u32) -> Option<usize> {
        let comp_id = self.get_component_id::<T>()?;
        let arch = self.archetypes.get(archetype_id as usize)?;
        if comp_id >= arch.component_to_column.len() { return None; }
        let col_idx = arch.component_to_column[comp_id];
        if col_idx == u32::MAX { return None; }
        let col = unsafe { &*arch.columns[col_idx as usize].get() };
        Some(col.data.capacity())
    }

    pub fn apply_commands(&mut self) {
        if unsafe { self.spawn_queue.get_mut().unwrap_unchecked().is_empty() && self.despawn_queue.get_mut().unwrap_unchecked().is_empty() && self.typed_queues.get_mut().unwrap_unchecked().is_empty() } { return; }
        let mut spawns = std::mem::take(self.spawn_queue.get_mut().unwrap());
        let mut despawns = std::mem::take(self.despawn_queue.get_mut().unwrap());

        if !spawns.is_empty() {
            let max_id = spawns.iter().map(|e| e.id).max().unwrap_or(0) as usize;
            if max_id >= self.records.len() {
                let new_len = (max_id + 1).max(self.records.len() * 2).max(128);
                self.records.resize(new_len, EntityRecord::DEAD);
            }
            let arch = &mut self.archetypes[0];
            if arch.entities.is_empty() {
                self.active_archetypes.push(0);
            }
            let start_row = arch.entities.len();
            arch.entities.extend_from_slice(&spawns);
            for (i, &e) in spawns.iter().enumerate() {
                self.records[e.id as usize] = EntityRecord {
                    generation: e.generation,
                    archetype_id: 0,
                    row: (start_row + i) as u32,
                };
            }
            self.structural_gen += 1;
        }

        let mut queues = std::mem::take(self.typed_queues.get_mut().unwrap());
        for (_, q) in &mut queues { q.apply(self); }

        if !despawns.is_empty() {
            despawns.sort_unstable_by_key(|e| e.id);
            despawns.dedup_by_key(|e| e.id);

            let mut arch_despawn_counts = vec![0; self.archetypes.len()];
            for &e in &despawns {
                let id = e.id as usize;
                if id < self.records.len() {
                    let rec = self.records[id];
                    if rec.generation == e.generation && rec.archetype_id != u32::MAX {
                        arch_despawn_counts[rec.archetype_id as usize] += 1;
                    }
                }
            }

            for &e in &despawns {
                let id = e.id as usize;
                if id >= self.records.len() { continue; }
                let rec = self.records[id];
                if rec.generation != e.generation || rec.archetype_id == u32::MAX { continue; }

                let arch_id = rec.archetype_id as usize;
                let arch = &mut self.archetypes[arch_id];

                if arch_despawn_counts[arch_id] == arch.entities.len() && arch.entities.len() > 1 {
                    if let Some(free) = self.free_entities.get_mut().ok() {
                        let mut count = 0;
                        for &ent in &arch.entities {
                            let eid = ent.id as usize;
                            let new_gen = self.records[eid].generation.wrapping_add(1);
                            self.records[eid].archetype_id = u32::MAX;
                            self.records[eid].generation = new_gen;
                            free.push(Entity { id: ent.id, generation: new_gen });
                            count += 1;
                        }
                        self.free_entities_count.fetch_add(count, Ordering::Relaxed);
                    }
                    arch.entities.clear();
                    for col_cell in arch.columns.iter() {
                        unsafe { &mut *col_cell.get() }.clear();
                    }
                    arch.last_modified_tick.store(self.current_tick, Ordering::Relaxed);

                    if let Some(pos) = self.active_archetypes.iter().position(|&x| x == arch_id as u32) {
                        self.active_archetypes.swap_remove(pos);
                    }
                } else {
                    self.kill(e);
                }
            }
            self.structural_gen += 1;
        }

        spawns.clear();
        let q_spawn = self.spawn_queue.get_mut().unwrap();
        if q_spawn.capacity() < spawns.capacity() { *q_spawn = spawns; }

        despawns.clear();
        let q_despawn = self.despawn_queue.get_mut().unwrap();
        if q_despawn.capacity() < despawns.capacity() { *q_despawn = despawns; }

        let q_typed = self.typed_queues.get_mut().unwrap();
        if q_typed.is_empty() {
            *q_typed = queues;
        } else {
            for (tid, q) in queues {
                if !q_typed.iter().any(|(t, _)| *t == tid) {
                    q_typed.push((tid, q));
                }
            }
        }
    }

    pub fn save_to_json(&self) -> String {
        super::serialize::save_world_to_json(self)
    }

    pub fn load_from_json(&mut self, json: &str) {
        super::serialize::load_world_from_json(self, json);
    }

    pub fn save_to_json_impl(&self) -> String {
        let mut sw = super::serialize::SerializedWorld {
            current_tick: self.current_tick,
            structural_gen: self.structural_gen,
            next_entity_id: self.next_entity_id.load(Ordering::Relaxed),
            records: self.records.clone(),
            archetypes: Vec::new(),
        };
        for arch in &self.archetypes {
            let mut sa = super::serialize::SerializedArchetype {
                id: arch.id,
                signature: arch.signature.to_vec(),
                entities: arch.entities.clone(),
                columns: Vec::new(),
            };
            for &cid in &arch.signature[..] {
                if let Some(ser) = self.component_serializers.get(&cid) {
                    let mut sc = super::serialize::SerializedColumn { component_id: cid, data: Vec::new() };
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

    pub fn load_from_json_impl(&mut self, json: &str) {
        if let Ok(sw) = serde_json::from_str::<super::serialize::SerializedWorld>(json) {
            self.records = sw.records;
            self.current_tick = sw.current_tick;
            self.structural_gen = sw.structural_gen;
            self.next_entity_id.store(sw.next_entity_id, Ordering::Relaxed);

            self.archetypes.clear();
            self.active_archetypes.clear();
            self.archetype_index.clear();

            for sa in sw.archetypes {
                if !sa.entities.is_empty() {
                    self.active_archetypes.push(sa.id);
                }
                self.archetype_index.insert(sa.signature.clone().into_boxed_slice(), sa.id);

                let mut columns = Vec::new();
                for &cid in &sa.signature {
                    columns.push(UnsafeCell::new(Column::new(self.component_layouts[cid], self.component_drop_fns[cid], sa.entities.len().max(8))));
                }

                let mut component_to_column = vec![u32::MAX; self.component_layouts.len()];
                for (i, &cid) in sa.signature.iter().enumerate() {
                    component_to_column[cid] = i as u32;
                }

                let arch = Archetype {
                    id: sa.id,
                    last_modified_tick: AtomicU32::new(self.current_tick),
                    entities: sa.entities,
                    columns: columns.into_boxed_slice(),
                    signature: sa.signature.into_boxed_slice(),
                    component_to_column: component_to_column.into_boxed_slice(),
                    add_edges: Vec::new(),
                    remove_edges: Vec::new(),
                };

                for sc in sa.columns {
                    if let Some(deser) = self.component_deserializers.get(&sc.component_id) {
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
                self.archetypes.push(arch);
            }
        }
    }
}

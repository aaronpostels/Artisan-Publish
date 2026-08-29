use std::any::TypeId;
use std::marker::PhantomData;
use rayon::prelude::*;

use super::archetype::{Archetype, Column};
use super::query_filter::*;
use crate::ecs::component::Component;
use super::entity::Entity;
use super::system::{Access, SystemParam};
use super::world::World;

pub trait WorldQuery {
    type Item<'a>;
    type Fetcher<'a>: Copy;
    type State: Send + Sync;
    const COMPONENT_COUNT: usize;
    const HAS_WRITES: bool;
    fn required_components() -> Vec<TypeId>;
    fn reads() -> Vec<TypeId> { vec![] }
    fn writes() -> Vec<TypeId> { vec![] }
    fn init_state(world: &mut World) -> Self::State;
    unsafe fn init_fetcher<'a>(world: &'a World, arch: &'a Archetype, state: &Self::State, columns: &[*mut Column]) -> Self::Fetcher<'a>;
    unsafe fn fetch<'a>(fetcher: Self::Fetcher<'a>, row: usize) -> Self::Item<'a>;
}

impl WorldQuery for Entity {
    type Item<'a> = Entity;
    type Fetcher<'a> = *const Entity;
    type State = ();
    const COMPONENT_COUNT: usize = 0;
    const HAS_WRITES: bool = false;
    fn required_components() -> Vec<TypeId> { vec![] }
    #[inline(always)]
    fn init_state(_: &mut World) -> Self::State {}
    #[inline(always)]
    unsafe fn init_fetcher<'a>(_: &'a World, arch: &'a Archetype, _: &Self::State, _: &[*mut Column]) -> Self::Fetcher<'a> {
        arch.entities.as_ptr()
    }
    #[inline(always)]
    unsafe fn fetch<'a>(ptr: Self::Fetcher<'a>, row: usize) -> Self::Item<'a> {
        unsafe { *ptr.add(row) }
    }
}

impl<'q, T: Component> WorldQuery for &'q T {
    type Item<'a> = &'a T;
    type Fetcher<'a> = *const T;
    type State = usize;
    const COMPONENT_COUNT: usize = 1;
    const HAS_WRITES: bool = false;
    fn required_components() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    fn reads() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    #[inline(always)]
    fn init_state(world: &mut World) -> Self::State { world.register::<T>() }
    #[inline(always)]
    unsafe fn init_fetcher<'a>(_: &'a World, _arch: &'a Archetype, _state: &Self::State, columns: &[*mut Column]) -> Self::Fetcher<'a> {
        unsafe {
            let col_ptr = *columns.get_unchecked(0);
            let col = &*col_ptr;
            col.data.as_ptr::<T>()
        }
    }
    #[inline(always)]
    unsafe fn fetch<'a>(ptr: Self::Fetcher<'a>, row: usize) -> Self::Item<'a> {
        unsafe { &*ptr.add(row) }
    }
}

impl<'q, T: Component> WorldQuery for &'q mut T {
    type Item<'a> = &'a mut T;
    type Fetcher<'a> = *mut T;
    type State = usize;
    const COMPONENT_COUNT: usize = 1;
    const HAS_WRITES: bool = true;
    fn required_components() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    fn writes() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    #[inline(always)]
    fn init_state(world: &mut World) -> Self::State { world.register::<T>() }
    #[inline(always)]
    unsafe fn init_fetcher<'a>(world: &'a World, _arch: &'a Archetype, _state: &Self::State, columns: &[*mut Column]) -> Self::Fetcher<'a> {
        unsafe {
            let col_ptr = *columns.get_unchecked(0);
            let col = &mut *col_ptr;
            col.last_changed_tick.store(world.current_tick, std::sync::atomic::Ordering::Relaxed);
            col.data.as_mut_ptr::<T>()
        }
    }
    #[inline(always)]
    unsafe fn fetch<'a>(ptr: Self::Fetcher<'a>, row: usize) -> Self::Item<'a> {
        unsafe { &mut *ptr.add(row) }
    }
}

pub struct Tracked<T: Component>(pub PhantomData<T>);

pub struct Mut<'a, T: Component> {
    pub value: &'a mut T,
    pub tick: &'a mut u32,
    pub col_changed_tick: *const std::sync::atomic::AtomicU32,
    pub current_tick: u32,
}
impl<'a, T: Component> std::ops::Deref for Mut<'a, T> {
    type Target = T;
    #[inline(always)] fn deref(&self) -> &T { self.value }
}
impl<'a, T: Component> std::ops::DerefMut for Mut<'a, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        *self.tick = self.current_tick;
        unsafe { (*self.col_changed_tick).store(self.current_tick, std::sync::atomic::Ordering::Relaxed); }
        self.value
    }
}

impl<T: Component> WorldQuery for Tracked<T> {
    type Item<'a> = Mut<'a, T>;
    type Fetcher<'a> = (*mut T, *mut u32, *const std::sync::atomic::AtomicU32, u32);
    type State = usize;
    const COMPONENT_COUNT: usize = 1;
    const HAS_WRITES: bool = true;
    fn required_components() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    fn writes() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    #[inline(always)]
    fn init_state(world: &mut World) -> Self::State { world.register::<T>() }
    #[inline(always)]
    unsafe fn init_fetcher<'a>(world: &'a World, _arch: &'a Archetype, _state: &Self::State, columns: &[*mut Column]) -> Self::Fetcher<'a> {
        unsafe {
            let col_ptr = *columns.get_unchecked(0);
            let col = &mut *col_ptr;
            (
                col.data.as_mut_ptr::<T>(),
                col.changed_ticks.as_mut_ptr(),
                &col.last_changed_tick as *const std::sync::atomic::AtomicU32,
                world.current_tick,
            )
        }
    }
    #[inline(always)]
    unsafe fn fetch<'a>(fetcher: Self::Fetcher<'a>, row: usize) -> Self::Item<'a> {
        let (data, ticks, col_tick, cur) = fetcher;
        unsafe {
            Mut {
                value: &mut *data.add(row),
                tick: &mut *ticks.add(row),
                col_changed_tick: col_tick,
                current_tick: cur,
            }
        }
    }
}

macro_rules! impl_world_query_tuple {
    ($($name:ident),*) => {
        impl<$($name: WorldQuery),*> WorldQuery for ($($name,)*) {
            type Item<'a> = ($($name::Item<'a>,)*);
            type Fetcher<'a> = ($($name::Fetcher<'a>,)*);
            type State = ($($name::State,)*);

            const COMPONENT_COUNT: usize = 0 $(+ $name::COMPONENT_COUNT)*;
            const HAS_WRITES: bool = false $(|| $name::HAS_WRITES)*;

            #[allow(unused_mut)]
            fn required_components() -> Vec<TypeId> {
                let mut r = Vec::new();
                $(r.extend($name::required_components());)*
                r
            }
            #[allow(unused_mut)]
            fn reads() -> Vec<TypeId> {
                let mut r = Vec::new(); $(r.extend($name::reads());)* r
            }
            #[allow(unused_mut)]
            fn writes() -> Vec<TypeId> {
                let mut r = Vec::new(); $(r.extend($name::writes());)* r
            }

            #[allow(unused_variables)]
            #[inline(always)]
            fn init_state(world: &mut World) -> Self::State {
                ($($name::init_state(world),)*)
            }

            #[allow(unused_variables, unused_unsafe, non_snake_case)]
            #[inline(always)]
            unsafe fn init_fetcher<'a>(world: &'a World, arch: &'a Archetype, state: &Self::State, columns: &[*mut Column]) -> Self::Fetcher<'a> {
                let ($($name,)*) = state;
                let mut _offset = 0;
                unsafe { ($( {
                    let len = <$name as WorldQuery>::COMPONENT_COUNT;
                    let f = <$name as WorldQuery>::init_fetcher(world, arch, $name, &columns[_offset.._offset+len]);
                    _offset += len;
                    f
                },)*) }
            }

            #[allow(non_snake_case, unused_variables, unused_unsafe, clippy::unused_unit)]
            #[inline(always)]
            unsafe fn fetch<'a>(fetcher: Self::Fetcher<'a>, row: usize) -> Self::Item<'a> {
                let ($($name,)*) = fetcher;
                unsafe { ($($name::fetch($name, row),)*) }
            }
        }
    };
}

impl_world_query_tuple!();
impl_world_query_tuple!(P1);
impl_world_query_tuple!(P1, P2);
impl_world_query_tuple!(P1, P2, P3);
impl_world_query_tuple!(P1, P2, P3, P4);
impl_world_query_tuple!(P1, P2, P3, P4, P5, P6);

impl_world_query_tuple!(P1, P2, P3, P4, P5, P6, P7);
impl_world_query_tuple!(P1, P2, P3, P4, P5, P6, P7, P8);
impl_world_query_tuple!(P1, P2, P3, P4, P5, P6, P7, P8, P9);
impl_world_query_tuple!(P1, P2, P3, P4, P5, P6, P7, P8, P9, P10);
impl_world_query_tuple!(P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11);
impl_world_query_tuple!(P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12);

#[derive(Clone, Copy)]
pub struct MatchedArchetype {
    pub arch_index: u32,
    pub query_cols_start: u32,
    pub filter_cols_start: u32,
}

pub struct QueryState<Q: WorldQuery, F: QueryFilter> {
    pub required_components: Vec<TypeId>,
    pub exclude_components: Vec<TypeId>,
    pub query_comp_tids: Vec<TypeId>,
    pub filter_comp_tids: Vec<TypeId>,
    pub req_ids: Vec<usize>,
    pub exc_ids: Vec<usize>,
    pub query_ids: Vec<usize>,
    pub filter_ids: Vec<usize>,
    pub matched_archetypes: Vec<MatchedArchetype>,
    pub archetype_mapping: Vec<u32>,
    pub query_columns: Vec<*mut Column>,
    pub filter_columns: Vec<*mut Column>,
    pub last_archetype_count: usize,
    pub last_run_tick: u32,
    pub last_structural_gen: u64,
    pub initialized: bool,
    pub query_state: Q::State,
    pub filter_state: F::State,
}

unsafe impl<Q: WorldQuery, F: QueryFilter> Send for QueryState<Q, F> {}
unsafe impl<Q: WorldQuery, F: QueryFilter> Sync for QueryState<Q, F> {}

impl<Q: WorldQuery, F: QueryFilter> QueryState<Q, F> {
    pub fn new(world: &mut World, required: Vec<TypeId>, exclude: Vec<TypeId>) -> Self {
        Self {
            required_components: required,
            exclude_components: exclude,
            query_comp_tids: Q::required_components(),
            filter_comp_tids: F::required_components(),
            req_ids: Vec::new(),
            exc_ids: Vec::new(),
            query_ids: Vec::new(),
            filter_ids: Vec::new(),
            matched_archetypes: Vec::new(),
            archetype_mapping: Vec::new(),
            query_columns: Vec::new(),
            filter_columns: Vec::new(),
            last_archetype_count: 0,
            last_run_tick: 0,
            last_structural_gen: 0,
            initialized: false,
            query_state: Q::init_state(world),
            filter_state: F::init_state(world),
        }
    }

    pub fn update_cache(&mut self, world: &World) {
        if self.initialized && self.last_structural_gen == world.structural_gen {
            return;
        }

        if !self.initialized {
            self.req_ids.clear();
            for &tid in &self.required_components {
                if let Some(id) = world.get_component_id_by_type_id(tid) { self.req_ids.push(id); }
                else { return; }
            }
            self.exc_ids.clear();
            for &tid in &self.exclude_components {
                if let Some(id) = world.get_component_id_by_type_id(tid) { self.exc_ids.push(id); }
            }
            self.query_ids.clear();
            for &tid in &self.query_comp_tids {
                self.query_ids.push(world.get_component_id_by_type_id(tid).unwrap());
            }
            self.filter_ids.clear();
            for &tid in &self.filter_comp_tids {
                self.filter_ids.push(world.get_component_id_by_type_id(tid).unwrap());
            }
            self.initialized = true;
            self.last_archetype_count = 0;

            self.query_columns.clear();
            self.filter_columns.clear();
            self.matched_archetypes.clear();
        }

        if self.archetype_mapping.len() < world.archetypes.len() {
            self.archetype_mapping.resize(world.archetypes.len(), u32::MAX);
        }

        if self.last_archetype_count != world.archetypes.len() {
            let start = self.last_archetype_count;
            'outer: for arch in &world.archetypes[start..] {
                for &cid in &self.req_ids {
                    if cid >= arch.component_to_column.len() || unsafe { *arch.component_to_column.get_unchecked(cid) } == u32::MAX { continue 'outer; }
                }
                for &cid in &self.exc_ids {
                    if cid < arch.component_to_column.len() && unsafe { *arch.component_to_column.get_unchecked(cid) } != u32::MAX { continue 'outer; }
                }

                let q_start = self.query_columns.len() as u32;
                for &cid in &self.query_ids {
                    self.query_columns.push(unsafe { arch.columns.get_unchecked(*arch.component_to_column.get_unchecked(cid) as usize).get() });
                }

                let f_start = self.filter_columns.len() as u32;
                for &cid in &self.filter_ids {
                    self.filter_columns.push(unsafe { arch.columns.get_unchecked(*arch.component_to_column.get_unchecked(cid) as usize).get() });
                }

                let m_idx = self.matched_archetypes.len() as u32;
                self.matched_archetypes.push(MatchedArchetype {
                    arch_index: arch.id,
                    query_cols_start: q_start,
                    filter_cols_start: f_start,
                });
                self.archetype_mapping[arch.id as usize] = m_idx;
            }
            self.last_archetype_count = world.archetypes.len();
        }

        self.last_structural_gen = world.structural_gen;
    }
}

#[derive(Clone, Copy)]
pub struct PtrSendSync<T>(pub T);
impl<T: Copy> PtrSendSync<T> { #[inline(always)] pub fn get(&self) -> T { self.0 } }
unsafe impl<T> Send for PtrSendSync<T> {}
unsafe impl<T> Sync for PtrSendSync<T> {}

pub struct Query<'a, Q: WorldQuery, F: QueryFilter = ()> {
    world: &'a World,
    state: &'a mut QueryState<Q, F>,
    last_run_tick: u32,
    _marker: PhantomData<(Q, F)>,
}

impl<'q, Q: WorldQuery + 'static, F: QueryFilter + 'static> SystemParam for Query<'q, Q, F> {
    type State = QueryState<Q, F>;
    type Item<'i> = Query<'i, Q, F>;

    #[inline(always)]
    fn init_state(world: &mut World) -> Self::State {
        let mut req = Q::required_components();
        req.extend(F::required_components());
        QueryState::new(world, req, F::exclude_components())
    }

    #[inline(always)]
    fn get_access() -> Access {
        Access {
            reads: Q::reads().into_iter().collect(),
            writes: Q::writes().into_iter().collect(),
        }
    }

    #[inline(always)]
    unsafe fn get_param<'i>(state: &'i mut QueryState<Q, F>, world: &'i World) -> Query<'i, Q, F> {
        state.update_cache(world);
        let prev_tick = state.last_run_tick;
        state.last_run_tick = world.current_tick;
        Query { world, state, last_run_tick: prev_tick, _marker: PhantomData }
    }
}

impl<'a, Q: WorldQuery, F: QueryFilter> Query<'a, Q, F> {
    #[inline(always)]
    pub fn get(&self, entity: Entity) -> Option<Q::Item<'_>> {
        let id = entity.id as usize;
        if id >= self.world.records.len() { return None; }
        let rec = unsafe { self.world.records.get_unchecked(id) };
        if rec.generation != entity.generation || rec.archetype_id == u32::MAX { return None; }
        let m_idx = self.state.archetype_mapping[rec.archetype_id as usize];
        if m_idx == u32::MAX { return None; }
        let matched = unsafe { self.state.matched_archetypes.get_unchecked(m_idx as usize) };
        let arch = unsafe { self.world.archetypes.get_unchecked(rec.archetype_id as usize) };
        let q_cols = unsafe { self.state.query_columns.get_unchecked(matched.query_cols_start as usize .. matched.query_cols_start as usize + Q::COMPONENT_COUNT) };
        let fetcher = unsafe { Q::init_fetcher(self.world, arch, &self.state.query_state, q_cols) };
        Some(unsafe { Q::fetch(fetcher, rec.row as usize) })
    }

    #[inline(always)]
    pub fn get_by_id(&self, id: u32) -> Option<Q::Item<'_>> {
        let id = id as usize;
        if id >= self.world.records.len() { return None; }
        let rec = unsafe { self.world.records.get_unchecked(id) };
        if rec.archetype_id == u32::MAX { return None; }
        let m_idx = self.state.archetype_mapping[rec.archetype_id as usize];
        if m_idx == u32::MAX { return None; }
        let matched = unsafe { self.state.matched_archetypes.get_unchecked(m_idx as usize) };
        let arch = unsafe { self.world.archetypes.get_unchecked(rec.archetype_id as usize) };
        let q_cols = unsafe { self.state.query_columns.get_unchecked(matched.query_cols_start as usize .. matched.query_cols_start as usize + Q::COMPONENT_COUNT) };
        let fetcher = unsafe { Q::init_fetcher(self.world, arch, &self.state.query_state, q_cols) };
        Some(unsafe { Q::fetch(fetcher, rec.row as usize) })
    }

    #[inline(always)]
    pub fn get_mut(&mut self, entity: Entity) -> Option<Q::Item<'_>> {
        let id = entity.id as usize;
        if id >= self.world.records.len() { return None; }
        let rec = unsafe { *self.world.records.get_unchecked(id) };
        if rec.generation != entity.generation || rec.archetype_id == u32::MAX { return None; }

        let m_idx = self.state.archetype_mapping[rec.archetype_id as usize];
        if m_idx == u32::MAX { return None; }
        let matched = unsafe { self.state.matched_archetypes.get_unchecked(m_idx as usize) };
        let arch = unsafe { self.world.archetypes.get_unchecked(rec.archetype_id as usize) };
        if Q::HAS_WRITES { arch.last_modified_tick.store(self.world.current_tick, std::sync::atomic::Ordering::Relaxed); }
        let q_cols = unsafe { self.state.query_columns.get_unchecked(matched.query_cols_start as usize .. matched.query_cols_start as usize + Q::COMPONENT_COUNT) };
        let fetcher = unsafe { Q::init_fetcher(self.world, arch, &self.state.query_state, q_cols) };
        Some(unsafe { Q::fetch(fetcher, rec.row as usize) })
    }

    #[inline]
    pub fn len(&mut self) -> usize {
        if F::IS_ARCHETYPE_ONLY {
            let archetypes_ptr = self.world.archetypes.as_ptr();
            let mut total = 0usize;
            for matched in &self.state.matched_archetypes {
                let arch = unsafe { &*archetypes_ptr.add(matched.arch_index as usize) };
                total += arch.entities.len();
            }
            total
        } else {
            self.iter().count()
        }
    }

    #[inline]
    pub fn is_empty(&mut self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    pub fn for_each<FCb: FnMut(Q::Item<'_>)>(&mut self, mut f: FCb) {
        let archetypes_ptr = self.world.archetypes.as_ptr();
        for matched in &self.state.matched_archetypes {
            let arch = unsafe { &*archetypes_ptr.add(matched.arch_index as usize) };
            let len = arch.entities.len();
            if len == 0 { continue; }

            if !F::IS_ARCHETYPE_ONLY {
                let f_cols = unsafe { self.state.filter_columns.get_unchecked(matched.filter_cols_start as usize .. matched.filter_cols_start as usize + F::COMPONENT_COUNT) };
                if !unsafe { F::filter_arch(arch, self.last_run_tick, &self.state.filter_state, f_cols) } {
                    continue;
                }
            }

            if Q::HAS_WRITES {
                arch.last_modified_tick.store(self.world.current_tick, std::sync::atomic::Ordering::Relaxed);
            }

            let q_cols = unsafe { self.state.query_columns.get_unchecked(matched.query_cols_start as usize .. matched.query_cols_start as usize + Q::COMPONENT_COUNT) };
            let fetcher = unsafe { Q::init_fetcher(self.world, arch, &self.state.query_state, q_cols) };

            if F::IS_ARCHETYPE_ONLY {
                for row in 0..len {
                    unsafe { std::hint::assert_unchecked(row < len); }
                    f(unsafe { Q::fetch(fetcher, row) });
                }
            } else {
                let f_cols = unsafe { self.state.filter_columns.get_unchecked(matched.filter_cols_start as usize .. matched.filter_cols_start as usize + F::COMPONENT_COUNT) };
                let filter_fetcher = unsafe { F::init_fetcher(self.world, arch, self.last_run_tick, &self.state.filter_state, f_cols) };
                for row in 0..len {
                    unsafe { std::hint::assert_unchecked(row < len); }
                    if unsafe { F::filter(filter_fetcher, row) } {
                        f(unsafe { Q::fetch(fetcher, row) });
                    }
                }
            }
        }
    }

    #[inline(always)]
    pub fn par_for_each<FCb: Fn(Q::Item<'_>) + Send + Sync>(&mut self, f: FCb) {
        let last_run_tick = self.last_run_tick;
        let current_tick = self.world.current_tick;
        let matched = &self.state.matched_archetypes;
        let q_state = &self.state.query_state;
        let f_state = &self.state.filter_state;
        let archetypes_ptr = self.world.archetypes.as_ptr() as usize;
        let world_ptr = self.world as *const World as usize;
        let query_cols_ptr = self.state.query_columns.as_ptr() as usize;
        let query_cols_len = self.state.query_columns.len();
        let filter_cols_ptr = self.state.filter_columns.as_ptr() as usize;
        let filter_cols_len = self.state.filter_columns.len();
        let f_ref = &f;

        let num_threads = rayon::current_num_threads();

        matched.par_iter().for_each(|m| {
            let world = unsafe { &*(world_ptr as *const World) };
            let arch = unsafe { &*(archetypes_ptr as *const Archetype).add(m.arch_index as usize) };
            let len = arch.entities.len();
            if len == 0 { return; }

            if !F::IS_ARCHETYPE_ONLY {
                let filter_cols = unsafe { std::slice::from_raw_parts(filter_cols_ptr as *const *mut Column, filter_cols_len) };
                let f_cols_slice = unsafe { filter_cols.get_unchecked(m.filter_cols_start as usize .. m.filter_cols_start as usize + F::COMPONENT_COUNT) };
                if !unsafe { F::filter_arch(arch, last_run_tick, f_state, f_cols_slice) } {
                    return;
                }
            }

            if Q::HAS_WRITES {
                arch.last_modified_tick.store(current_tick, std::sync::atomic::Ordering::Relaxed);
            }

            let query_cols = unsafe { std::slice::from_raw_parts(query_cols_ptr as *const *mut Column, query_cols_len) };
            let q_cols_slice = unsafe { query_cols.get_unchecked(m.query_cols_start as usize .. m.query_cols_start as usize + Q::COMPONENT_COUNT) };
            let fetcher = unsafe { Q::init_fetcher(world, arch, q_state, q_cols_slice) };
            let wrapped_fetcher = PtrSendSync(fetcher);

            let min_len = (len / num_threads).max(4096);

            if F::IS_ARCHETYPE_ONLY {
                if len >= 256 {
                    (0..len).into_par_iter().with_min_len(min_len).for_each(|row| {
                        unsafe { std::hint::assert_unchecked(row < len); }
                        f_ref(unsafe { Q::fetch(wrapped_fetcher.get(), row) });
                    });
                } else {
                    for row in 0..len {
                        unsafe { std::hint::assert_unchecked(row < len); }
                        f_ref(unsafe { Q::fetch(wrapped_fetcher.get(), row) });
                    }
                }
            } else {
                let filter_cols = unsafe { std::slice::from_raw_parts(filter_cols_ptr as *const *mut Column, filter_cols_len) };
                let f_cols_slice = unsafe { filter_cols.get_unchecked(m.filter_cols_start as usize .. m.filter_cols_start as usize + F::COMPONENT_COUNT) };
                let filter_fetcher = unsafe { F::init_fetcher(self.world, arch, last_run_tick, f_state, f_cols_slice) };
                let wrapped_filter = PtrSendSync(filter_fetcher);

                if len >= 256 {
                    (0..len).into_par_iter().with_min_len(min_len).for_each(|row| {
                        unsafe { std::hint::assert_unchecked(row < len); }
                        if unsafe { F::filter(wrapped_filter.get(), row) } {
                            f_ref(unsafe { Q::fetch(wrapped_fetcher.get(), row) });
                        }
                    });
                } else {
                    for row in 0..len {
                        unsafe { std::hint::assert_unchecked(row < len); }
                        if unsafe { F::filter(wrapped_filter.get(), row) } {
                            f_ref(unsafe { Q::fetch(wrapped_fetcher.get(), row) });
                        }
                    }
                }
            }
        });
    }

    #[inline(always)]
    pub fn iter(&mut self) -> QueryIter<'_, Q, F> {
        QueryIter {
            world: self.world,
            matched_archetypes: &self.state.matched_archetypes,
            query_columns: &self.state.query_columns,
            filter_columns: &self.state.filter_columns,
            arch_idx: 0,
            row_idx: 0,
            current_len: 0,
            last_run_tick: self.last_run_tick,
            query_state: &self.state.query_state,
            filter_state: &self.state.filter_state,
            fetcher: None,
            filter_fetcher: None,
        }
    }
}

pub struct QueryIter<'a, Q: WorldQuery, F: QueryFilter> {
    world: &'a World,
    matched_archetypes: &'a[MatchedArchetype],
    query_columns: &'a [*mut Column],
    filter_columns: &'a[*mut Column],
    arch_idx: usize,
    row_idx: usize,
    current_len: usize,
    last_run_tick: u32,
    query_state: &'a Q::State,
    filter_state: &'a F::State,
    fetcher: Option<Q::Fetcher<'a>>,
    filter_fetcher: Option<F::Fetcher<'a>>,
}

impl<'a, Q: WorldQuery, F: QueryFilter> QueryIter<'a, Q, F> {
    #[inline(always)]
    fn next_arch(&mut self) -> bool {
        let archetypes_ptr = self.world.archetypes.as_ptr();
        while self.arch_idx < self.matched_archetypes.len() {
            let matched = unsafe { self.matched_archetypes.get_unchecked(self.arch_idx) };
            self.arch_idx += 1;
            let arch = unsafe { &*archetypes_ptr.add(matched.arch_index as usize) };

            self.current_len = arch.entities.len();
            if self.current_len == 0 { continue; }

            if !F::IS_ARCHETYPE_ONLY {
                let f_cols = unsafe { self.filter_columns.get_unchecked(matched.filter_cols_start as usize .. matched.filter_cols_start as usize + F::COMPONENT_COUNT) };
                if !unsafe { F::filter_arch(arch, self.last_run_tick, self.filter_state, f_cols) } {
                    continue;
                }
            }

            self.row_idx = 0;

            if Q::HAS_WRITES { arch.last_modified_tick.store(self.world.current_tick, std::sync::atomic::Ordering::Relaxed); }
            let q_cols = unsafe { self.query_columns.get_unchecked(matched.query_cols_start as usize .. matched.query_cols_start as usize + Q::COMPONENT_COUNT) };
            self.fetcher = Some(unsafe { Q::init_fetcher(self.world, arch, self.query_state, q_cols) });
            if !F::IS_ARCHETYPE_ONLY {
                let f_cols = unsafe { self.filter_columns.get_unchecked(matched.filter_cols_start as usize .. matched.filter_cols_start as usize + F::COMPONENT_COUNT) };
                self.filter_fetcher = Some(unsafe { F::init_fetcher(self.world, arch, self.last_run_tick, self.filter_state, f_cols) });
            }
            return true;
        }
        false
    }
}

impl<'a, Q: WorldQuery, F: QueryFilter> Iterator for QueryIter<'a, Q, F> {
    type Item = Q::Item<'a>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.row_idx < self.current_len {
                let row = self.row_idx;
                self.row_idx += 1;
                let filter_pass = F::IS_ARCHETYPE_ONLY || unsafe { F::filter(self.filter_fetcher.unwrap_unchecked(), row) };
                if filter_pass {
                    return Some(unsafe { Q::fetch(self.fetcher.unwrap_unchecked(), row) });
                }
                continue;
            }
            if !self.next_arch() { return None; }
        }
    }

    #[inline(always)]
    fn fold<B, Fold>(mut self, mut init: B, mut f: Fold) -> B
    where
        Fold: FnMut(B, Self::Item) -> B,
    {
        while self.row_idx < self.current_len || self.next_arch() {
            let len = self.current_len;
            if F::IS_ARCHETYPE_ONLY {
                let fetcher = unsafe { self.fetcher.unwrap_unchecked() };
                for row in self.row_idx..len {
                    unsafe { std::hint::assert_unchecked(row < len); }
                    init = f(init, unsafe { Q::fetch(fetcher, row) });
                }
            } else {
                let fetcher = unsafe { self.fetcher.unwrap_unchecked() };
                let filter_fetcher = unsafe { self.filter_fetcher.unwrap_unchecked() };
                for row in self.row_idx..len {
                    unsafe { std::hint::assert_unchecked(row < len); }
                    if unsafe { F::filter(filter_fetcher, row) } {
                        init = f(init, unsafe { Q::fetch(fetcher, row) });
                    }
                }
            }
            self.row_idx = len;
        }
        init
    }

    #[inline(always)]
    fn count(mut self) -> usize {
        if F::IS_ARCHETYPE_ONLY {
            let mut c = self.current_len - self.row_idx;
            let archetypes_ptr = self.world.archetypes.as_ptr();
            while self.arch_idx < self.matched_archetypes.len() {
                let matched = unsafe { self.matched_archetypes.get_unchecked(self.arch_idx) };
                self.arch_idx += 1;
                let arch = unsafe { &*archetypes_ptr.add(matched.arch_index as usize) };
                c += arch.entities.len();
            }
            c
        } else {
            self.fold(0, |acc, _| acc + 1)
        }
    }
}

impl<'q, T: Component> WorldQuery for Option<&'q T> {
    type Item<'a> = Option<&'a T>;
    type Fetcher<'a> = (bool, *const T);
    type State = usize;
    const COMPONENT_COUNT: usize = 0;
    const HAS_WRITES: bool = false;
    fn required_components() -> Vec<TypeId> { vec![] }
    fn reads() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    fn writes() -> Vec<TypeId> { vec![] }
    #[inline(always)] fn init_state(world: &mut World) -> Self::State { world.register::<T>() }
    #[inline(always)] unsafe fn init_fetcher<'a>(_: &'a World, arch: &'a Archetype, state: &Self::State, _: &[*mut Column]) -> Self::Fetcher<'a> {
        let cid = *state;
        if cid < arch.component_to_column.len() && arch.component_to_column[cid] != u32::MAX {
            unsafe { (true, (*arch.columns[arch.component_to_column[cid] as usize].get()).data.as_ptr::<T>()) }
        } else { (false, std::ptr::null()) }
    }
    #[inline(always)] unsafe fn fetch<'a>(ptr: Self::Fetcher<'a>, row: usize) -> Self::Item<'a> {
        if ptr.0 { unsafe { Some(&*ptr.1.add(row)) } } else { None }
    }
}

impl<'q, T: Component> WorldQuery for Option<&'q mut T> {
    type Item<'a> = Option<&'a mut T>;
    type Fetcher<'a> = (bool, *mut T);
    type State = usize;
    const COMPONENT_COUNT: usize = 0;
    const HAS_WRITES: bool = true;
    fn required_components() -> Vec<TypeId> { vec![] }
    fn reads() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    fn writes() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    #[inline(always)] fn init_state(world: &mut World) -> Self::State { world.register::<T>() }
    #[inline(always)] unsafe fn init_fetcher<'a>(w: &'a World, arch: &'a Archetype, state: &Self::State, _: &[*mut Column]) -> Self::Fetcher<'a> {
        let cid = *state;
        if cid < arch.component_to_column.len() && arch.component_to_column[cid] != u32::MAX {
            unsafe {
                let col = &mut *arch.columns[arch.component_to_column[cid] as usize].get();
                col.last_changed_tick.store(w.current_tick, std::sync::atomic::Ordering::Relaxed);
                (true, col.data.as_mut_ptr::<T>())
            }
        } else { (false, std::ptr::null_mut()) }
    }
    #[inline(always)] unsafe fn fetch<'a>(ptr: Self::Fetcher<'a>, row: usize) -> Self::Item<'a> {
        if ptr.0 { unsafe { Some(&mut *ptr.1.add(row)) } } else { None }
    }
}

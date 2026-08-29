use std::any::TypeId;
use std::marker::PhantomData;
use super::archetype::{Archetype, Column};
use crate::ecs::component::Component;
use super::world::World;

pub trait QueryFilter {
    type Fetcher<'a>: Copy;
    type State: Send + Sync;
    const IS_ARCHETYPE_ONLY: bool;
    const COMPONENT_COUNT: usize;
    fn required_components() -> Vec<TypeId>;
    fn exclude_components() -> Vec<TypeId>;
    fn init_state(world: &mut World) -> Self::State;
    unsafe fn init_fetcher<'a>(
        world: &'a World,
        arch: &'a Archetype,
        last_run_tick: u32,
        state: &Self::State,
        columns: &[*mut Column],
    ) -> Self::Fetcher<'a>;
    #[inline(always)]
    unsafe fn filter_arch(_arch: &Archetype, _last_run_tick: u32, _state: &Self::State, _columns: &[*mut Column]) -> bool { true }
    unsafe fn filter<'a>(fetcher: Self::Fetcher<'a>, row: usize) -> bool;
}

impl QueryFilter for () {
    type Fetcher<'a> = ();
    type State = ();
    const IS_ARCHETYPE_ONLY: bool = true;
    const COMPONENT_COUNT: usize = 0;
    fn required_components() -> Vec<TypeId> { vec![] }
    fn exclude_components() -> Vec<TypeId> { vec![] }
    #[inline(always)]
    fn init_state(_: &mut World) -> Self::State {}
    #[inline(always)]
    unsafe fn init_fetcher<'a>(_: &'a World, _: &'a Archetype, _: u32, _: &Self::State, _: &[*mut Column]) -> Self::Fetcher<'a> {}
    #[inline(always)]
    unsafe fn filter<'a>(_: Self::Fetcher<'a>, _: usize) -> bool { true }
}

pub struct With<T>(PhantomData<T>);
impl<T: Component> QueryFilter for With<T> {
    type Fetcher<'a> = ();
    type State = ();
    const IS_ARCHETYPE_ONLY: bool = true;
    const COMPONENT_COUNT: usize = 1;
    fn required_components() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    fn exclude_components() -> Vec<TypeId> { vec![] }
    #[inline(always)]
    fn init_state(_: &mut World) -> Self::State {}
    #[inline(always)]
    unsafe fn init_fetcher<'a>(_: &'a World, _: &'a Archetype, _: u32, _: &Self::State, _: &[*mut Column]) -> Self::Fetcher<'a> {}
    #[inline(always)]
    unsafe fn filter<'a>(_: Self::Fetcher<'a>, _: usize) -> bool { true }
}

pub struct Without<T>(PhantomData<T>);
impl<T: Component> QueryFilter for Without<T> {
    type Fetcher<'a> = ();
    type State = ();
    const IS_ARCHETYPE_ONLY: bool = true;
    const COMPONENT_COUNT: usize = 0;
    fn required_components() -> Vec<TypeId> { vec![] }
    fn exclude_components() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    #[inline(always)]
    fn init_state(_: &mut World) -> Self::State {}
    #[inline(always)]
    unsafe fn init_fetcher<'a>(_: &'a World, _: &'a Archetype, _: u32, _: &Self::State, _: &[*mut Column]) -> Self::Fetcher<'a> {}
    #[inline(always)]
    unsafe fn filter<'a>(_: Self::Fetcher<'a>, _: usize) -> bool { true }
}

pub struct Added<T>(PhantomData<T>);
impl<T: Component> QueryFilter for Added<T> {
    type Fetcher<'a> = (*const u32, u32);
    type State = usize;
    const IS_ARCHETYPE_ONLY: bool = false;
    const COMPONENT_COUNT: usize = 1;
    fn required_components() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    fn exclude_components() -> Vec<TypeId> { vec![] }
    #[inline(always)]
    fn init_state(world: &mut World) -> Self::State { world.register::<T>() }
    #[inline(always)]
    unsafe fn init_fetcher<'a>(world: &'a World, _arch: &'a Archetype, _: u32, _state: &Self::State, columns: &[*mut Column]) -> Self::Fetcher<'a> {
        unsafe {
            let col_ptr = *columns.get_unchecked(0);
            let col = &*col_ptr;
            (col.added_ticks.as_ptr(), world.current_tick)
        }
    }
    #[inline(always)]
    unsafe fn filter_arch(_arch: &Archetype, last_run_tick: u32, _state: &Self::State, columns: &[*mut Column]) -> bool {
        let col = unsafe { &**columns.get_unchecked(0) };
        col.last_added_tick.load(std::sync::atomic::Ordering::Relaxed) > last_run_tick
    }
    #[inline(always)]
    unsafe fn filter<'a>(fetcher: Self::Fetcher<'a>, row: usize) -> bool {
        let (ticks_ptr, cur) = fetcher;
        unsafe { *ticks_ptr.add(row) == cur }
    }
}

pub struct Changed<T>(PhantomData<T>);
impl<T: Component> QueryFilter for Changed<T> {
    type Fetcher<'a> = (*const u32, u32);
    type State = usize;
    const IS_ARCHETYPE_ONLY: bool = false;
    const COMPONENT_COUNT: usize = 1;
    fn required_components() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
    fn exclude_components() -> Vec<TypeId> { vec![] }
    #[inline(always)]
    fn init_state(world: &mut World) -> Self::State { world.register::<T>() }
    #[inline(always)]
    unsafe fn init_fetcher<'a>(_world: &'a World, _arch: &'a Archetype, last_run_tick: u32, _state: &Self::State, columns: &[*mut Column]) -> Self::Fetcher<'a> {
        unsafe {
            let col_ptr = *columns.get_unchecked(0);
            let col = &*col_ptr;
            (col.changed_ticks.as_ptr(), last_run_tick)
        }
    }
    #[inline(always)]
    unsafe fn filter_arch(_arch: &Archetype, last_run_tick: u32, _state: &Self::State, columns: &[*mut Column]) -> bool {
        let col = unsafe { &**columns.get_unchecked(0) };
        col.last_changed_tick.load(std::sync::atomic::Ordering::Relaxed) > last_run_tick
    }
    #[inline(always)]
    unsafe fn filter<'a>(fetcher: Self::Fetcher<'a>, row: usize) -> bool {
        let (ticks_ptr, last) = fetcher;
        unsafe { *ticks_ptr.add(row) > last }
    }
}

impl<A: QueryFilter, B: QueryFilter> QueryFilter for (A, B) {
    type Fetcher<'a> = (A::Fetcher<'a>, B::Fetcher<'a>);
    type State = (A::State, B::State);
    const IS_ARCHETYPE_ONLY: bool = A::IS_ARCHETYPE_ONLY && B::IS_ARCHETYPE_ONLY;
    const COMPONENT_COUNT: usize = A::COMPONENT_COUNT + B::COMPONENT_COUNT;
    fn required_components() -> Vec<TypeId> {
        let mut r = A::required_components();
        r.extend(B::required_components());
        r
    }
    fn exclude_components() -> Vec<TypeId> {
        let mut r = A::exclude_components();
        r.extend(B::exclude_components());
        r
    }
    #[inline(always)]
    fn init_state(world: &mut World) -> Self::State {
        (A::init_state(world), B::init_state(world))
    }
    #[inline(always)]
    unsafe fn init_fetcher<'a>(world: &'a World, arch: &'a Archetype, last: u32, state: &Self::State, columns: &[*mut Column]) -> Self::Fetcher<'a> {
        let (sa, sb) = state;
        let a_count = A::COMPONENT_COUNT;
        unsafe { (A::init_fetcher(world, arch, last, sa, &columns[..a_count]), B::init_fetcher(world, arch, last, sb, &columns[a_count..])) }
    }
    #[inline(always)]
    unsafe fn filter_arch(arch: &Archetype, last_run_tick: u32, state: &Self::State, columns: &[*mut Column]) -> bool {
        let (sa, sb) = state;
        let a_count = A::COMPONENT_COUNT;
        unsafe { A::filter_arch(arch, last_run_tick, sa, &columns[..a_count]) && B::filter_arch(arch, last_run_tick, sb, &columns[a_count..]) }
    }
    #[inline(always)]
    unsafe fn filter<'a>(fetcher: Self::Fetcher<'a>, row: usize) -> bool {
        let (fa, fb) = fetcher;
        unsafe { A::filter(fa, row) && B::filter(fb, row) }
    }
}

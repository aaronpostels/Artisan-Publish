use std::any::TypeId;
use std::collections::HashSet;
use std::ops::{Deref, DerefMut};
use crate::ecs::component::Resource;
use super::world::World;

#[derive(Default, Clone)]
pub struct Access {
    pub reads: HashSet<TypeId>,
    pub writes: HashSet<TypeId>,
}

impl Access {
    #[inline(always)]
    pub fn is_compatible_with(&self, other: &Access) -> bool {
        self.writes.is_disjoint(&other.reads)
            && self.reads.is_disjoint(&other.writes)
            && self.writes.is_disjoint(&other.writes)
    }

    #[inline(always)]
    pub fn merge(&mut self, other: &Access) {
        self.reads.extend(other.reads.iter().copied());
        self.writes.extend(other.writes.iter().copied());
    }
}

pub trait System: Send + Sync {
    fn run(&mut self, world: &World);
    fn access(&self) -> Access;
}

pub trait IntoSystem<Marker> {
    type System: System + 'static;
    fn into_system(self, world: &mut World) -> Self::System;
}

pub trait SystemParam {
    type State: Send + Sync + 'static;
    type Item<'a>;

    fn init_state(world: &mut World) -> Self::State;
    fn get_access() -> Access;
    unsafe fn get_param<'a>(state: &'a mut Self::State, world: &'a World) -> Self::Item<'a>;
}

impl<'a> SystemParam for &'a World {
    type State = ();
    type Item<'i> = &'i World;
    #[inline(always)] fn init_state(_: &mut World) {}
    #[inline(always)] fn get_access() -> Access { Access::default() }
    #[inline(always)] unsafe fn get_param<'i>(_: &'i mut (), world: &'i World) -> Self::Item<'i> { world }
}

pub struct Res<'a, T: Resource> {
    value: &'a T,
}

impl<'a, T: Resource> Deref for Res<'a, T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'res, T: Resource> SystemParam for Res<'res, T> {
    type State = ();
    type Item<'a> = Res<'a, T>;

    #[inline(always)]
    fn init_state(_: &mut World) {}

    #[inline(always)]
    fn get_access() -> Access {
        let mut reads = HashSet::new();
        reads.insert(TypeId::of::<T>());
        Access { reads, writes: HashSet::new() }
    }

    #[inline(always)]
    unsafe fn get_param<'a>(_: &'a mut (), world: &'a World) -> Self::Item<'a> {
        Res {
            value: world.get_resource::<T>().unwrap(),
        }
    }
}

pub struct ResMut<'a, T: Resource> {
    value: &'a mut T,
}

impl<'a, T: Resource> Deref for ResMut<'a, T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'a, T: Resource> DerefMut for ResMut<'a, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<'res, T: Resource> SystemParam for ResMut<'res, T> {
    type State = ();
    type Item<'a> = ResMut<'a, T>;

    #[inline(always)]
    fn init_state(_: &mut World) {}

    #[inline(always)]
    fn get_access() -> Access {
        let mut writes = HashSet::new();
        writes.insert(TypeId::of::<T>());
        Access { reads: HashSet::new(), writes }
    }

    #[inline(always)]
    unsafe fn get_param<'a>(_: &'a mut (), world: &'a World) -> Self::Item<'a> {
        ResMut {
            value: unsafe { &mut *world.get_resource_mut_ptr::<T>().unwrap() },
        }
    }
}

pub struct FunctionSystem<Func, P: SystemParam> {
    func: Func,
    state: P::State,
}

macro_rules! impl_into_system {
    ($($name:ident),*) => {
        impl<$($name: SystemParam + 'static),*> SystemParam for ($($name,)*) {
            type State = ($($name::State,)*);
            type Item<'a> = ($($name::Item<'a>,)*);

            #[allow(unused_variables)]
            #[inline(always)]
            fn init_state(world: &mut World) -> Self::State {
                ($($name::init_state(world),)*)
            }

            #[allow(unused_mut)]
            #[inline(always)]
            fn get_access() -> Access {
                let mut access = Access::default();
                $(access.merge(&$name::get_access());)*
                access
            }

            #[allow(non_snake_case, clippy::unused_unit, unused_unsafe, unused_variables)]
            #[inline(always)]
            unsafe fn get_param<'a>(state: &'a mut Self::State, world: &'a World) -> Self::Item<'a> {
                let ($($name,)*) = state;
                unsafe { ($($name::get_param($name, world),)*) }
            }
        }

        #[allow(non_snake_case)]
        impl<Func, $($name: SystemParam + 'static),*> System for FunctionSystem<Func, ($($name,)*)>
        where
            for<'a> Func: FnMut($($name::Item<'a>),*) + Send + Sync,
            <($($name,)*) as SystemParam>::State: Send + Sync,
        {
            #[inline(always)]
            fn run(&mut self, world: &World) {
                let ($($name,)*) = unsafe { <($($name,)*) as SystemParam>::get_param(&mut self.state, world) };
                (self.func)($($name),*);
            }

            #[inline(always)]
            fn access(&self) -> Access {
                <($($name,)*) as SystemParam>::get_access()
            }
        }

        #[allow(non_snake_case)]
        impl<Func, $($name: SystemParam + 'static),*> IntoSystem<fn($($name,)*)> for Func
        where
            for<'a> Func: FnMut($($name::Item<'a>),*) + Send + Sync + 'static,
            Func: FnMut($($name),*),
            <($($name,)*) as SystemParam>::State: Send + Sync,
        {
            type System = FunctionSystem<Func, ($($name,)*)>;

            #[inline(always)]
            fn into_system(self, world: &mut World) -> Self::System {
                FunctionSystem {
                    func: self,
                    state: <($($name,)*) as SystemParam>::init_state(world),
                }
            }
        }
    };
}

impl_into_system!();
impl_into_system!(P1);
impl_into_system!(P1, P2);
impl_into_system!(P1, P2, P3);
impl_into_system!(P1, P2, P3, P4);
impl_into_system!(P1, P2, P3, P4, P5);
impl_into_system!(P1, P2, P3, P4, P5, P6);
impl_into_system!(P1, P2, P3, P4, P5, P6, P7);
impl_into_system!(P1, P2, P3, P4, P5, P6, P7, P8);

pub struct Events<T> {
    pub current: Vec<T>,
    pub previous: Vec<T>,
}
impl<T> Default for Events<T> {
    fn default() -> Self { Self { current: Vec::new(), previous: Vec::new() } }
}
impl<T> Events<T> {
    pub fn update(&mut self) {
        self.previous.clear();
        std::mem::swap(&mut self.current, &mut self.previous);
    }
}

pub struct EventWriter<'a, T: Resource> {
    events: &'a mut Events<T>,
}
impl<'a, T: Resource> EventWriter<'a, T> {
    #[inline(always)] pub fn send(&mut self, event: T) { self.events.current.push(event); }
}
impl<'a, T: Resource> SystemParam for EventWriter<'a, T> {
    type State = ();
    type Item<'i> = EventWriter<'i, T>;
    fn init_state(world: &mut World) {
        if world.get_resource::<Events<T>>().is_none() {
            world.insert_resource(Events::<T>::default());
        }
    }
    fn get_access() -> Access {
        let mut writes = std::collections::HashSet::new();
        writes.insert(TypeId::of::<Events<T>>());
        Access { reads: std::collections::HashSet::new(), writes }
    }
    unsafe fn get_param<'i>(_: &'i mut (), world: &'i World) -> Self::Item<'i> {
        EventWriter { events: unsafe { &mut *world.get_resource_mut_ptr::<Events<T>>().unwrap() } }
    }
}

pub struct EventReader<'a, T: Resource> {
    events: &'a Events<T>,
}
impl<'a, T: Resource> EventReader<'a, T> {
    #[inline(always)] pub fn read(&self) -> impl Iterator<Item = &T> {
        self.events.previous.iter().chain(self.events.current.iter())
    }
}
impl<'a, T: Resource> SystemParam for EventReader<'a, T> {
    type State = ();
    type Item<'i> = EventReader<'i, T>;
    fn init_state(world: &mut World) {
        if world.get_resource::<Events<T>>().is_none() {
            world.insert_resource(Events::<T>::default());
        }
    }
    fn get_access() -> Access {
        let mut reads = std::collections::HashSet::new();
        reads.insert(TypeId::of::<Events<T>>());
        Access { reads, writes: std::collections::HashSet::new() }
    }
    unsafe fn get_param<'i>(_: &'i mut (), world: &'i World) -> Self::Item<'i> {
        EventReader { events: world.get_resource::<Events<T>>().unwrap() }
    }
}

pub struct SystemNode { pub system: Box<dyn System>, pub name: &'static str, pub labels: Vec<&'static str>, pub before: Vec<&'static str>, pub after: Vec<&'static str>, }
pub trait IntoSystemConfig<Marker> {
    fn into_config(self, world: &mut World) -> SystemNode;
    fn run_if(self, condition: Box<dyn Fn(&World) -> bool + Send + Sync>) -> SystemConfigWrapper<Self> where Self: Sized {
        SystemConfigWrapper { system: self, labels: vec![], before: vec![], after: vec![], condition: Some(condition) }
    }
}
pub struct SystemConfigWrapper<T> { pub system: T, pub labels: Vec<&'static str>, pub before: Vec<&'static str>, pub after: Vec<&'static str>, pub condition: Option<Box<dyn Fn(&World) -> bool + Send + Sync>>, }
struct ConditionalSystem { condition: Box<dyn Fn(&World) -> bool + Send + Sync>, inner: Box<dyn System> }
impl System for ConditionalSystem {
    fn run(&mut self, w: &World) { if (self.condition)(w) { self.inner.run(w); } }
    fn access(&self) -> Access { self.inner.access() }
}
impl<M, T: IntoSystem<M>> IntoSystemConfig<(M,)> for T {
    fn into_config(self, world: &mut World) -> SystemNode {

        SystemNode { system: Box::new(self.into_system(world)), name: std::any::type_name::<T>(), labels: vec![], before: vec![], after: vec![] }
    }
}
impl<M, T: IntoSystemConfig<M>> IntoSystemConfig<(M, bool)> for SystemConfigWrapper<T> {
    fn into_config(mut self, world: &mut World) -> SystemNode {
        let mut node = self.system.into_config(world);
        node.labels.extend(self.labels);
        node.before.extend(self.before);
        node.after.extend(self.after);
        if let Some(cond) = self.condition.take() {
            let inner = node.system;
            node.system = Box::new(ConditionalSystem { condition: cond, inner });
        }
        node
    }
}
pub trait SystemConfig<M>: Sized {
    fn before(self, label: &'static str) -> SystemConfigWrapper<Self>;
    fn after(self, label: &'static str) -> SystemConfigWrapper<Self>;
    fn label(self, label: &'static str) -> SystemConfigWrapper<Self>;
    fn run_if(self, condition: Box<dyn Fn(&World) -> bool + Send + Sync>) -> SystemConfigWrapper<Self>;
}
impl<Marker, T: IntoSystem<Marker>> SystemConfig<Marker> for T {
    fn before(self, label: &'static str) -> SystemConfigWrapper<Self> { SystemConfigWrapper { system: self, labels: vec![], before: vec![label], after: vec![], condition: None } }
    fn after(self, label: &'static str) -> SystemConfigWrapper<Self> { SystemConfigWrapper { system: self, labels: vec![], before: vec![], after: vec![label], condition: None } }
    fn label(self, label: &'static str) -> SystemConfigWrapper<Self> { SystemConfigWrapper { system: self, labels: vec![label], before: vec![], after: vec![], condition: None } }
    fn run_if(self, condition: Box<dyn Fn(&World) -> bool + Send + Sync>) -> SystemConfigWrapper<Self> { SystemConfigWrapper { system: self, labels: vec![], before: vec![], after: vec![], condition: Some(condition) } }
}
impl<T> SystemConfigWrapper<T> {
    pub fn before(mut self, label: &'static str) -> Self { self.before.push(label); self }
    pub fn after(mut self, label: &'static str) -> Self { self.after.push(label); self }
    pub fn label(mut self, label: &'static str) -> Self { self.labels.push(label); self }
    pub fn run_if(mut self, condition: Box<dyn Fn(&World) -> bool + Send + Sync>) -> Self { self.condition = Some(condition); self }
}

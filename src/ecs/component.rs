use super::world::World;
use super::archetype::Archetype;
use super::entity::Entity;
pub trait Component: 'static + Sized {}
impl<T: 'static + Sized> Component for T {}
pub trait Resource: 'static + Sized {}
impl<T: 'static + Sized> Resource for T {}
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)] pub struct Parent(pub Entity);
#[derive(Clone, serde::Serialize, serde::Deserialize)] pub struct Children(pub Vec<Entity>);
pub trait Bundle: 'static + Sized {
    fn tuple_component_ids(world: &mut World) -> Vec<usize>;
    unsafe fn insert_components(self, arch: &mut Archetype, col_indices: &[usize], current_tick: u32);
}
macro_rules! impl_bundle {
    ($($name:ident),*) => {
        impl<$($name: Component),*> Bundle for ($($name,)*) {
            #[allow(unused_mut, unused_variables)]
            fn tuple_component_ids(world: &mut World) -> Vec<usize> {
                vec![$(world.register::<$name>()),*]
            }
            #[allow(unused_variables, unused_unsafe, unused_mut, non_snake_case)]
            unsafe fn insert_components(self, arch: &mut Archetype, col_indices: &[usize], current_tick: u32) {
                let ($($name,)*) = self;
                let mut _idx = 0;
                $(
                    let col_idx = unsafe { *col_indices.get_unchecked(_idx) };
                    let col = unsafe { &mut *arch.columns.get_unchecked(col_idx).get() };
                    col.data.push_typed($name);
                    col.added_ticks.push(current_tick);
                    col.changed_ticks.push(current_tick);
                    col.last_added_tick.store(current_tick, std::sync::atomic::Ordering::Relaxed);
                    col.last_changed_tick.store(current_tick, std::sync::atomic::Ordering::Relaxed);
                    _idx += 1;
                )*
            }
        }
    };
}
impl_bundle!();
impl_bundle!(P1);
impl_bundle!(P1, P2);
impl_bundle!(P1, P2, P3);
impl_bundle!(P1, P2, P3, P4);
impl_bundle!(P1, P2, P3, P4, P5);
impl_bundle!(P1, P2, P3, P4, P5, P6);
impl_bundle!(P1, P2, P3, P4, P5, P6, P7);
impl_bundle!(P1, P2, P3, P4, P5, P6, P7, P8);
impl_bundle!(P1, P2, P3, P4, P5, P6, P7, P8, P9);
impl_bundle!(P1, P2, P3, P4, P5, P6, P7, P8, P9, P10);

use crate::ecs::component::Component;
use super::entity::Entity;
use super::system::SystemParam;
use super::world::{AnyQueue, Op, TypedQueue, World};
use std::sync::atomic::Ordering;
use std::any::TypeId;

#[derive(Default)]
pub struct CommandQueue {
    pub spawns: Vec<Entity>,
    pub despawns: Vec<Entity>,
    pub typed_queues: Vec<(TypeId, Box<dyn AnyQueue>)>,
}

pub struct Commands<'a> {
    pub(crate) world: &'a World,
    pub(crate) queue: &'a mut CommandQueue,
}

pub struct EntityCommands<'a, 'b> {
    entity: Entity,
    commands: &'b mut Commands<'a>,
}

impl<'a, 'b> EntityCommands<'a, 'b> {
    pub fn id(&self) -> Entity {
        self.entity
    }

    pub fn insert<T: Component + Send + Sync + 'static>(self, component: T) -> Self {
        let entity = self.entity;
        self.commands.insert(entity, component);
        self
    }

    pub fn remove<T: Component + Send + Sync + 'static>(self) -> Self {
        let entity = self.entity;
        self.commands.remove::<T>(entity);
        self
    }

    pub fn despawn(self) {
        let entity = self.entity;
        self.commands.despawn(entity);
    }
}

impl<'a> Commands<'a> {
    pub fn spawn(&mut self) -> EntityCommands<'a, '_> {
        let entity = self.world.reserve_entity();
        self.queue.spawns.push(entity);
        EntityCommands {
            entity,
            commands: self,
        }
    }

    pub fn spawn_batch(&mut self, count: u32) -> Vec<Entity> {
        let mut ents = Vec::with_capacity(count as usize);
        let mut remaining = count;
        if let Ok(mut free) = self.world.free_entities.lock() {
            while remaining > 0 {
                if let Some(e) = free.pop() {
                    ents.push(e);
                    remaining -= 1;
                } else {
                    break;
                }
            }
        }
        if remaining > 0 {
            let start_id = self.world.next_entity_id.fetch_add(remaining, Ordering::Relaxed);
            for i in 0..remaining {
                ents.push(Entity { id: start_id + i, generation: 0 });
            }
        }
        self.queue.spawns.extend_from_slice(&ents);
        ents
    }

    pub fn despawn(&mut self, entity: Entity) {
        self.queue.despawns.push(entity);
    }

    pub fn insert<T: Component + Send + Sync + 'static>(&mut self, entity: Entity, component: T) {
        let tid = TypeId::of::<T>();
        let idx = self.queue.typed_queues.iter().position(|(t, _)| *t == tid).unwrap_or_else(|| {
            self.queue.typed_queues.push((tid, Box::new(TypedQueue::<T> { ops: Vec::new() })));
            self.queue.typed_queues.len() - 1
        });
        let q = self.queue.typed_queues[idx].1.as_any_mut().downcast_mut::<TypedQueue<T>>().unwrap();
        q.ops.push(Op::Insert(entity, component));
    }

    pub fn remove<T: Component + Send + Sync + 'static>(&mut self, entity: Entity) {
        let tid = TypeId::of::<T>();
        let idx = self.queue.typed_queues.iter().position(|(t, _)| *t == tid).unwrap_or_else(|| {
            self.queue.typed_queues.push((tid, Box::new(TypedQueue::<T> { ops: Vec::new() })));
            self.queue.typed_queues.len() - 1
        });
        let q = self.queue.typed_queues[idx].1.as_any_mut().downcast_mut::<TypedQueue<T>>().unwrap();
        q.ops.push(Op::Remove(entity));
    }
}

impl<'a> Drop for Commands<'a> {
    fn drop(&mut self) {
        if !self.queue.spawns.is_empty() {
            self.world.spawn_queue.lock().unwrap().extend(self.queue.spawns.drain(..));
        }
        if !self.queue.despawns.is_empty() {
            self.world.despawn_queue.lock().unwrap().extend(self.queue.despawns.drain(..));
        }
        if !self.queue.typed_queues.is_empty() {
            let mut world_queues = self.world.typed_queues.lock().unwrap();
            for (tid, q) in self.queue.typed_queues.drain(..) {
                if let Some((_, wq)) = world_queues.iter_mut().find(|(t, _)| *t == tid) {
                    wq.merge(q);
                } else {
                    world_queues.push((tid, q));
                }
            }
        }
    }
}

impl<'a> SystemParam for Commands<'a> {
    type State = CommandQueue;
    type Item<'i> = Commands<'i>;

    fn init_state(_world: &mut World) -> Self::State {
        CommandQueue::default()
    }

    fn get_access() -> crate::ecs::system::Access {
        crate::ecs::system::Access::default()
    }

    #[inline(always)]
    unsafe fn get_param<'i>(state: &'i mut Self::State, world: &'i World) -> Self::Item<'i> {
        Commands {
            world,
            queue: state,
        }
    }
}

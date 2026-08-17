//! Storage for the entities of a world.

use bota_proto::EntityId;

/// A slot-indexed store handing out [`EntityId`] handles.
///
/// Removing an entity bumps the generation of its slot, so a handle kept across
/// a death never resolves to whoever took the slot over.
///
/// Iteration runs in slot order and never depends on a hash, which is what makes
/// a tick reproducible.
#[derive(Clone, Debug)]
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    /// Indices of slots that can be reused.
    free: Vec<u32>,
    len: usize,
}

#[derive(Clone, Debug)]
struct Slot<T> {
    /// Bumped on every removal. Starts at one, so a zeroed handle is invalid.
    generation: u32,
    value: Option<T>,
}

impl<T> Arena<T> {
    /// An empty arena.
    pub fn new() -> Arena<T> {
        Arena {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    /// An empty arena with room for `capacity` entities before it grows.
    pub fn with_capacity(capacity: usize) -> Arena<T> {
        Arena {
            slots: Vec::with_capacity(capacity),
            free: Vec::new(),
            len: 0,
        }
    }

    /// How many entities are alive.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether no entity is alive.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Adds an entity and returns its handle.
    pub fn insert(&mut self, value: T) -> EntityId {
        self.len += 1;
        if let Some(idx) = self.free.pop() {
            let slot = &mut self.slots[idx as usize];
            slot.value = Some(value);
            return EntityId {
                idx,
                generation: slot.generation,
            };
        }
        let idx = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 1,
            value: Some(value),
        });
        EntityId { idx, generation: 1 }
    }

    /// Removes an entity, returning it if the handle was still live.
    pub fn remove(&mut self, id: EntityId) -> Option<T> {
        let slot = self.slots.get_mut(id.idx as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        let value = slot.value.take()?;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.idx);
        self.len -= 1;
        Some(value)
    }

    /// The entity behind a handle, if it is still live.
    pub fn get(&self, id: EntityId) -> Option<&T> {
        let slot = self.slots.get(id.idx as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.value.as_ref()
    }

    /// The entity behind a handle, if it is still live.
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut T> {
        let slot = self.slots.get_mut(id.idx as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.value.as_mut()
    }

    /// Whether a handle is still live.
    pub fn contains(&self, id: EntityId) -> bool {
        self.get(id).is_some()
    }

    /// Every live entity, in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.slots.iter().enumerate().filter_map(|(idx, slot)| {
            slot.value.as_ref().map(|value| {
                (
                    EntityId {
                        idx: idx as u32,
                        generation: slot.generation,
                    },
                    value,
                )
            })
        })
    }

    /// Every live entity, in slot order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EntityId, &mut T)> {
        self.slots.iter_mut().enumerate().filter_map(|(idx, slot)| {
            let generation = slot.generation;
            slot.value.as_mut().map(|value| {
                (
                    EntityId {
                        idx: idx as u32,
                        generation,
                    },
                    value,
                )
            })
        })
    }

    /// Every live handle, in slot order.
    ///
    /// Useful for a pass that has to mutate the arena while walking it, which
    /// [`iter_mut`](Arena::iter_mut) cannot allow.
    pub fn ids(&self) -> Vec<EntityId> {
        self.iter().map(|(id, _)| id).collect()
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Arena<T> {
        Arena::new()
    }
}

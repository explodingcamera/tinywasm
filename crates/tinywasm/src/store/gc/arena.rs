//! Mark-and-sweep storage with stable handles for WebAssembly GC objects.

use alloc::vec::Vec;
use core::cell::Cell;
use core::mem::size_of;
use core::num::NonZeroU32;

/// A stable reference to an arena slot.
///
/// Reclaimed slots increment their generation so stale handles cannot access a
/// new object allocated in the same slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Handle {
    index: u32,
    generation: NonZeroU32,
}

/// Adds the handles referenced by an arena object to the mark worklist.
pub(crate) trait Trace {
    /// Calls `mark` for every arena object referenced by this object.
    fn trace(&self, mark: &mut impl FnMut(Handle));
}

/// An arena allocation or capacity error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AllocError;

impl core::fmt::Display for AllocError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("GC arena capacity exhausted")
    }
}

impl core::error::Error for AllocError {}

#[inline]
fn mark<T>(slots: &[Slot<T>], worklist: &mut Vec<u32>, handle: Handle) {
    let Some(slot) = slots.get(handle.index as usize) else {
        return;
    };
    if slot.generation != handle.generation || !matches!(slot.state, SlotState::Occupied { .. }) || slot.marked.get() {
        return;
    }
    slot.marked.set(true);
    worklist.push(handle.index);
}

struct Slot<T> {
    generation: NonZeroU32,
    marked: Cell<bool>,
    state: SlotState<T>,
}

enum SlotState<T> {
    Occupied { value: T, bytes: usize },
    Free { next: Option<u32> },
    Retired,
}

/// A mark-and-sweep arena with stable generational handles.
pub(crate) struct Arena<T> {
    slots: Vec<Slot<T>>,
    free_head: Option<u32>,
    worklist: Vec<u32>,
    len: usize,
    allocated_bytes: usize,
    collection_threshold: usize,
    next_collection: usize,
}

impl<T> Arena<T> {
    /// Creates an empty arena with the given initial collection threshold.
    pub(crate) const fn new(collection_threshold: usize) -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
            worklist: Vec::new(),
            len: 0,
            allocated_bytes: 0,
            collection_threshold,
            next_collection: collection_threshold,
        }
    }

    /// Allocates a value with `out_of_line_bytes` of storage owned outside its slot.
    ///
    /// The byte count must remain valid while the value is in the arena.
    pub(crate) fn alloc(&mut self, value: T, out_of_line_bytes: usize) -> Result<Handle, AllocError> {
        let bytes = size_of::<Slot<T>>().checked_add(out_of_line_bytes).ok_or(AllocError)?;
        let allocated_bytes = self.allocated_bytes.checked_add(bytes).ok_or(AllocError)?;

        let handle = if let Some(index) = self.free_head {
            let slot = &mut self.slots[index as usize];
            let SlotState::Free { next } = slot.state else { unreachable!("free list points to an occupied slot") };
            self.free_head = next;
            slot.marked.set(false);
            slot.state = SlotState::Occupied { value, bytes };
            Handle { index, generation: slot.generation }
        } else {
            if self.slots.len() >= u32::MAX as usize {
                return Err(AllocError);
            }
            let index = self.slots.len() as u32;
            self.slots.try_reserve(1).map_err(|_| AllocError)?;
            self.slots.push(Slot {
                generation: NonZeroU32::MIN,
                marked: Cell::new(false),
                state: SlotState::Occupied { value, bytes },
            });
            Handle { index, generation: NonZeroU32::MIN }
        };

        self.len += 1;
        self.allocated_bytes = allocated_bytes;
        Ok(handle)
    }

    /// Returns a shared reference if the handle is live and current.
    pub(crate) fn get(&self, handle: Handle) -> Option<&T> {
        let slot = self.slots.get(handle.index as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        match &slot.state {
            SlotState::Occupied { value, .. } => Some(value),
            SlotState::Free { .. } | SlotState::Retired => None,
        }
    }

    /// Returns an exclusive reference if the handle is live and current.
    pub(crate) fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
        let slot = self.slots.get_mut(handle.index as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        match &mut slot.state {
            SlotState::Occupied { value, .. } => Some(value),
            SlotState::Free { .. } | SlotState::Retired => None,
        }
    }

    /// Returns whether an allocation of this size should trigger collection.
    pub(crate) fn should_collect(&self, out_of_line_bytes: usize) -> bool {
        size_of::<Slot<T>>()
            .checked_add(out_of_line_bytes)
            .and_then(|bytes| self.allocated_bytes.checked_add(bytes))
            .is_none_or(|bytes| bytes >= self.next_collection)
    }
}

impl<T: Trace> Arena<T> {
    /// Collects objects that cannot be reached from `roots`.
    pub(crate) fn collect(&mut self, roots: impl IntoIterator<Item = Handle>) -> Result<(), AllocError> {
        self.worklist.clear();
        self.worklist.try_reserve(self.len).map_err(|_| AllocError)?;

        for root in roots {
            mark(&self.slots, &mut self.worklist, root);
        }

        while let Some(index) = self.worklist.pop() {
            let slots = &self.slots;
            let worklist = &mut self.worklist;
            let SlotState::Occupied { value, .. } = &slots[index as usize].state else {
                unreachable!("marked slots are occupied")
            };
            value.trace(&mut |handle| mark(slots, worklist, handle));
        }

        let mut live_objects = 0;
        let mut live_bytes = 0;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let SlotState::Occupied { bytes, .. } = &slot.state else {
                continue;
            };
            if slot.marked.replace(false) {
                live_objects += 1;
                live_bytes += bytes;
                continue;
            }

            if let Some(generation) = slot.generation.checked_add(1) {
                slot.generation = generation;
                slot.state = SlotState::Free { next: self.free_head };
                self.free_head = Some(index as u32);
            } else {
                slot.state = SlotState::Retired;
            }
        }

        self.len = live_objects;
        self.allocated_bytes = live_bytes;
        self.next_collection = live_bytes.saturating_mul(2).max(self.collection_threshold);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    struct Object(Vec<Handle>);

    impl Trace for Object {
        fn trace(&self, mark: &mut impl FnMut(Handle)) {
            for &handle in &self.0 {
                mark(handle);
            }
        }
    }

    #[test]
    fn reuses_slots_and_rejects_stale_handles() {
        let mut arena = Arena::new(1024);
        let stale = arena.alloc(Object(vec![]), 0).unwrap();

        arena.collect([]).unwrap();
        let current = arena.alloc(Object(vec![]), 0).unwrap();

        assert_eq!(stale.index, current.index);
        assert_ne!(stale.generation, current.generation);
        assert!(arena.get(stale).is_none());
        assert!(arena.get(current).is_some());
    }

    #[test]
    fn keeps_reachable_cycles() {
        let mut arena = Arena::new(1024);
        let first = arena.alloc(Object(Vec::with_capacity(1)), size_of::<Handle>()).unwrap();
        let second = arena.alloc(Object(vec![first]), size_of::<Handle>()).unwrap();
        arena.get_mut(first).unwrap().0.push(second);

        arena.collect([first]).unwrap();

        assert_eq!(arena.len, 2);
        assert_eq!(arena.allocated_bytes, size_of::<Slot<Object>>() * 2 + size_of::<Handle>() * 2);
        assert_eq!(arena.get(second).unwrap().0, [first]);
    }

    #[test]
    fn reclaims_unreachable_cycles() {
        let mut arena = Arena::new(1024);
        let first = arena.alloc(Object(Vec::with_capacity(1)), size_of::<Handle>()).unwrap();
        let second = arena.alloc(Object(vec![first]), size_of::<Handle>()).unwrap();
        arena.get_mut(first).unwrap().0.push(second);

        arena.collect([]).unwrap();

        assert_eq!(arena.len, 0);
        assert_eq!(arena.allocated_bytes, 0);
    }

    #[test]
    fn accounts_for_object_storage() {
        let mut arena = Arena::new(1024);
        let root = arena.alloc(Object(vec![]), 24).unwrap();

        arena.collect([root]).unwrap();

        assert_eq!(arena.allocated_bytes, size_of::<Slot<Object>>() + 24);
    }
}

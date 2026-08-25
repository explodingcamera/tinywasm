use alloc::vec::Vec;
use tinywasm_types::{Shared, WeakShared};

use crate::Trap;
use crate::interpreter::ValueRef;

pub(crate) struct Roots {
    entries: Vec<Option<Root>>,
    free: Vec<u32>,
}

struct Root {
    value: ValueRef,
    token: WeakShared<()>,
}

impl Roots {
    pub(crate) const fn new() -> Self {
        Self { entries: Vec::new(), free: Vec::new() }
    }

    pub(crate) fn reserve(&mut self, additional: usize) -> Result<(), Trap> {
        self.remove_dead();
        let needed = additional.saturating_sub(self.free.len());
        self.entries.try_reserve(needed).map_err(|_| Trap::OutOfMemory)
    }

    pub(crate) fn insert(&mut self, value: ValueRef) -> Result<Shared<()>, Trap> {
        if self.free.is_empty() && self.entries.len() == self.entries.capacity() {
            self.remove_dead();
        }
        let index = if let Some(index) = self.free.pop() {
            index
        } else {
            let index = u32::try_from(self.entries.len()).map_err(|_| Trap::OutOfMemory)?;
            self.entries.try_reserve(1).map_err(|_| Trap::OutOfMemory)?;
            self.entries.push(None);
            index
        };

        let token = Shared::new(());
        self.entries[index as usize] = Some(Root { value, token: Shared::downgrade(&token) });
        Ok(token)
    }

    pub(crate) fn values(&mut self) -> impl Iterator<Item = ValueRef> + '_ {
        self.remove_dead();
        self.entries.iter().flatten().map(|root| root.value)
    }

    fn remove_dead(&mut self) {
        for (index, root) in self.entries.iter_mut().enumerate() {
            if root.as_ref().is_some_and(|root| root.token.strong_count() == 0) {
                *root = None;
                self.free.push(index as u32);
            }
        }
    }
}

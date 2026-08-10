use alloc::{boxed::Box, vec::Vec};
use core::cell::RefCell;
use core::mem::size_of;
use core::sync::atomic::{AtomicU32, Ordering};

use tinywasm_types::TypeAddr;

use crate::interpreter::{TinyWasmValue, ValueRef};

use super::{AllocError, Arena, Handle, Trace};

static NEXT_GC_REF: AtomicU32 = AtomicU32::new(0);

pub(crate) struct GcObject {
    pub(crate) type_addr: TypeAddr,
    pub(crate) values: Box<[TinyWasmValue]>,
    references: Option<Box<[Option<Handle>]>>,
}

impl Trace for GcObject {
    fn trace(&self, mark: &mut impl FnMut(Handle)) {
        if let Some(references) = &self.references {
            references.iter().flatten().copied().for_each(mark);
        }
    }
}

pub(crate) struct GcHeap {
    objects: Arena<GcObject>,
    directory: Vec<(u32, Handle)>,
    pinned: RefCell<Vec<Handle>>,
}

impl Default for GcHeap {
    fn default() -> Self {
        Self::new(1024 * 1024)
    }
}

impl GcHeap {
    /// Creates a heap with the configured allocation threshold.
    pub(crate) const fn new(collection_threshold: usize) -> Self {
        Self { objects: Arena::new(collection_threshold), directory: Vec::new(), pinned: RefCell::new(Vec::new()) }
    }

    #[inline]
    /// Resolves a compact reference to its generation-checked arena handle.
    pub(crate) fn handle(&self, value: ValueRef) -> Option<Handle> {
        let key = value.addr()?;
        let index = self.directory.binary_search_by_key(&key, |entry| entry.0).ok()?;
        Some(self.directory[index].1)
    }

    #[inline]
    pub(crate) fn get(&self, value: ValueRef) -> Option<&GcObject> {
        self.objects.get(self.handle(value)?)
    }

    #[inline]
    pub(crate) fn get_mut(&mut self, value: ValueRef) -> Option<&mut GcObject> {
        self.objects.get_mut(self.handle(value)?)
    }

    /// Allocates an object and returns its compact runtime reference.
    pub(crate) fn alloc(
        &mut self,
        type_addr: TypeAddr,
        values: Vec<TinyWasmValue>,
        trace_references: bool,
    ) -> Result<ValueRef, AllocError> {
        let key =
            NEXT_GC_REF.try_update(Ordering::Relaxed, Ordering::Relaxed, |key| (key < (1 << 30)).then_some(key + 1));
        let Ok(key) = key else {
            return Err(AllocError);
        };
        let references = if trace_references {
            let mut references = Vec::new();
            references.try_reserve_exact(values.len()).map_err(|_| AllocError)?;
            references.extend(values.iter().map(|value| match value {
                TinyWasmValue::ValueRef(value) => self.handle(*value),
                _ => None,
            }));
            Some(references.into_boxed_slice())
        } else {
            None
        };
        let element_size = size_of::<TinyWasmValue>() + if trace_references { size_of::<Option<Handle>>() } else { 0 };
        let out_of_line_bytes = values.len().checked_mul(element_size).ok_or(AllocError)?;
        let object = GcObject { type_addr, values: values.into_boxed_slice(), references };
        self.directory.try_reserve(1).map_err(|_| AllocError)?;
        let handle = self.objects.alloc(object, out_of_line_bytes)?;
        self.directory.push((key, handle));
        Ok(ValueRef::from_category_addr(key))
    }

    pub(crate) fn set(&mut self, object: ValueRef, index: usize, value: TinyWasmValue) -> Option<()> {
        let reference = match value {
            TinyWasmValue::ValueRef(value) => self.handle(value),
            _ => None,
        };
        let object = self.get_mut(object)?;
        *object.values.get_mut(index)? = value;
        if let Some(references) = &mut object.references {
            references[index] = reference;
        }
        Some(())
    }

    /// Replaces a contiguous range and updates its traced references.
    pub(crate) fn set_slice(&mut self, object: ValueRef, index: usize, values: &[TinyWasmValue]) -> Option<()> {
        let object_handle = self.handle(object)?;
        let end = index.checked_add(values.len())?;
        let directory = &self.directory;
        let object = self.objects.get_mut(object_handle)?;
        object.values.get_mut(index..end)?.copy_from_slice(values);
        if let Some(references) = &mut object.references {
            for (reference, value) in references[index..end].iter_mut().zip(values) {
                *reference = match value {
                    TinyWasmValue::ValueRef(value) => {
                        let key = value.addr();
                        key.and_then(|key| directory.binary_search_by_key(&key, |entry| entry.0).ok())
                            .map(|index| directory[index].1)
                    }
                    _ => None,
                };
            }
        }
        Some(())
    }

    /// Fills a contiguous range and updates its traced references.
    pub(crate) fn fill(
        &mut self,
        object: ValueRef,
        range: core::ops::Range<usize>,
        value: TinyWasmValue,
    ) -> Option<()> {
        let reference = match value {
            TinyWasmValue::ValueRef(value) => self.handle(value),
            _ => None,
        };
        let object = self.get_mut(object)?;
        object.values.get_mut(range.clone())?.fill(value);
        if let Some(references) = &mut object.references {
            references[range].fill(reference);
        }
        Some(())
    }

    pub(crate) fn should_collect(&self, value_count: usize, trace_references: bool) -> bool {
        let element_size = size_of::<TinyWasmValue>() + if trace_references { size_of::<Option<Handle>>() } else { 0 };
        let bytes = value_count.saturating_mul(element_size);
        self.objects.should_collect(bytes)
    }

    /// Reclaims objects unreachable from runtime and permanent host roots.
    pub(crate) fn collect(&mut self, roots: impl IntoIterator<Item = ValueRef>) -> Result<(), AllocError> {
        let pinned = self.pinned.borrow();
        let directory = &self.directory;
        let root_handles = roots.into_iter().filter_map(|value| {
            let key = value.addr()?;
            Some(directory.get(directory.binary_search_by_key(&key, |entry| entry.0).ok()?)?.1)
        });
        self.objects.collect(pinned.iter().copied().chain(root_handles))?;
        drop(pinned);
        self.directory.retain(|(_, handle)| self.objects.get(*handle).is_some());
        Ok(())
    }

    /// Permanently roots a managed reference exposed through the copyable host API.
    pub(crate) fn pin(&self, value: ValueRef) {
        if let Some(handle) = self.handle(value) {
            let mut pinned = self.pinned.borrow_mut();
            if let Err(index) = pinned.binary_search(&handle) {
                pinned.insert(index, handle);
            }
        }
    }

    pub(crate) fn copy_within(&mut self, object: ValueRef, src: core::ops::Range<usize>, dst: usize) -> Option<()> {
        let object = self.get_mut(object)?;
        object.values.copy_within(src.clone(), dst);
        if let Some(references) = &mut object.references {
            references.copy_within(src, dst);
        }
        Some(())
    }

    /// Copies values and tracing metadata between two distinct objects.
    pub(crate) fn copy_between(
        &mut self,
        src: ValueRef,
        src_range: core::ops::Range<usize>,
        dst: ValueRef,
        dst_index: usize,
    ) -> Option<()> {
        let src = self.handle(src)?;
        let dst = self.handle(dst)?;
        let (src, dst) = self.objects.get_disjoint_mut(src, dst)?;
        let src_values = src.values.get(src_range.clone())?;
        let dst_end = dst_index.checked_add(src_values.len())?;
        dst.values.get_mut(dst_index..dst_end)?.copy_from_slice(src_values);
        if let Some(dst_refs) = &mut dst.references {
            if let Some(src_refs) = &src.references {
                dst_refs[dst_index..dst_end].copy_from_slice(&src_refs[src_range]);
            } else {
                dst_refs[dst_index..dst_end].fill(None);
            }
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_keys_are_not_reused_after_collection() {
        let mut heap = GcHeap::default();
        let stale = heap.alloc(0, Vec::new(), false).unwrap();
        heap.collect([]).unwrap();
        let current = heap.alloc(0, Vec::new(), false).unwrap();

        assert_ne!(stale, current);
        assert!(heap.get(stale).is_none());
        assert!(heap.get(current).is_some());
    }

    #[test]
    fn compact_keys_do_not_resolve_in_another_heap() {
        let mut first = GcHeap::default();
        let second = GcHeap::default();
        let value = first.alloc(0, Vec::new(), false).unwrap();

        assert!(second.get(value).is_none());
    }

    #[test]
    fn collection_reclaims_object_cycles() {
        let mut heap = GcHeap::default();
        let first = heap.alloc(0, alloc::vec![TinyWasmValue::ValueRef(ValueRef::NULL)], true).unwrap();
        let second = heap.alloc(0, alloc::vec![TinyWasmValue::ValueRef(first)], true).unwrap();
        heap.set(first, 0, TinyWasmValue::ValueRef(second)).unwrap();

        heap.collect([]).unwrap();

        assert!(heap.get(first).is_none());
        assert!(heap.get(second).is_none());
    }
}

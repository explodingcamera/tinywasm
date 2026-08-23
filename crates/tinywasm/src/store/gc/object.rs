use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::mem::size_of;
use core::sync::atomic::{AtomicU32, Ordering};

use tinywasm_types::{TagAddr, TypeAddr};

use crate::engine::Config;
use crate::interpreter::{RuntimeValue, ValueRef};
use crate::{ResourceLimiter, Trap};

use super::{AllocError, Arena, Handle, Trace};

static NEXT_GC_REF: AtomicU32 = AtomicU32::new(0);

pub(crate) struct GcObject {
    pub(crate) kind: GcObjectKind,
    pub(crate) values: Box<[RuntimeValue]>,
    references: Option<Box<[Option<Handle>]>>,
}

#[derive(Clone, Copy)]
pub(crate) enum GcObjectKind {
    Composite(TypeAddr),
    Exception(TagAddr),
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
    resource_limiter: Option<Arc<dyn ResourceLimiter>>,
}

impl Default for GcHeap {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl GcHeap {
    /// Creates a heap with the configured allocation threshold.
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            objects: Arena::new(config.gc_collection_threshold),
            directory: Vec::new(),
            resource_limiter: config.resource_limiter.clone(),
        }
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
    pub(crate) fn get_handle(&self, handle: Handle) -> Option<&GcObject> {
        self.objects.get(handle)
    }

    pub(crate) fn check_allocation(&self, value_count: usize, trace_references: bool) -> Result<(), Trap> {
        let element_size = size_of::<RuntimeValue>() + if trace_references { size_of::<Option<Handle>>() } else { 0 };
        let out_of_line_bytes = value_count.checked_mul(element_size).ok_or(Trap::OutOfMemory)?;
        let allocation_size = Arena::<GcObject>::allocation_size(out_of_line_bytes).ok_or(Trap::OutOfMemory)?;
        let desired = self.objects.allocated_bytes.checked_add(allocation_size).ok_or(Trap::OutOfMemory)?;
        if let Some(limiter) = &self.resource_limiter
            && !limiter.gc_growing(self.objects.allocated_bytes, desired, None)?
        {
            return Err(Trap::OutOfMemory);
        }
        Ok(())
    }

    /// Allocates an object and returns its compact runtime reference.
    pub(crate) fn alloc(
        &mut self,
        type_addr: TypeAddr,
        values: Vec<RuntimeValue>,
        trace_references: bool,
    ) -> Result<ValueRef, Trap> {
        self.alloc_kind(GcObjectKind::Composite(type_addr), values, trace_references, None)
    }

    /// Allocates an exception and traces references in its payload.
    pub(crate) fn alloc_exception(
        &mut self,
        tag_addr: TagAddr,
        payload: Vec<RuntimeValue>,
        trace_fields: &[bool],
    ) -> Result<ValueRef, Trap> {
        self.alloc_kind(GcObjectKind::Exception(tag_addr), payload, true, Some(trace_fields))
    }

    fn alloc_kind(
        &mut self,
        kind: GcObjectKind,
        values: Vec<RuntimeValue>,
        trace_references: bool,
        trace_fields: Option<&[bool]>,
    ) -> Result<ValueRef, Trap> {
        let element_size = size_of::<RuntimeValue>() + if trace_references { size_of::<Option<Handle>>() } else { 0 };
        let out_of_line_bytes = values.len().checked_mul(element_size).ok_or(Trap::OutOfMemory)?;
        let key =
            NEXT_GC_REF.try_update(Ordering::Relaxed, Ordering::Relaxed, |key| (key < (1 << 30)).then_some(key + 1));
        let Ok(key) = key else {
            return Err(Trap::OutOfMemory);
        };
        let references = if trace_references {
            let mut references = Vec::new();
            references.try_reserve_exact(values.len()).map_err(|_| Trap::OutOfMemory)?;
            references.extend(values.iter().enumerate().map(|(index, value)| match value {
                RuntimeValue::ValueRef(value) if trace_fields.is_none_or(|fields| fields[index]) => self.handle(*value),
                _ => None,
            }));
            Some(references.into_boxed_slice())
        } else {
            None
        };
        let object = GcObject { kind, values: values.into_boxed_slice(), references };
        self.directory.try_reserve(1).map_err(|_| Trap::OutOfMemory)?;
        let handle = self.objects.alloc(object, out_of_line_bytes).map_err(|_| Trap::OutOfMemory)?;
        self.directory.push((key, handle));
        Ok(ValueRef::from_category_addr(key))
    }

    pub(crate) fn set(&mut self, object: Handle, index: usize, value: RuntimeValue) -> Option<()> {
        let reference = match value {
            RuntimeValue::ValueRef(value) => self.handle(value),
            _ => None,
        };
        let object = self.objects.get_mut(object)?;
        *object.values.get_mut(index)? = value;
        if let Some(references) = &mut object.references {
            references[index] = reference;
        }
        Some(())
    }

    /// Replaces a contiguous range and updates its traced references.
    pub(crate) fn set_slice(&mut self, object: Handle, index: usize, values: &[RuntimeValue]) -> Option<()> {
        let end = index.checked_add(values.len())?;
        let directory = &self.directory;
        let object = self.objects.get_mut(object)?;
        object.values.get_mut(index..end)?.copy_from_slice(values);
        if let Some(references) = &mut object.references {
            for (reference, value) in references[index..end].iter_mut().zip(values) {
                *reference = match value {
                    RuntimeValue::ValueRef(value) => value.addr().and_then(|key| {
                        directory.binary_search_by_key(&key, |entry| entry.0).ok().map(|index| directory[index].1)
                    }),
                    _ => None,
                };
            }
        }
        Some(())
    }

    /// Fills a contiguous range and updates its traced references.
    pub(crate) fn fill(&mut self, object: Handle, range: core::ops::Range<usize>, value: RuntimeValue) -> Option<()> {
        let reference = match value {
            RuntimeValue::ValueRef(value) => self.handle(value),
            _ => None,
        };
        let object = self.objects.get_mut(object)?;
        object.values.get_mut(range.clone())?.fill(value);
        if let Some(references) = &mut object.references {
            references[range].fill(reference);
        }
        Some(())
    }

    pub(crate) fn should_collect(&self, value_count: usize, trace_references: bool) -> bool {
        let element_size = size_of::<RuntimeValue>() + if trace_references { size_of::<Option<Handle>>() } else { 0 };
        self.objects.should_collect(value_count.saturating_mul(element_size))
    }

    /// Reclaims objects unreachable from runtime roots.
    pub(crate) fn collect(&mut self, roots: impl IntoIterator<Item = ValueRef>) -> Result<(), AllocError> {
        let directory = &self.directory;
        let root_handles = roots.into_iter().filter_map(|value| {
            let key = value.addr()?;
            Some(directory.get(directory.binary_search_by_key(&key, |entry| entry.0).ok()?)?.1)
        });
        self.objects.collect(root_handles)?;
        self.directory.retain(|(_, handle)| self.objects.get(*handle).is_some());
        Ok(())
    }

    pub(crate) fn copy_within(&mut self, object: Handle, src: core::ops::Range<usize>, dst: usize) -> Option<()> {
        let object = self.objects.get_mut(object)?;
        object.values.copy_within(src.clone(), dst);
        if let Some(references) = &mut object.references {
            references.copy_within(src, dst);
        }
        Some(())
    }

    /// Copies values and tracing metadata between two distinct objects.
    pub(crate) fn copy_between(
        &mut self,
        src: Handle,
        src_range: core::ops::Range<usize>,
        dst: Handle,
        dst_index: usize,
    ) -> Option<()> {
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
        let first = heap.alloc(0, alloc::vec![RuntimeValue::ValueRef(ValueRef::NULL)], true).unwrap();
        let second = heap.alloc(0, alloc::vec![RuntimeValue::ValueRef(first)], true).unwrap();
        heap.set(heap.handle(first).unwrap(), 0, RuntimeValue::ValueRef(second)).unwrap();

        heap.collect([]).unwrap();

        assert!(heap.get(first).is_none());
        assert!(heap.get(second).is_none());
    }

    #[test]
    fn exception_payload_traces_managed_objects() {
        let mut heap = GcHeap::default();
        let payload = heap.alloc(0, Vec::new(), false).unwrap();
        let exception = heap.alloc_exception(0, alloc::vec![RuntimeValue::ValueRef(payload)], &[true]).unwrap();

        heap.collect([exception]).unwrap();
        assert!(heap.get(payload).is_some());

        heap.collect([]).unwrap();
        assert!(heap.get(exception).is_none());
        assert!(heap.get(payload).is_none());
    }
}

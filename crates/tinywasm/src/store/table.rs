use crate::{ResourceLimiter, Result, Trap, interpreter::ValueRef};
use alloc::vec::Vec;
use core::ops::Range;
use tinywasm_types::*;

const MAX_TABLE_SIZE: usize = 10_000_000;

/// A WebAssembly Table Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#table-instances>
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct TableInstance {
    pub(crate) elements: Vec<ValueRef>,
    pub(crate) kind: TableType,
}

impl TableInstance {
    /// Creates a table filled with the given initial reference.
    pub(crate) fn new(kind: TableType, init: ValueRef, limiter: Option<&dyn ResourceLimiter>) -> Result<Self> {
        if kind.size_max.is_some_and(|maximum| maximum < kind.size_initial) {
            return Err(Trap::OutOfMemory.into());
        }
        let size = cold_err!(usize::try_from(kind.size_initial)).map_err(|_| Trap::OutOfMemory)?;
        if size > MAX_TABLE_SIZE {
            return Err(Trap::OutOfMemory.into());
        }
        if size != 0
            && let Some(limiter) = limiter
            && !limiter.table_growing(0, size, Self::maximum_size(kind))?
        {
            return Err(Trap::OutOfMemory.into());
        }
        let mut elements = Vec::new();
        cold_err!(elements.try_reserve_exact(size)).map_err(|_| Trap::OutOfMemory)?;
        elements.resize(size, init);
        Ok(Self { elements, kind })
    }

    #[inline(never)]
    #[cold]
    fn trap_oob(&self, addr: usize, len: usize) -> Trap {
        crate::Trap::TableOutOfBounds { offset: addr, len, max: self.elements.len() }
    }

    fn checked_range(&self, addr: usize, len: usize) -> Result<Range<usize>, Trap> {
        let end = addr.checked_add(len).ok_or_else(|| self.trap_oob(addr, len))?;
        if end > self.elements.len() {
            return Err(self.trap_oob(addr, len));
        }
        Ok(addr..end)
    }

    pub(crate) fn fill(&mut self, addr: usize, len: usize, val: ValueRef) -> Result<(), Trap> {
        let range = self.checked_range(addr, len)?;
        self.elements[range].fill(val);
        Ok(())
    }

    pub(crate) fn get(&self, addr: usize) -> Result<&ValueRef, Trap> {
        self.elements.get(addr).ok_or_else(|| self.trap_oob(addr, 1))
    }

    pub(crate) fn copy_from_slice(&mut self, dst: usize, src: &[ValueRef]) -> Result<(), Trap> {
        let range = self.checked_range(dst, src.len())?;
        self.elements[range].copy_from_slice(src);
        Ok(())
    }

    pub(crate) fn load(&self, addr: usize, len: usize) -> Result<&[ValueRef], Trap> {
        Ok(&self.elements[self.checked_range(addr, len)?])
    }

    pub(crate) fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Result<(), Trap> {
        let src = self.checked_range(src, len)?;
        self.checked_range(dst, len)?;
        self.elements.copy_within(src, dst);
        Ok(())
    }

    pub(crate) fn set(&mut self, table_idx: usize, value: ValueRef) -> Result<(), Trap> {
        let range = self.checked_range(table_idx, 1)?;
        self.elements[range.start] = value;
        Ok(())
    }

    pub(crate) fn grow(
        &mut self,
        n: usize,
        init: ValueRef,
        limiter: Option<&dyn ResourceLimiter>,
    ) -> Result<bool, Trap> {
        let Some(len) = n.checked_add(self.elements.len()) else {
            return Ok(false);
        };
        let declared_max = self.kind.size_max.and_then(|max| usize::try_from(max).ok()).unwrap_or(usize::MAX);
        let max = declared_max.min(MAX_TABLE_SIZE);
        if len > max {
            return Ok(false);
        }
        if len == self.elements.len() {
            return Ok(true);
        }
        if let Some(limiter) = limiter
            && !limiter.table_growing(self.elements.len(), len, Self::maximum_size(self.kind))?
        {
            return Ok(false);
        }

        if cold_err!(self.elements.try_reserve_exact(n)).is_err() {
            return Ok(false);
        }
        self.elements.resize(len, init);
        Ok(true)
    }

    fn maximum_size(kind: TableType) -> Option<usize> {
        kind.size_max.and_then(|maximum| usize::try_from(maximum).ok())
    }

    pub(crate) fn size(&self) -> usize {
        self.elements.len()
    }

    pub(crate) fn init(&mut self, offset: usize, init: &[ValueRef]) -> Result<(), Trap> {
        let range = self.checked_range(offset, init.len())?;
        self.elements[range].copy_from_slice(init);
        Ok(())
    }
}

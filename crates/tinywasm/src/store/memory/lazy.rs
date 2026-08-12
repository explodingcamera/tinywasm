use alloc::boxed::Box;

use tinywasm_types::MemoryType;

use crate::{Error, MemoryBackend, Result};

use super::LinearMemory;

/// A linear memory wrapper that allocates its backend on the first mutation.
///
/// Before materialization, the memory is represented by its logical length and
/// reads return the zeroes required by WebAssembly semantics.
pub struct LazyLinearMemory {
    ty: MemoryType,
    initial_len: usize,
    backend: MemoryBackend,
    inner: Option<Box<dyn LinearMemory>>,
}

impl LazyLinearMemory {
    /// Creates a lazy memory for `ty` using `backend` for eventual storage.
    pub fn try_new(ty: MemoryType, backend: MemoryBackend) -> Result<Self> {
        let page_size = usize::try_from(ty.page_size())
            .map_err(|_| Error::UnsupportedFeature("memory page size exceeds the host address space"))?;
        let pages = usize::try_from(ty.page_count_initial())
            .map_err(|_| Error::UnsupportedFeature("memory size exceeds the host address space"))?;
        let initial_len = pages
            .checked_mul(page_size)
            .ok_or(Error::UnsupportedFeature("memory size exceeds the host address space"))?;
        Ok(Self::new_with_initial_len(ty, initial_len, backend))
    }

    pub(crate) fn new_with_initial_len(ty: MemoryType, initial_len: usize, backend: MemoryBackend) -> Self {
        Self { ty, initial_len, backend, inner: None }
    }

    fn materialize(&mut self) -> core::result::Result<&mut dyn LinearMemory, crate::Trap> {
        if self.inner.is_none() {
            let storage = cold_err!(self.backend.create(self.ty, self.initial_len))?;
            self.inner = Some(storage);
        }
        Ok(self.inner.as_deref_mut().expect("lazy memory should be materialized"))
    }
}

impl LinearMemory for LazyLinearMemory {
    fn len(&self) -> usize {
        self.inner.as_deref().map_or(self.initial_len, LinearMemory::len)
    }

    fn grow_to(&mut self, new_len: usize) -> Result<(), crate::Trap> {
        self.materialize()?.grow_to(new_len)
    }

    fn read(&self, addr: usize, dst: &mut [u8]) -> usize {
        if let Some(inner) = self.inner.as_deref() {
            return inner.read(addr, dst);
        }
        if addr >= self.initial_len {
            return 0;
        }
        let read_len = dst.len().min(self.initial_len - addr);
        dst[..read_len].fill(0);
        read_len
    }

    fn write(&mut self, addr: usize, src: &[u8]) -> core::result::Result<usize, crate::Trap> {
        if src.is_empty() || addr >= self.len() {
            return Ok(0);
        }
        self.materialize()?.write(addr, src)
    }

    fn write_all(&mut self, addr: usize, src: &[u8]) -> core::result::Result<Option<()>, crate::Trap> {
        let Some(end) = addr.checked_add(src.len()) else { return Ok(None) };
        if end > self.len() {
            return Ok(None);
        }
        if src.is_empty() {
            return Ok(Some(()));
        }
        self.materialize()?.write_all(addr, src)
    }

    fn fill(&mut self, addr: usize, len: usize, val: u8) -> core::result::Result<Option<()>, crate::Trap> {
        let Some(end) = addr.checked_add(len) else { return Ok(None) };
        if end > self.len() {
            return Ok(None);
        }
        if len == 0 || val == 0 && self.inner.is_none() {
            return Ok(Some(()));
        }
        self.materialize()?.fill(addr, len, val)
    }

    fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> core::result::Result<Option<()>, crate::Trap> {
        let Some(src_end) = src.checked_add(len) else { return Ok(None) };
        let Some(dst_end) = dst.checked_add(len) else { return Ok(None) };
        if src_end > self.len() || dst_end > self.len() {
            return Ok(None);
        }
        if self.inner.is_none() || len == 0 || dst == src {
            return Ok(Some(()));
        }
        self.materialize()?.copy_within(dst, src, len)
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for LazyLinearMemory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LazyLinearMemory").field("ty", &self.ty).field("materialized", &self.inner.is_some()).finish()
    }
}

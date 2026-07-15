use alloc::boxed::Box;
use core::hint::cold_path;

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
        let initial_len = usize::try_from(ty.initial_size())
            .map_err(|_| Error::UnsupportedFeature("memory size exceeds the host address space"))?;
        Ok(Self::new_with_initial_len(ty, initial_len, backend))
    }

    pub(crate) fn new_with_initial_len(ty: MemoryType, initial_len: usize, backend: MemoryBackend) -> Self {
        Self { ty, initial_len, backend, inner: None }
    }

    fn materialize(&mut self) -> &mut dyn LinearMemory {
        if self.inner.is_none() {
            self.inner =
                Some(self.backend.create(self.ty, self.initial_len).expect("lazy memory materialization failed").0);
        }
        self.inner.as_deref_mut().expect("lazy memory should be materialized")
    }

    fn try_materialize(&mut self) -> core::result::Result<&mut dyn LinearMemory, crate::Trap> {
        if self.inner.is_none() {
            let storage = match self.backend.create(self.ty, self.initial_len) {
                Ok(storage) => storage,
                Err(Error::Trap(trap)) => {
                    cold_path();
                    return Err(trap);
                }
                Err(err) => panic!("lazy memory materialization failed: {err}"),
            };
            self.inner = Some(storage.0);
        }
        Ok(self.inner.as_deref_mut().expect("lazy memory should be materialized"))
    }
}

impl LinearMemory for LazyLinearMemory {
    fn len(&self) -> usize {
        self.inner.as_deref().map_or(self.initial_len, LinearMemory::len)
    }

    fn grow_to(&mut self, new_len: usize) -> Result<(), crate::Trap> {
        self.try_materialize()?.grow_to(new_len)
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

    fn write(&mut self, addr: usize, src: &[u8]) -> usize {
        if src.is_empty() || addr >= self.len() {
            return 0;
        }
        self.materialize().write(addr, src)
    }

    fn write_all(&mut self, addr: usize, src: &[u8]) -> Option<()> {
        let end = addr.checked_add(src.len())?;
        if end > self.len() {
            return None;
        }
        if src.is_empty() {
            return Some(());
        }
        self.materialize().write_all(addr, src)
    }

    fn fill(&mut self, addr: usize, len: usize, val: u8) -> Option<()> {
        let end = addr.checked_add(len)?;
        if end > self.len() {
            return None;
        }
        if len == 0 || val == 0 && self.inner.is_none() {
            return Some(());
        }
        self.materialize().fill(addr, len, val)
    }

    fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Option<()> {
        let src_end = src.checked_add(len)?;
        let dst_end = dst.checked_add(len)?;
        if src_end > self.len() || dst_end > self.len() {
            return None;
        }
        if self.inner.is_none() || len == 0 || dst == src {
            return Some(());
        }
        self.materialize().copy_within(dst, src, len)
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for LazyLinearMemory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LazyLinearMemory").field("ty", &self.ty).field("materialized", &self.inner.is_some()).finish()
    }
}

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::RefCell;

use tinywasm_types::MemoryType;

use crate::{Error, MemoryBackend, Result};

use super::LinearMemory;

/// A linear memory wrapper that materializes its inner backend on first access.
///
/// If the wrapped backend fails to create the inner memory during first access,
/// this wrapper will panic.
pub struct LazyLinearMemory {
    ty: MemoryType,
    initial_len: usize,
    backend: MemoryBackend,
    inner: RefCell<Option<Box<dyn LinearMemory>>>,
}

impl LazyLinearMemory {
    /// Creates a lazy memory for `ty` using `backend` for the eventual materialized storage.
    pub fn new(ty: MemoryType, backend: MemoryBackend) -> Result<Self> {
        let initial_len = usize::try_from(ty.initial_size())
            .map_err(|_| Error::UnsupportedFeature("memory size exceeds the host address space"))?;
        Ok(Self::new_with_initial_len(ty, initial_len, backend))
    }

    pub(crate) fn new_with_initial_len(ty: MemoryType, initial_len: usize, backend: MemoryBackend) -> Self {
        Self { ty, initial_len, backend, inner: RefCell::new(None) }
    }

    fn with_inner<R>(&self, f: impl FnOnce(&dyn LinearMemory) -> R) -> R {
        self.ensure_materialized();
        let inner = self.inner.borrow();
        f(inner.as_deref().expect("lazy memory should be materialized"))
    }

    fn with_inner_mut<R>(&self, f: impl FnOnce(&mut dyn LinearMemory) -> R) -> R {
        self.ensure_materialized();
        let mut inner = self.inner.borrow_mut();
        f(inner.as_deref_mut().expect("lazy memory should be materialized"))
    }

    fn ensure_materialized(&self) {
        if self.inner.borrow().is_some() {
            return;
        }

        // Lazy materialization happens from trait methods that cannot surface backend creation errors.
        let storage = self.backend.create(self.ty, self.initial_len).expect("lazy memory materialization failed");
        *self.inner.borrow_mut() = Some(storage.0);
    }
}

impl LinearMemory for LazyLinearMemory {
    fn len(&self) -> usize {
        self.with_inner(|inner| inner.len())
    }

    fn grow_to(&mut self, new_len: usize) -> Option<()> {
        self.with_inner_mut(|inner| inner.grow_to(new_len))
    }

    fn read(&self, addr: usize, dst: &mut [u8]) -> usize {
        self.with_inner(|inner| inner.read(addr, dst))
    }

    fn write(&mut self, addr: usize, src: &[u8]) -> usize {
        self.with_inner_mut(|inner| inner.write(addr, src))
    }

    fn write_all(&mut self, addr: usize, src: &[u8]) -> Option<()> {
        self.with_inner_mut(|inner| inner.write_all(addr, src))
    }

    fn fill(&mut self, addr: usize, len: usize, val: u8) -> Option<()> {
        self.with_inner_mut(|inner| inner.fill(addr, len, val))
    }

    fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Option<()> {
        self.with_inner_mut(|inner| inner.copy_within(dst, src, len))
    }

    fn read_exact(&self, addr: usize, dst: &mut [u8]) -> Option<()> {
        self.with_inner(|inner| inner.read_exact(addr, dst))
    }

    fn read_vec(&self, addr: usize, len: usize) -> Option<Vec<u8>> {
        self.with_inner(|inner| inner.read_vec(addr, len))
    }

    fn read_8(&self, base: u64, offset: u64) -> core::result::Result<u8, crate::Trap> {
        self.with_inner(|inner| inner.read_8(base, offset))
    }

    fn read_16(&self, base: u64, offset: u64) -> core::result::Result<[u8; 2], crate::Trap> {
        self.with_inner(|inner| inner.read_16(base, offset))
    }

    fn read_32(&self, base: u64, offset: u64) -> core::result::Result<[u8; 4], crate::Trap> {
        self.with_inner(|inner| inner.read_32(base, offset))
    }

    fn read_64(&self, base: u64, offset: u64) -> core::result::Result<[u8; 8], crate::Trap> {
        self.with_inner(|inner| inner.read_64(base, offset))
    }

    fn read_128(&self, base: u64, offset: u64) -> core::result::Result<[u8; 16], crate::Trap> {
        self.with_inner(|inner| inner.read_128(base, offset))
    }

    fn write_8(&mut self, base: u64, offset: u64, byte: u8) -> core::result::Result<(), crate::Trap> {
        self.with_inner_mut(|inner| inner.write_8(base, offset, byte))
    }

    fn write_16(&mut self, base: u64, offset: u64, bytes: [u8; 2]) -> core::result::Result<(), crate::Trap> {
        self.with_inner_mut(|inner| inner.write_16(base, offset, bytes))
    }

    fn write_32(&mut self, base: u64, offset: u64, bytes: [u8; 4]) -> core::result::Result<(), crate::Trap> {
        self.with_inner_mut(|inner| inner.write_32(base, offset, bytes))
    }

    fn write_64(&mut self, base: u64, offset: u64, bytes: [u8; 8]) -> core::result::Result<(), crate::Trap> {
        self.with_inner_mut(|inner| inner.write_64(base, offset, bytes))
    }

    fn write_128(&mut self, base: u64, offset: u64, bytes: [u8; 16]) -> core::result::Result<(), crate::Trap> {
        self.with_inner_mut(|inner| inner.write_128(base, offset, bytes))
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for LazyLinearMemory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LazyLinearMemory")
            .field("ty", &self.ty)
            .field("materialized", &self.inner.borrow().is_some())
            .finish()
    }
}

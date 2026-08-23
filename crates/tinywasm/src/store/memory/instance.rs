use tinywasm_types::{MemoryArch, MemoryType};

use crate::{Error, ResourceLimiter, Result, Trap};

use super::{MemoryStorage, memory_oob};

/// A WebAssembly Memory Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#memory-instances>
pub(crate) struct MemoryInstance {
    pub(crate) kind: MemoryType,
    pub(crate) inner: MemoryStorage,
    pub(crate) page_count: usize,
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for MemoryInstance {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MemoryInstance").field("kind", &self.kind).field("page_count", &self.page_count).finish()
    }
}

impl MemoryInstance {
    #[inline]
    fn host_size(kind: MemoryType, pages: u64) -> Option<usize> {
        pages.checked_mul(kind.page_size()).and_then(|size| usize::try_from(size).ok())
    }

    #[inline]
    fn maximum_size(kind: MemoryType) -> Option<usize> {
        kind.page_count_max_declared().map(|pages| Self::host_size(kind, pages).unwrap_or(usize::MAX))
    }

    #[inline(always)]
    pub(crate) fn effective_addr<const N: usize>(&self, base: usize, offset: u64) -> Result<usize, Trap> {
        #[cfg(target_pointer_width = "64")]
        {
            if !self.is_64bit() {
                debug_assert!(u32::try_from(offset).is_ok(), "validated memory32 offsets fit in u32");
                return Ok(base + offset as usize);
            }
            match base.checked_add(offset as usize) {
                Some(addr) => Ok(addr),
                None => cold!(Err(memory_oob(base, N, self.inner.len()))),
            }
        }

        #[cfg(not(target_pointer_width = "64"))]
        {
            match usize::try_from(offset).ok().and_then(|offset| base.checked_add(offset)) {
                Some(addr) => Ok(addr),
                None => cold!(Err(memory_oob(base, N, self.inner.len()))),
            }
        }
    }

    pub(crate) fn new(kind: MemoryType, limiter: Option<&dyn ResourceLimiter>) -> Result<Self> {
        let initial_len = cold_err!(
            Self::host_size(kind, kind.page_count_initial())
                .ok_or(Error::UnsupportedFeature("memory size exceeds the host address space"))
        )?;

        crate::log::debug!(
            "initializing memory with {} pages of {} bytes",
            kind.page_count_initial(),
            kind.page_size()
        );

        if initial_len != 0
            && let Some(limiter) = limiter
            && !limiter.memory_growing(0, initial_len, Self::maximum_size(kind))?
        {
            return cold!(Err(Trap::OutOfMemory.into()));
        }

        let storage = MemoryStorage::try_new(initial_len)?;
        Ok(Self { kind, inner: storage, page_count: kind.page_count_initial() as usize })
    }

    pub(crate) const fn is_64bit(&self) -> bool {
        matches!(self.kind.arch(), MemoryArch::I64)
    }

    pub(crate) fn copy_from_memory(
        &mut self,
        dst: usize,
        src_memory: &MemoryInstance,
        src: usize,
        len: usize,
    ) -> Result<(), Trap> {
        src_memory.inner.checked_range(src, len).ok_or_else(|| cold!(memory_oob(src, len, src_memory.inner.len())))?;
        self.inner.checked_range(dst, len).ok_or_else(|| cold!(memory_oob(dst, len, self.inner.len())))?;
        self.inner.copy_from(dst, &src_memory.inner, src, len);
        Ok(())
    }

    pub(crate) fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Result<(), Trap> {
        self.inner.copy_within(dst, src, len).ok_or_else(|| cold!(memory_oob(dst, len, self.inner.len())))
    }

    pub(crate) fn grow(
        &mut self,
        pages_delta: i64,
        limiter: Option<&dyn ResourceLimiter>,
    ) -> Result<Option<i64>, Trap> {
        let current_pages = self.page_count;
        let Some(new_pages) = usize::try_from(pages_delta).ok().and_then(|delta| current_pages.checked_add(delta))
        else {
            return cold!(Ok(None));
        };
        let max_pages = self.kind.page_count_max().try_into().unwrap_or(usize::MAX);

        if new_pages > max_pages {
            return cold!({
                crate::log::debug!("memory.grow failed: new_pages={}, max_pages={}", new_pages, max_pages);
                Ok(None)
            });
        }

        let Some(new_size) = Self::host_size(self.kind, new_pages as u64) else {
            return cold!(Ok(None));
        };
        if new_size == self.inner.len() {
            return Ok(i64::try_from(current_pages).ok());
        }

        if let Some(limiter) = limiter
            && !limiter.memory_growing(self.inner.len(), new_size, Self::maximum_size(self.kind))?
        {
            return cold!(Ok(None));
        }

        if self.inner.grow_to(new_size).is_err() {
            return cold!(Ok(None));
        }
        self.page_count = new_pages;
        Ok(i64::try_from(current_pages).ok())
    }
}

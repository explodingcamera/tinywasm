use alloc::format;
use tinywasm_types::{MemoryArch, MemoryType};

use crate::{Error, MemoryBackend, Result, Trap};

use super::{MemoryStorage, memory_oob};
use core::hint::cold_path;

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
    const COPY_CHUNK_SIZE: usize = 4 * 1024;

    #[inline(always)]
    pub(crate) fn effective_addr_32<const N: usize>(&self, base: u32, offset: u64) -> Result<usize, Trap> {
        #[cfg(target_pointer_width = "64")]
        {
            debug_assert!(u32::try_from(offset).is_ok(), "validated memory32 offsets fit in u32");
            Ok(base as usize + offset as usize)
        }

        #[cfg(not(target_pointer_width = "64"))]
        {
            match usize::try_from(u64::from(base) + offset) {
                Ok(addr) => Ok(addr),
                Err(_) => {
                    cold_path();
                    Err(memory_oob(base as usize, N, self.inner.len()))
                }
            }
        }
    }

    #[inline(always)]
    pub(crate) fn effective_addr_64<const N: usize>(&self, base: u64, offset: u64) -> Result<usize, Trap> {
        match base.checked_add(offset).and_then(|addr| usize::try_from(addr).ok()) {
            Some(addr) => Ok(addr),
            None => {
                cold_path();
                Err(memory_oob(base as usize, N, self.inner.len()))
            }
        }
    }

    pub(crate) fn new(kind: MemoryType, backend: &MemoryBackend) -> Result<Self> {
        assert!(kind.page_count_initial() <= kind.page_count_max());

        let initial_len = usize::try_from(kind.initial_size())
            .map_err(|_| Error::UnsupportedFeature("memory size exceeds the host address space"))?;

        crate::log::debug!(
            "initializing memory with {} pages of {} bytes",
            kind.page_count_initial(),
            kind.page_size()
        );

        let storage = backend.create(kind, initial_len)?;
        if storage.len() != initial_len {
            return Err(Error::Other(format!(
                "memory backend returned {} bytes for a memory that requires {initial_len}",
                storage.len()
            )));
        }

        Ok(Self { kind, inner: storage, page_count: kind.page_count_initial() as usize })
    }

    pub(crate) fn new_lazy(kind: MemoryType, backend: &MemoryBackend) -> Result<Self> {
        assert!(kind.page_count_initial() <= kind.page_count_max());

        let initial_len = usize::try_from(kind.initial_size())
            .map_err(|_| Error::UnsupportedFeature("memory size exceeds the host address space"))?;

        crate::log::debug!(
            "initializing lazy memory with {} pages of {} bytes",
            kind.page_count_initial(),
            kind.page_size()
        );

        let storage = backend.create_lazy(kind, initial_len)?;
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
        fn check_range(mem: &MemoryStorage, addr: usize, len: usize) -> Result<(), crate::Trap> {
            let Some(end) = addr.checked_add(len) else {
                cold_path();
                return Err(memory_oob(addr, len, mem.len()));
            };

            if end > mem.len() || end < addr {
                cold_path();
                return Err(memory_oob(addr, len, mem.len()));
            }
            Ok(())
        }

        check_range(&src_memory.inner, src, len)?;
        check_range(&self.inner, dst, len)?;

        if len == 0 {
            return Ok(());
        }

        let mut buf = [0u8; Self::COPY_CHUNK_SIZE];
        let mut copied = 0;
        while copied < len {
            let chunk_len = buf.len().min(len - copied);
            src_memory.inner.read_exact(src + copied, &mut buf[..chunk_len]).ok_or_else(|| {
                cold_path();
                memory_oob(src + copied, chunk_len, src_memory.inner.len())
            })?;
            self.inner.write_all(dst + copied, &buf[..chunk_len]).ok_or_else(|| {
                cold_path();
                memory_oob(dst + copied, chunk_len, self.inner.len())
            })?;
            copied += chunk_len;
        }

        Ok(())
    }

    pub(crate) fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Result<(), Trap> {
        self.inner.copy_within(dst, src, len).ok_or_else(|| {
            cold_path();
            memory_oob(dst, len, self.inner.len())
        })
    }

    pub(crate) fn grow(&mut self, pages_delta: i64, trap_on_oom: bool) -> Result<Option<i64>, Trap> {
        if pages_delta < 0 {
            cold_path();
            crate::log::debug!("memory.grow failed: negative delta {}", pages_delta);
            return Ok(None);
        }

        let current_pages = self.page_count;
        let Some(pages_delta) = usize::try_from(pages_delta).ok() else {
            return Ok(None);
        };
        let Some(new_pages) = current_pages.checked_add(pages_delta) else {
            return Ok(None);
        };
        let max_pages = self.kind.page_count_max().try_into().unwrap_or(usize::MAX);

        if new_pages > max_pages {
            cold_path();
            crate::log::debug!("memory.grow failed: new_pages={}, max_pages={}", new_pages, max_pages);
            return Ok(None);
        }

        let Some(new_size) = (new_pages as u64).checked_mul(self.kind.page_size()) else {
            return Ok(None);
        };
        if new_size > self.kind.max_size() {
            cold_path();
            crate::log::debug!("memory.grow failed: new_size={}, max_size={}", new_size, self.kind.max_size());
            return Ok(None);
        }

        let Some(new_size) = usize::try_from(new_size).ok() else {
            return Ok(None);
        };
        if new_size == self.inner.len() {
            return Ok(i64::try_from(current_pages).ok());
        }

        if let Err(err) = self.inner.grow_to(new_size) {
            if trap_on_oom {
                return Err(err);
            }
            return Ok(None);
        }
        self.page_count = new_pages;
        Ok(i64::try_from(current_pages).ok())
    }
}

use alloc::format;
use tinywasm_types::MemoryArch;
use tinywasm_types::MemoryType;

use crate::Error;
use crate::MemoryBackend;
use crate::Result;
use crate::Trap;

use super::MemoryStorage;
use super::memory_oob;
use core::hint::cold_path;

/// A WebAssembly Memory Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#memory-instances>
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct MemoryInstance {
    pub(crate) kind: MemoryType,
    pub(crate) inner: MemoryStorage,
    pub(crate) page_count: usize,
}

impl MemoryInstance {
    const COPY_CHUNK_SIZE: usize = 4 * 1024;

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

    pub(crate) const fn is_64bit(&self) -> bool {
        matches!(self.kind.arch(), MemoryArch::I64)
    }

    #[inline(always)]
    pub(crate) fn load<const SIZE: usize>(&self, base: u64, offset: u64) -> Result<[u8; SIZE], Trap> {
        // the compiler doesn't optimize .as_slice().try_into() for some reason, so we have to manually copy the bytes into an array
        // heavy usage of cold_path() seems to help a lot from looking at profile data
        match SIZE {
            1 => {
                let res = match self.inner.read_8(base, offset) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        cold_path();
                        return Err(e);
                    }
                };
                let mut bytes = [0; SIZE];
                bytes[0] = res;
                Ok(bytes)
            }
            2 => {
                let res = match self.inner.read_16(base, offset) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        cold_path();
                        return Err(e);
                    }
                };
                let mut bytes = [0; SIZE];
                bytes[0] = res[0];
                bytes[1] = res[1];
                Ok(bytes)
            }
            4 => {
                let mut bytes = [0; SIZE];
                let res = match self.inner.read_32(base, offset) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        cold_path();
                        return Err(e);
                    }
                };
                bytes[0] = res[0];
                bytes[1] = res[1];
                bytes[2] = res[2];
                bytes[3] = res[3];
                Ok(bytes)
            }
            8 => {
                let mut bytes = [0; SIZE];
                let res = match self.inner.read_64(base, offset) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        cold_path();
                        return Err(e);
                    }
                };
                bytes[0] = res[0];
                bytes[1] = res[1];
                bytes[2] = res[2];
                bytes[3] = res[3];
                bytes[4] = res[4];
                bytes[5] = res[5];
                bytes[6] = res[6];
                bytes[7] = res[7];
                Ok(bytes)
            }
            16 => {
                let mut bytes = [0; SIZE];
                let res = match self.inner.read_128(base, offset) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        cold_path();
                        return Err(e);
                    }
                };
                bytes[0] = res[0];
                bytes[1] = res[1];
                bytes[2] = res[2];
                bytes[3] = res[3];
                bytes[4] = res[4];
                bytes[5] = res[5];
                bytes[6] = res[6];
                bytes[7] = res[7];
                bytes[8] = res[8];
                bytes[9] = res[9];
                bytes[10] = res[10];
                bytes[11] = res[11];
                bytes[12] = res[12];
                bytes[13] = res[13];
                bytes[14] = res[14];
                bytes[15] = res[15];
                Ok(bytes)
            }
            _ => unreachable!("unsupported fixed-size read width {SIZE}"),
        }
    }

    #[inline(always)]
    pub(crate) fn store<const SIZE: usize>(&mut self, base: u64, offset: u64, bytes: [u8; SIZE]) -> Result<(), Trap> {
        // the compiler doesn't optimize .as_slice().try_into() for some reason, so we have to manually copy the bytes into an array
        // heavy usage of cold_path() seems to help a lot from looking at profile data
        let res = match SIZE {
            1 => self.inner.write_8(base, offset, bytes[0]),
            2 => self.inner.write_16(base, offset, [bytes[0], bytes[1]]),
            4 => self.inner.write_32(base, offset, [bytes[0], bytes[1], bytes[2], bytes[3]]),
            8 => self.inner.write_64(
                base,
                offset,
                [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]],
            ),
            16 => self.inner.write_128(
                base,
                offset,
                [
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9],
                    bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
                ],
            ),
            _ => unreachable!("unsupported fixed-size write width {SIZE}"),
        };

        if let Err(e) = res {
            cold_path();
            return Err(e);
        }
        Ok(())
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

    pub(crate) fn grow(&mut self, pages_delta: i64) -> Option<i64> {
        if pages_delta < 0 {
            cold_path();
            crate::log::debug!("memory.grow failed: negative delta {}", pages_delta);
            return None;
        }

        let current_pages = self.page_count;
        let pages_delta = usize::try_from(pages_delta).ok()?;
        let new_pages = current_pages.checked_add(pages_delta)?;
        let max_pages = self.kind.page_count_max().try_into().unwrap_or(usize::MAX);

        if new_pages > max_pages {
            cold_path();
            crate::log::debug!("memory.grow failed: new_pages={}, max_pages={}", new_pages, max_pages);
            return None;
        }

        let new_size = (new_pages as u64).checked_mul(self.kind.page_size())?;
        if new_size > self.kind.max_size() {
            cold_path();
            crate::log::debug!("memory.grow failed: new_size={}, max_size={}", new_size, self.kind.max_size());
            return None;
        }

        let new_size = usize::try_from(new_size).ok()?;
        if new_size == self.inner.len() {
            return i64::try_from(current_pages).ok();
        }

        self.inner.grow_to(new_size)?;
        self.page_count = new_pages;
        i64::try_from(current_pages).ok()
    }
}

use alloc::vec::Vec;
use core::ops::Range;
use tinywasm_types::MemoryArch;

use super::memory_oob;

/// A contiguous `Vec<u8>`-backed linear memory storage.
///
/// This is the default internal storage for [`super::MemoryInstance`].
pub(crate) struct VecMemory(Vec<u8>);

impl VecMemory {
    /// Returns a raw mutable pointer to the memory's backing allocation.
    /// Growth may invalidate the pointer. The caller must keep the store alive
    /// and uphold Rust's aliasing rules when dereferencing it.
    pub(crate) fn data_ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr()
    }

    /// Borrows the backing bytes.
    pub(crate) fn data(&self) -> &[u8] {
        &self.0
    }

    /// Borrows the backing bytes exclusively.
    pub(crate) fn data_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }

    /// Tries to create a new memory with `len` zero-initialized bytes.
    pub(crate) fn try_new(_arch: MemoryArch, len: usize, _max_len: usize) -> Result<Self, crate::Trap> {
        let mut data = Vec::new();
        cold_err!(data.try_reserve(len)).map_err(|_| crate::Trap::OutOfMemory)?;
        data.resize(len, 0);
        Ok(Self(data))
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// Resolves the memory offset while retaining the byte length for bounds errors.
    #[inline(always)]
    pub(crate) fn effective_addr<const N: usize>(&self, base: usize, offset: u64) -> Result<usize, crate::Trap> {
        #[cfg(target_pointer_width = "64")]
        let address = base.checked_add(offset as usize);
        #[cfg(not(target_pointer_width = "64"))]
        let address = usize::try_from(offset).ok().and_then(|offset| base.checked_add(offset));
        address.ok_or_else(|| memory_oob(base, N, self.len()))
    }

    #[inline(always)]
    pub(crate) fn checked_range(&self, addr: usize, len: usize) -> Option<Range<usize>> {
        let end = addr.checked_add(len)?;
        (end <= self.0.len()).then_some(addr..end)
    }

    /// Grows the backing allocation to `new_len`. Only called after the Wasm limits and any user
    /// limiter have accepted the grow.
    #[inline(always)]
    pub(crate) fn grow_to(&mut self, new_len: usize) -> Result<(), crate::Trap> {
        debug_assert!(new_len >= self.0.len(), "memory only grows");
        cold_err!(self.0.try_reserve(new_len - self.0.len())).map_err(|_| crate::Trap::OutOfMemory)?;
        self.0.resize(new_len, 0);
        Ok(())
    }

    /// Reads exactly `N` bytes at `addr` into a fixed-size array.
    #[inline(always)]
    pub(crate) fn read_fixed<const N: usize>(&self, addr: usize) -> Result<[u8; N], crate::Trap> {
        if N > self.0.len() || addr > self.0.len() - N {
            return cold!(Err(memory_oob(addr, N, self.0.len())));
        }
        let mut bytes = [0u8; N];
        bytes.copy_from_slice(&self.0[addr..addr + N]);
        Ok(bytes)
    }

    /// Writes exactly `N` bytes from `bytes` at `addr`.
    #[inline(always)]
    pub(crate) fn write_fixed<const N: usize>(&mut self, addr: usize, bytes: &[u8; N]) -> Result<(), crate::Trap> {
        if N > self.0.len() || addr > self.0.len() - N {
            return cold!(Err(memory_oob(addr, N, self.0.len())));
        }
        self.0[addr..addr + N].copy_from_slice(bytes);
        Ok(())
    }

    /// Reads up to `dst.len()` bytes starting at `addr` and returns the number of bytes read.
    #[inline(always)]
    pub(crate) fn read(&self, addr: usize, dst: &mut [u8]) -> usize {
        if addr >= self.0.len() {
            return 0;
        }
        let read_len = dst.len().min(self.0.len() - addr);
        dst[..read_len].copy_from_slice(&self.0[addr..addr + read_len]);
        read_len
    }

    /// Writes up to `src.len()` bytes starting at `addr` and returns the number of bytes written.
    #[inline(always)]
    pub(crate) fn write(&mut self, addr: usize, src: &[u8]) -> usize {
        if addr >= self.0.len() {
            return 0;
        }
        let write_len = src.len().min(self.0.len() - addr);
        self.0[addr..addr + write_len].copy_from_slice(&src[..write_len]);
        write_len
    }

    /// Reads exactly `dst.len()` bytes starting at `addr`, returning `None` for an invalid range.
    #[inline(always)]
    pub(crate) fn read_exact(&self, addr: usize, dst: &mut [u8]) -> Option<()> {
        dst.copy_from_slice(&self.0[self.checked_range(addr, dst.len())?]);
        Some(())
    }

    /// Reads `len` bytes starting at `addr` into a newly allocated buffer, returning `None` for an
    /// invalid range.
    #[inline(always)]
    pub(crate) fn read_vec(&self, addr: usize, len: usize) -> Option<Vec<u8>> {
        Some(self.0[self.checked_range(addr, len)?].to_vec())
    }

    /// Writes all of `src` at `addr`, returning `None` for an invalid range.
    #[inline(always)]
    pub(crate) fn write_all(&mut self, addr: usize, src: &[u8]) -> Option<()> {
        let range = self.checked_range(addr, src.len())?;
        self.0[range].copy_from_slice(src);
        Some(())
    }

    /// Fills the range `[addr, addr + len)` with `val`, returning `None` for an invalid range.
    #[inline(always)]
    pub(crate) fn fill(&mut self, addr: usize, len: usize, val: u8) -> Option<()> {
        let range = self.checked_range(addr, len)?;
        self.0[range].fill(val);
        Some(())
    }

    /// Copies `len` bytes from `src` to `dst` within the memory, returning `None` for an invalid
    /// range.
    #[inline(always)]
    pub(crate) fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Option<()> {
        let src = self.checked_range(src, len)?;
        self.checked_range(dst, len)?;
        self.0.copy_within(src, dst);
        Some(())
    }

    /// Copies from another memory, checking both ranges before mutation and reporting source errors first.
    #[inline(always)]
    pub(super) fn copy_from(
        &mut self,
        dst: usize,
        src_memory: &Self,
        src: usize,
        len: usize,
    ) -> Result<(), crate::Trap> {
        let src_range =
            cold_err!(src_memory.checked_range(src, len).ok_or_else(|| memory_oob(src, len, src_memory.len())))?;
        let dst_range = cold_err!(self.checked_range(dst, len).ok_or_else(|| memory_oob(dst, len, self.len())))?;
        self.0[dst_range].copy_from_slice(&src_memory.0[src_range]);
        Ok(())
    }
}

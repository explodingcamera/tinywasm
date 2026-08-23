use alloc::vec::Vec;
use core::ops::Range;

use super::memory_oob;

/// A contiguous `Vec<u8>`-backed linear memory storage.
///
/// This is the internal storage boundary for [`super::MemoryInstance`]. Keeping it a concrete type
/// rather than a `Vec<u8>` directly means the backing representation can later be swapped for an
/// mmap-backed implementation without touching the interpreter's load and store paths.
pub(crate) struct VecMemory {
    data: Vec<u8>,
}

impl VecMemory {
    /// Tries to create a new memory with `len` zero-initialized bytes.
    pub(crate) fn try_new(len: usize) -> Result<Self, crate::Trap> {
        let mut data = Vec::new();
        cold_err!(data.try_reserve_exact(len)).map_err(|_| crate::Trap::OutOfMemory)?;
        data.resize(len, 0);
        Ok(Self { data })
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }

    #[inline(always)]
    pub(super) fn checked_range(&self, addr: usize, len: usize) -> Option<Range<usize>> {
        let end = addr.checked_add(len)?;
        (end <= self.data.len()).then_some(addr..end)
    }

    /// Grows the backing allocation to `new_len`. Only called after the Wasm limits and any user
    /// limiter have accepted the grow.
    #[inline(always)]
    pub(crate) fn grow_to(&mut self, new_len: usize) -> Result<(), crate::Trap> {
        debug_assert!(new_len >= self.data.len(), "memory only grows");
        cold_err!(self.data.try_reserve_exact(new_len - self.data.len())).map_err(|_| crate::Trap::OutOfMemory)?;
        self.data.resize(new_len, 0);
        Ok(())
    }

    #[inline(always)]
    fn check_fixed_addr<const N: usize>(&self, addr: usize) -> Result<(), crate::Trap> {
        if N > self.data.len() || addr > self.data.len() - N {
            return cold!(Err(memory_oob(addr, N, self.data.len())));
        }
        Ok(())
    }

    /// Reads exactly `N` bytes at `addr` into a fixed-size array.
    #[inline(always)]
    pub(crate) fn read_fixed<const N: usize>(&self, addr: usize) -> Result<[u8; N], crate::Trap> {
        self.check_fixed_addr::<N>(addr)?;
        let mut bytes = [0u8; N];
        bytes.copy_from_slice(&self.data[addr..addr + N]);
        Ok(bytes)
    }

    /// Writes exactly `N` bytes from `bytes` at `addr`.
    #[inline(always)]
    pub(crate) fn write_fixed<const N: usize>(&mut self, addr: usize, bytes: &[u8; N]) -> Result<(), crate::Trap> {
        self.check_fixed_addr::<N>(addr)?;
        self.data[addr..addr + N].copy_from_slice(bytes);
        Ok(())
    }

    /// Reads up to `dst.len()` bytes starting at `addr` and returns the number of bytes read.
    #[inline(always)]
    pub(crate) fn read(&self, addr: usize, dst: &mut [u8]) -> usize {
        if addr >= self.data.len() {
            return 0;
        }
        let read_len = dst.len().min(self.data.len() - addr);
        dst[..read_len].copy_from_slice(&self.data[addr..addr + read_len]);
        read_len
    }

    /// Writes up to `src.len()` bytes starting at `addr` and returns the number of bytes written.
    #[inline(always)]
    pub(crate) fn write(&mut self, addr: usize, src: &[u8]) -> usize {
        if addr >= self.data.len() {
            return 0;
        }
        let write_len = src.len().min(self.data.len() - addr);
        self.data[addr..addr + write_len].copy_from_slice(&src[..write_len]);
        write_len
    }

    /// Reads exactly `dst.len()` bytes starting at `addr`, returning `None` for an invalid range.
    #[inline(always)]
    pub(crate) fn read_exact(&self, addr: usize, dst: &mut [u8]) -> Option<()> {
        dst.copy_from_slice(&self.data[self.checked_range(addr, dst.len())?]);
        Some(())
    }

    /// Reads `len` bytes starting at `addr` into a newly allocated buffer, returning `None` for an
    /// invalid range.
    #[inline(always)]
    pub(crate) fn read_vec(&self, addr: usize, len: usize) -> Option<Vec<u8>> {
        Some(self.data[self.checked_range(addr, len)?].to_vec())
    }

    /// Writes all of `src` at `addr`, returning `None` for an invalid range.
    #[inline(always)]
    pub(crate) fn write_all(&mut self, addr: usize, src: &[u8]) -> Option<()> {
        let range = self.checked_range(addr, src.len())?;
        self.data[range].copy_from_slice(src);
        Some(())
    }

    /// Fills the range `[addr, addr + len)` with `val`, returning `None` for an invalid range.
    #[inline(always)]
    pub(crate) fn fill(&mut self, addr: usize, len: usize, val: u8) -> Option<()> {
        let range = self.checked_range(addr, len)?;
        self.data[range].fill(val);
        Some(())
    }

    /// Copies `len` bytes from `src` to `dst` within the memory, returning `None` for an invalid
    /// range.
    #[inline(always)]
    pub(crate) fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Option<()> {
        let src = self.checked_range(src, len)?;
        self.checked_range(dst, len)?;
        self.data.copy_within(src, dst);
        Some(())
    }

    /// Copies a previously checked range from another memory.
    #[inline(always)]
    pub(super) fn copy_from(&mut self, dst: usize, src_memory: &Self, src: usize, len: usize) {
        self.data[dst..dst + len].copy_from_slice(&src_memory.data[src..src + len]);
    }
}

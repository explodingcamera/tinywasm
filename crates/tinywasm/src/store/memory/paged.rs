use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cmp::min;
use core::hint::cold_path;

use super::{LinearMemory, memory_oob};

/// A sparse chunked linear memory.
///
/// This backend stores memory in fixed-size chunks, which makes growth cheaper because it avoids
/// resizing and copying one large contiguous buffer.
///
/// The tradeoff is that reads and writes do a bit more bookkeeping and may need to cross chunk
/// boundaries, so they are usually slightly slower than [`super::VecMemory`].
///
/// In particular, [`LinearMemory::read`] and [`LinearMemory::write`] return at most the bytes up to
/// the end of the current chunk. Higher-level exact helpers loop over these short operations when
/// they need a full range.
pub struct PagedMemory {
    len: usize,
    chunk_size: usize,
    chunk_shift: u32,
    chunk_mask: usize,
    chunks: Vec<Option<Box<[u8]>>>,
}

impl PagedMemory {
    /// Tries to create a new sparse memory with `len` addressable bytes and the given `chunk_size`.
    ///
    /// Prefer this backend when grow behavior matters more than absolute read and write speed.
    pub fn try_new(len: usize, chunk_size: usize) -> Result<Self, crate::Trap> {
        assert!(chunk_size.is_power_of_two(), "chunk_size must be a power of two");

        let mut memory = Self {
            len: 0,
            chunk_size,
            chunk_shift: chunk_size.trailing_zeros(),
            chunk_mask: chunk_size - 1,
            chunks: Vec::new(),
        };
        memory.grow_to(len)?;
        Ok(memory)
    }

    #[inline(always)]
    fn allocate_chunk(&self) -> Result<Box<[u8]>, crate::Trap> {
        let mut chunk = Vec::new();
        cold_err!(chunk.try_reserve_exact(self.chunk_size)).map_err(|_| crate::Trap::OutOfMemory)?;
        chunk.resize(self.chunk_size, 0);
        Ok(chunk.into_boxed_slice())
    }

    #[inline(always)]
    fn chunk_mut(&mut self, chunk_idx: usize) -> Result<&mut [u8], crate::Trap> {
        if self.chunks[chunk_idx].is_none() {
            self.chunks[chunk_idx] = Some(self.allocate_chunk()?);
        }

        Ok(self.chunks[chunk_idx].as_deref_mut().unwrap_or_else(|| unreachable!()))
    }

    #[inline(always)]
    fn chunk_slice(&self, chunk_idx: usize) -> Option<&[u8]> {
        self.chunks[chunk_idx].as_deref()
    }

    #[inline(always)]
    fn read_fixed<const N: usize>(&self, addr: usize) -> Result<[u8; N], crate::Trap> {
        let Some(end) = addr.checked_add(N).filter(|end| *end <= self.len) else {
            return cold!(Err(memory_oob(addr, N, self.len)));
        };
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        if end <= ((chunk_idx + 1) << self.chunk_shift) {
            let mut bytes = [0; N];
            if let Some(chunk) = self.chunk_slice(chunk_idx) {
                bytes.copy_from_slice(&chunk[chunk_offset..chunk_offset + N]);
            }
            return Ok(bytes);
        }
        cold_path();
        let mut bytes = [0; N];
        self.read_exact(addr, &mut bytes).ok_or_else(|| memory_oob(addr, N, self.len))?;
        Ok(bytes)
    }

    #[inline(always)]
    fn write_fixed<const N: usize>(&mut self, addr: usize, bytes: &[u8]) -> Result<(), crate::Trap> {
        let Some(end) = addr.checked_add(N).filter(|end| *end <= self.len) else {
            return cold!(Err(memory_oob(addr, N, self.len)));
        };
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        if end <= ((chunk_idx + 1) << self.chunk_shift) {
            self.chunk_mut(chunk_idx)?[chunk_offset..chunk_offset + N].copy_from_slice(bytes);
            return Ok(());
        }
        cold_path();
        self.write_all(addr, bytes)?.ok_or_else(|| memory_oob(addr, N, self.len))
    }

    #[inline(always)]
    fn checked_end(&self, addr: usize, len: usize) -> Option<usize> {
        let end = addr.checked_add(len)?;
        if end > self.len {
            return None;
        }
        Some(end)
    }

    #[inline(always)]
    fn copy_within_single_chunk(&mut self, dst: usize, src: usize, len: usize) -> bool {
        if len == 0 {
            return true;
        }

        if self.checked_end(src, len).is_none() || self.checked_end(dst, len).is_none() {
            return false;
        }

        let src_chunk_idx = src >> self.chunk_shift;
        let dst_chunk_idx = dst >> self.chunk_shift;
        if src_chunk_idx != dst_chunk_idx {
            return false;
        }

        let src_offset = src & self.chunk_mask;
        let dst_offset = dst & self.chunk_mask;
        if src_offset + len > self.chunk_size || dst_offset + len > self.chunk_size {
            return false;
        }

        if let Some(Some(chunk)) = self.chunks.get_mut(src_chunk_idx) {
            chunk.copy_within(src_offset..src_offset + len, dst_offset);
        }

        true
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for PagedMemory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let allocated_chunks = self.chunks.iter().filter(|chunk| chunk.is_some()).count();
        f.debug_struct("PagedMemory")
            .field("len", &self.len)
            .field("chunk_size", &self.chunk_size)
            .field("allocated_chunks", &allocated_chunks)
            .finish()
    }
}

impl LinearMemory for PagedMemory {
    #[inline(always)]
    fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    fn grow_to(&mut self, new_len: usize) -> Result<(), crate::Trap> {
        if new_len < self.len {
            return Err(crate::Trap::MemoryOutOfBounds { offset: new_len, len: 0, max: self.len });
        }

        let new_chunk_count = if new_len == 0 { 0 } else { new_len.div_ceil(self.chunk_size) };
        if new_chunk_count > self.chunks.len() {
            cold_err!(self.chunks.try_reserve_exact(new_chunk_count - self.chunks.len()))
                .map_err(|_| crate::Trap::OutOfMemory)?;
            self.chunks.resize_with(new_chunk_count, || None);
        } else {
            self.chunks.truncate(new_chunk_count);
        }

        self.len = new_len;
        Ok(())
    }

    #[inline(always)]
    fn read(&self, addr: usize, dst: &mut [u8]) -> usize {
        if addr >= self.len || dst.is_empty() {
            return 0;
        }

        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        let chunk_end = min((chunk_idx + 1) << self.chunk_shift, self.len);
        let read_len = min(chunk_end - addr, dst.len());
        if let Some(chunk) = self.chunk_slice(chunk_idx) {
            dst[..read_len].copy_from_slice(&chunk[chunk_offset..chunk_offset + read_len]);
        } else {
            dst[..read_len].fill(0);
        }

        read_len
    }

    #[inline(always)]
    fn write(&mut self, addr: usize, src: &[u8]) -> Result<usize, crate::Trap> {
        if addr >= self.len || src.is_empty() {
            return Ok(0);
        }

        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        let write_len = min(min(self.chunk_size - chunk_offset, self.len - addr), src.len());

        let chunk = self.chunk_mut(chunk_idx)?;
        chunk[chunk_offset..chunk_offset + write_len].copy_from_slice(&src[..write_len]);
        Ok(write_len)
    }

    #[inline(always)]
    fn write_all(&mut self, addr: usize, src: &[u8]) -> Result<Option<()>, crate::Trap> {
        let Some(end) = self.checked_end(addr, src.len()) else { return Ok(None) };
        let mut pos = addr;
        let mut src_offset = 0;

        while pos < end {
            let chunk_idx = pos >> self.chunk_shift;
            let chunk_offset = pos & self.chunk_mask;
            let copy_len = min(self.chunk_size - chunk_offset, end - pos);

            let chunk = self.chunk_mut(chunk_idx)?;
            chunk[chunk_offset..chunk_offset + copy_len].copy_from_slice(&src[src_offset..src_offset + copy_len]);

            pos += copy_len;
            src_offset += copy_len;
        }

        Ok(Some(()))
    }

    #[inline(always)]
    fn fill(&mut self, addr: usize, len: usize, val: u8) -> Result<Option<()>, crate::Trap> {
        let Some(end) = self.checked_end(addr, len) else { return Ok(None) };
        let mut pos = addr;

        while pos < end {
            let chunk_idx = pos >> self.chunk_shift;
            let chunk_offset = pos & self.chunk_mask;
            let chunk_start = chunk_idx << self.chunk_shift;
            let chunk_full_len = min(self.chunk_size, self.len - chunk_start);
            let chunk_end = min(chunk_start + self.chunk_size, end);
            let fill_len = chunk_end - pos;

            if val == 0 {
                if chunk_offset == 0 && fill_len == chunk_full_len {
                    self.chunks[chunk_idx] = None;
                } else if let Some(Some(chunk)) = self.chunks.get_mut(chunk_idx) {
                    chunk[chunk_offset..chunk_offset + fill_len].fill(0);
                }
            } else {
                self.chunk_mut(chunk_idx)?[chunk_offset..chunk_offset + fill_len].fill(val);
            }

            pos = chunk_end;
        }

        Ok(Some(()))
    }

    #[inline(always)]
    fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Result<Option<()>, crate::Trap> {
        if self.checked_end(src, len).is_none() || self.checked_end(dst, len).is_none() {
            return Ok(None);
        }

        if len == 0 || dst == src {
            return Ok(Some(()));
        }

        if self.copy_within_single_chunk(dst, src, len) {
            return Ok(Some(()));
        }

        let mut buf = [0u8; 256];

        if dst < src || dst >= src + len {
            let mut copied = 0;
            while copied < len {
                let chunk_len = min(buf.len(), len - copied);
                if self.read_exact(src + copied, &mut buf[..chunk_len]).is_none()
                    || self.write_all(dst + copied, &buf[..chunk_len])?.is_none()
                {
                    return Ok(None);
                }
                copied += chunk_len;
            }
        } else {
            let mut remaining = len;
            while remaining > 0 {
                let chunk_len = min(buf.len(), remaining);
                let chunk_start = remaining - chunk_len;
                if self.read_exact(src + chunk_start, &mut buf[..chunk_len]).is_none()
                    || self.write_all(dst + chunk_start, &buf[..chunk_len])?.is_none()
                {
                    return Ok(None);
                }
                remaining = chunk_start;
            }
        }

        Ok(Some(()))
    }

    #[inline(always)]
    fn read_8(&self, addr: usize) -> core::result::Result<[u8; 1], crate::Trap> {
        if addr >= self.len {
            cold_path();
            return Err(memory_oob(addr, 1, self.len));
        }
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        Ok([self.chunk_slice(chunk_idx).map_or(0, |chunk| chunk[chunk_offset])])
    }

    #[inline(always)]
    fn read_16(&self, addr: usize) -> core::result::Result<[u8; 2], crate::Trap> {
        self.read_fixed::<2>(addr)
    }

    #[inline(always)]
    fn read_32(&self, addr: usize) -> core::result::Result<[u8; 4], crate::Trap> {
        self.read_fixed::<4>(addr)
    }

    #[inline(always)]
    fn read_64(&self, addr: usize) -> core::result::Result<[u8; 8], crate::Trap> {
        self.read_fixed::<8>(addr)
    }

    #[inline(always)]
    fn read_128(&self, addr: usize) -> core::result::Result<[u8; 16], crate::Trap> {
        self.read_fixed::<16>(addr)
    }

    #[inline(always)]
    fn write_8(&mut self, addr: usize, bytes: &[u8]) -> core::result::Result<(), crate::Trap> {
        if addr >= self.len {
            cold_path();
            return Err(memory_oob(addr, 1, self.len));
        }
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        self.chunk_mut(chunk_idx)?[chunk_offset] = bytes[0];
        Ok(())
    }

    #[inline(always)]
    fn write_16(&mut self, addr: usize, bytes: &[u8]) -> core::result::Result<(), crate::Trap> {
        self.write_fixed::<2>(addr, bytes)
    }

    #[inline(always)]
    fn write_32(&mut self, addr: usize, bytes: &[u8]) -> core::result::Result<(), crate::Trap> {
        self.write_fixed::<4>(addr, bytes)
    }

    #[inline(always)]
    fn write_64(&mut self, addr: usize, bytes: &[u8]) -> core::result::Result<(), crate::Trap> {
        self.write_fixed::<8>(addr, bytes)
    }

    #[inline(always)]
    fn write_128(&mut self, addr: usize, bytes: &[u8]) -> core::result::Result<(), crate::Trap> {
        self.write_fixed::<16>(addr, bytes)
    }
}

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::min;

use super::{LinearMemory, checked_effective_addr};

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
    /// Creates a new sparse memory with `len` addressable bytes and the given `chunk_size`.
    ///
    /// Prefer this backend when grow behavior matters more than absolute read and write speed.
    pub fn new(len: usize, chunk_size: usize) -> Self {
        assert!(chunk_size.is_power_of_two(), "chunk_size must be a power of two");

        let mut memory = Self {
            len: 0,
            chunk_size,
            chunk_shift: chunk_size.trailing_zeros(),
            chunk_mask: chunk_size - 1,
            chunks: Vec::new(),
        };
        memory.grow_to(len).expect("initial length must be growable");
        memory
    }

    #[inline(always)]
    fn chunk_mut(&mut self, chunk_idx: usize) -> &mut [u8] {
        self.chunks[chunk_idx].get_or_insert_with(|| vec![0; self.chunk_size].into_boxed_slice()).as_mut()
    }

    #[inline(always)]
    fn chunk_slice(&self, chunk_idx: usize) -> Option<&[u8]> {
        self.chunks[chunk_idx].as_deref()
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
    fn grow_to(&mut self, new_len: usize) -> Option<()> {
        if new_len < self.len {
            return None;
        }
        self.chunks.resize_with(if new_len == 0 { 0 } else { new_len.div_ceil(self.chunk_size) }, || None);
        self.len = new_len;
        Some(())
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
    fn write(&mut self, addr: usize, src: &[u8]) -> usize {
        if addr >= self.len || src.is_empty() {
            return 0;
        }

        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        let write_len = min(min(self.chunk_size - chunk_offset, self.len - addr), src.len());

        let chunk = self.chunk_mut(chunk_idx);
        chunk[chunk_offset..chunk_offset + write_len].copy_from_slice(&src[..write_len]);
        write_len
    }

    #[inline(always)]
    fn write_all(&mut self, addr: usize, src: &[u8]) -> Option<()> {
        let end = self.checked_end(addr, src.len())?;
        let mut pos = addr;
        let mut src_offset = 0;

        while pos < end {
            let chunk_idx = pos >> self.chunk_shift;
            let chunk_offset = pos & self.chunk_mask;
            let copy_len = min(self.chunk_size - chunk_offset, end - pos);

            let chunk = self.chunk_mut(chunk_idx);
            chunk[chunk_offset..chunk_offset + copy_len].copy_from_slice(&src[src_offset..src_offset + copy_len]);

            pos += copy_len;
            src_offset += copy_len;
        }

        Some(())
    }

    #[inline(always)]
    fn fill(&mut self, addr: usize, len: usize, val: u8) -> Option<()> {
        let end = self.checked_end(addr, len)?;
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
                self.chunk_mut(chunk_idx)[chunk_offset..chunk_offset + fill_len].fill(val);
            }

            pos = chunk_end;
        }

        Some(())
    }

    #[inline(always)]
    fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Option<()> {
        self.checked_end(src, len)?;
        self.checked_end(dst, len)?;

        if len == 0 || dst == src {
            return Some(());
        }

        if self.copy_within_single_chunk(dst, src, len) {
            return Some(());
        }

        let mut buf = [0u8; 256];

        if dst < src || dst >= src + len {
            let mut copied = 0;
            while copied < len {
                let chunk_len = min(buf.len(), len - copied);
                self.read_exact(src + copied, &mut buf[..chunk_len])?;
                self.write_all(dst + copied, &buf[..chunk_len])?;
                copied += chunk_len;
            }
        } else {
            let mut remaining = len;
            while remaining > 0 {
                let chunk_len = min(buf.len(), remaining);
                let chunk_start = remaining - chunk_len;
                self.read_exact(src + chunk_start, &mut buf[..chunk_len])?;
                self.write_all(dst + chunk_start, &buf[..chunk_len])?;
                remaining = chunk_start;
            }
        }

        Some(())
    }

    #[inline(always)]
    fn read_8(&self, base: u64, offset: u64) -> core::result::Result<u8, crate::Trap> {
        let addr = checked_effective_addr::<1>(self.len, base, offset)?;
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        Ok(self.chunk_slice(chunk_idx).map_or(0, |chunk| chunk[chunk_offset]))
    }

    #[inline(always)]
    fn read_16(&self, base: u64, offset: u64) -> core::result::Result<[u8; 2], crate::Trap> {
        let addr = checked_effective_addr::<2>(self.len, base, offset)?;
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        if chunk_offset + 2 <= self.chunk_size {
            return Ok(match self.chunk_slice(chunk_idx) {
                Some(chunk) => chunk[chunk_offset..chunk_offset + 2].try_into().unwrap_or_else(|_| unreachable!()),
                None => [0; 2],
            });
        }

        let mut bytes = [0; 2];
        self.read_exact(addr, &mut bytes).unwrap();
        Ok(bytes)
    }

    #[inline(always)]
    fn read_32(&self, base: u64, offset: u64) -> core::result::Result<[u8; 4], crate::Trap> {
        let addr = checked_effective_addr::<4>(self.len, base, offset)?;
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        if chunk_offset + 4 <= self.chunk_size {
            return Ok(match self.chunk_slice(chunk_idx) {
                Some(chunk) => chunk[chunk_offset..chunk_offset + 4].try_into().unwrap_or_else(|_| unreachable!()),
                None => [0; 4],
            });
        }

        let mut bytes = [0; 4];
        self.read_exact(addr, &mut bytes).unwrap();
        Ok(bytes)
    }

    #[inline(always)]
    fn read_64(&self, base: u64, offset: u64) -> core::result::Result<[u8; 8], crate::Trap> {
        let addr = checked_effective_addr::<8>(self.len, base, offset)?;
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        if chunk_offset + 8 <= self.chunk_size {
            return Ok(match self.chunk_slice(chunk_idx) {
                Some(chunk) => chunk[chunk_offset..chunk_offset + 8].try_into().unwrap_or_else(|_| unreachable!()),
                None => [0; 8],
            });
        }

        let mut bytes = [0; 8];
        self.read_exact(addr, &mut bytes).unwrap();
        Ok(bytes)
    }

    #[inline(always)]
    fn read_128(&self, base: u64, offset: u64) -> core::result::Result<[u8; 16], crate::Trap> {
        let addr = checked_effective_addr::<16>(self.len, base, offset)?;
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        if chunk_offset + 16 <= self.chunk_size {
            return Ok(match self.chunk_slice(chunk_idx) {
                Some(chunk) => chunk[chunk_offset..chunk_offset + 16].try_into().unwrap_or_else(|_| unreachable!()),
                None => [0; 16],
            });
        }

        let mut bytes = [0; 16];
        self.read_exact(addr, &mut bytes).unwrap();
        Ok(bytes)
    }

    #[inline(always)]
    fn write_8(&mut self, base: u64, offset: u64, byte: u8) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<1>(self.len, base, offset)?;
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        self.chunk_mut(chunk_idx)[chunk_offset] = byte;
        Ok(())
    }

    #[inline(always)]
    fn write_16(&mut self, base: u64, offset: u64, bytes: [u8; 2]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<2>(self.len, base, offset)?;
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        if chunk_offset + 2 <= self.chunk_size {
            self.chunk_mut(chunk_idx)[chunk_offset..chunk_offset + 2].copy_from_slice(&bytes);
        } else {
            self.write_all(addr, &bytes).unwrap();
        }
        Ok(())
    }

    #[inline(always)]
    fn write_32(&mut self, base: u64, offset: u64, bytes: [u8; 4]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<4>(self.len, base, offset)?;
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        if chunk_offset + 4 <= self.chunk_size {
            self.chunk_mut(chunk_idx)[chunk_offset..chunk_offset + 4].copy_from_slice(&bytes);
        } else {
            self.write_all(addr, &bytes).unwrap();
        }
        Ok(())
    }

    #[inline(always)]
    fn write_64(&mut self, base: u64, offset: u64, bytes: [u8; 8]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<8>(self.len, base, offset)?;
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        if chunk_offset + 8 <= self.chunk_size {
            self.chunk_mut(chunk_idx)[chunk_offset..chunk_offset + 8].copy_from_slice(&bytes);
        } else {
            self.write_all(addr, &bytes).unwrap();
        }
        Ok(())
    }

    #[inline(always)]
    fn write_128(&mut self, base: u64, offset: u64, bytes: [u8; 16]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<16>(self.len, base, offset)?;
        let chunk_idx = addr >> self.chunk_shift;
        let chunk_offset = addr & self.chunk_mask;
        if chunk_offset + 16 <= self.chunk_size {
            self.chunk_mut(chunk_idx)[chunk_offset..chunk_offset + 16].copy_from_slice(&bytes);
        } else {
            self.write_all(addr, &bytes).unwrap();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{LinearMemory, PagedMemory};

    #[test]
    fn paged_memory_reads_zeroes_from_sparse_chunks() {
        let memory = PagedMemory::new(16, 4);
        let mut dst = [1; 6];
        assert_eq!(memory.read(5, &mut dst), 3);
        assert_eq!(&dst[..3], &[0; 3]);
        assert_eq!(&dst[3..], &[1; 3]);
    }

    #[test]
    fn paged_memory_store_and_load_crosses_chunk_boundaries() {
        let mut memory = PagedMemory::new(16, 4);
        memory.write_all(3, &[1, 2, 3, 4, 5, 6]).unwrap();

        let mut dst = [0; 6];
        memory.read_exact(3, &mut dst).unwrap();
        assert_eq!(dst, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn paged_memory_copy_within_handles_overlap() {
        let mut memory = PagedMemory::new(16, 4);
        memory.write_all(0, &[1, 2, 3, 4, 5, 6]).unwrap();
        memory.copy_within(2, 0, 6).unwrap();

        let mut dst = [0; 8];
        memory.read_exact(0, &mut dst).unwrap();
        assert_eq!(dst, [1, 2, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn paged_memory_write_stops_at_chunk_boundary() {
        let mut memory = PagedMemory::new(16, 4);
        assert_eq!(memory.write(3, &[1, 2, 3, 4]), 1);

        let mut dst = [0; 4];
        memory.read_exact(3, &mut dst).unwrap();
        assert_eq!(dst, [1, 0, 0, 0]);
    }
}

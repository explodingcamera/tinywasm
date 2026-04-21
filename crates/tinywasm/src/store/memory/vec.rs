use alloc::vec::Vec;
use core::hint::cold_path;

use super::{LinearMemory, checked_effective_addr};

/// A contiguous `Vec<u8>`-backed linear memory.
///
/// This is the simplest backend and typically gives the best read and write throughput because
/// the whole memory lives in one contiguous allocation.
///
/// The tradeoff is growth cost: large grows may need to reallocate and copy the full buffer,
/// which can get expensive for large memories.
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct VecMemory {
    data: Vec<u8>,
}

impl VecMemory {
    /// Tries to create a new memory with `len` zero-initialized bytes.
    ///
    /// Prefer this backend when contiguous access is more important than grow performance.
    pub fn try_new(len: usize) -> Result<Self, crate::Trap> {
        let mut data = Vec::new();
        match data.try_reserve_exact(len) {
            Ok(()) => {}
            Err(_) => {
                cold_path();
                return Err(crate::Trap::OutOfMemory);
            }
        }
        data.resize(len, 0);
        Ok(Self { data })
    }
}

impl LinearMemory for VecMemory {
    #[inline(always)]
    fn len(&self) -> usize {
        self.data.len()
    }

    #[inline(always)]
    fn grow_to(&mut self, new_len: usize) -> Result<(), crate::Trap> {
        if new_len < self.data.len() {
            return Err(crate::Trap::MemoryOutOfBounds { offset: new_len, len: 0, max: self.data.len() });
        }
        match self.data.try_reserve_exact(new_len.saturating_sub(self.data.len())) {
            Ok(()) => {}
            Err(_) => {
                cold_path();
                return Err(crate::Trap::OutOfMemory);
            }
        }
        self.data.resize(new_len, 0);
        Ok(())
    }

    #[inline(always)]
    fn read(&self, addr: usize, dst: &mut [u8]) -> usize {
        if addr >= self.data.len() {
            return 0;
        }
        let read_len = dst.len().min(self.data.len() - addr);
        dst[..read_len].copy_from_slice(&self.data[addr..addr + read_len]);
        read_len
    }

    #[inline(always)]
    fn read_exact(&self, addr: usize, dst: &mut [u8]) -> Option<()> {
        dst.copy_from_slice(self.data.get(addr..addr.checked_add(dst.len())?)?);
        Some(())
    }

    #[inline(always)]
    fn read_vec(&self, addr: usize, len: usize) -> Option<Vec<u8>> {
        Some(self.data.get(addr..addr.checked_add(len)?)?.to_vec())
    }

    #[inline(always)]
    fn write(&mut self, addr: usize, src: &[u8]) -> usize {
        if addr >= self.data.len() {
            return 0;
        }

        let write_len = src.len().min(self.data.len() - addr);
        self.data[addr..addr + write_len].copy_from_slice(&src[..write_len]);
        write_len
    }

    #[inline(always)]
    fn write_all(&mut self, addr: usize, src: &[u8]) -> Option<()> {
        let dst = self.data.get_mut(addr..addr.checked_add(src.len())?)?;
        dst.copy_from_slice(src);
        Some(())
    }

    #[inline(always)]
    fn fill(&mut self, addr: usize, len: usize, val: u8) -> Option<()> {
        self.data.get_mut(addr..addr.checked_add(len)?)?.fill(val);
        Some(())
    }

    #[inline(always)]
    fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Option<()> {
        let src_end = src.checked_add(len)?;
        let dst_end = dst.checked_add(len)?;
        if src_end > self.data.len() || dst_end > self.data.len() {
            return None;
        }

        self.data.copy_within(src..src_end, dst);
        Some(())
    }

    #[inline(always)]
    fn read_8(&self, base: u64, offset: u64) -> core::result::Result<u8, crate::Trap> {
        Ok(self.data[checked_effective_addr::<1>(self.data.len(), base, offset)?])
    }

    #[inline(always)]
    fn read_16(&self, base: u64, offset: u64) -> core::result::Result<[u8; 2], crate::Trap> {
        let addr = checked_effective_addr::<2>(self.data.len(), base, offset)?;
        Ok(self.data[addr..addr + 2].try_into().unwrap_or_else(|_| unreachable!()))
    }

    #[inline(always)]
    fn read_32(&self, base: u64, offset: u64) -> core::result::Result<[u8; 4], crate::Trap> {
        let addr = checked_effective_addr::<4>(self.data.len(), base, offset)?;
        Ok(self.data[addr..addr + 4].try_into().unwrap_or_else(|_| unreachable!()))
    }

    #[inline(always)]
    fn read_64(&self, base: u64, offset: u64) -> core::result::Result<[u8; 8], crate::Trap> {
        let addr = checked_effective_addr::<8>(self.data.len(), base, offset)?;
        Ok(self.data[addr..addr + 8].try_into().unwrap_or_else(|_| unreachable!()))
    }

    #[inline(always)]
    fn read_128(&self, base: u64, offset: u64) -> core::result::Result<[u8; 16], crate::Trap> {
        let addr = checked_effective_addr::<16>(self.data.len(), base, offset)?;
        Ok(self.data[addr..addr + 16].try_into().unwrap_or_else(|_| unreachable!()))
    }

    #[inline(always)]
    fn write_8(&mut self, base: u64, offset: u64, byte: u8) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<1>(self.data.len(), base, offset)?;
        self.data[addr] = byte;
        Ok(())
    }

    #[inline(always)]
    fn write_16(&mut self, base: u64, offset: u64, bytes: [u8; 2]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<2>(self.data.len(), base, offset)?;
        self.data[addr..addr + 2].copy_from_slice(&bytes);
        Ok(())
    }

    #[inline(always)]
    fn write_32(&mut self, base: u64, offset: u64, bytes: [u8; 4]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<4>(self.data.len(), base, offset)?;
        self.data[addr..addr + 4].copy_from_slice(&bytes);
        Ok(())
    }

    #[inline(always)]
    fn write_64(&mut self, base: u64, offset: u64, bytes: [u8; 8]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<8>(self.data.len(), base, offset)?;
        self.data[addr..addr + 8].copy_from_slice(&bytes);
        Ok(())
    }

    #[inline(always)]
    fn write_128(&mut self, base: u64, offset: u64, bytes: [u8; 16]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<16>(self.data.len(), base, offset)?;
        self.data[addr..addr + 16].copy_from_slice(&bytes);
        Ok(())
    }
}

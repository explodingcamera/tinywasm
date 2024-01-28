use alloc::vec;
use alloc::vec::Vec;
use tinywasm_types::{MemoryType, ModuleInstanceAddr};

use crate::{cold, unlikely, Error, Result};

pub(crate) const PAGE_SIZE: usize = 65536;
pub(crate) const MAX_PAGES: usize = 65536;
pub(crate) const MAX_SIZE: u64 = PAGE_SIZE as u64 * MAX_PAGES as u64;

/// A WebAssembly Memory Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#memory-instances>
#[derive(Debug)]
pub(crate) struct MemoryInstance {
    pub(crate) kind: MemoryType,
    pub(crate) data: Vec<u8>,
    pub(crate) page_count: usize,
    pub(crate) _owner: ModuleInstanceAddr, // index into store.module_instances
}

impl MemoryInstance {
    pub(crate) fn new(kind: MemoryType, owner: ModuleInstanceAddr) -> Self {
        assert!(kind.page_count_initial <= kind.page_count_max.unwrap_or(MAX_PAGES as u64));
        log::debug!("initializing memory with {} pages", kind.page_count_initial);

        Self {
            kind,
            data: vec![0; PAGE_SIZE * kind.page_count_initial as usize],
            page_count: kind.page_count_initial as usize,
            _owner: owner,
        }
    }

    pub(crate) fn store(&mut self, addr: usize, _align: usize, data: &[u8], len: usize) -> Result<()> {
        let Some(end) = addr.checked_add(len) else {
            cold();
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds {
                offset: addr,
                len: data.len(),
                max: self.data.len(),
            }));
        };

        if unlikely(end > self.data.len() || end < addr) {
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds {
                offset: addr,
                len: data.len(),
                max: self.data.len(),
            }));
        }

        // WebAssembly doesn't require alignment for stores
        #[cfg(not(feature = "unsafe"))]
        self.data[addr..end].copy_from_slice(data);

        #[cfg(feature = "unsafe")]
        // SAFETY: we checked that `end` is in bounds above, this is the same as `copy_from_slice`
        // src must is for reads of count * size_of::<T>() bytes.
        // dst must is for writes of count * size_of::<T>() bytes.
        // Both src and dst are properly aligned.
        // The region of memory beginning at src does not overlap with the region of memory beginning at dst with the same size.
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), self.data[addr..end].as_mut_ptr(), len);
        }

        Ok(())
    }

    pub(crate) fn max_pages(&self) -> usize {
        self.kind.page_count_max.unwrap_or(MAX_PAGES as u64) as usize
    }

    pub(crate) fn load(&self, addr: usize, _align: usize, len: usize) -> Result<&[u8]> {
        let Some(end) = addr.checked_add(len) else {
            cold();
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds { offset: addr, len, max: self.data.len() }));
        };

        if unlikely(end > self.data.len() || end < addr) {
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds { offset: addr, len, max: self.data.len() }));
        }

        Ok(&self.data[addr..end])
    }

    // this is a workaround since we can't use generic const expressions yet (https://github.com/rust-lang/rust/issues/76560)
    pub(crate) fn load_as<const SIZE: usize, T: MemLoadable<SIZE>>(&self, addr: usize, _align: usize) -> Result<T> {
        let Some(end) = addr.checked_add(SIZE) else {
            cold();
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds { offset: addr, len: SIZE, max: self.max_pages() }));
        };

        if unlikely(end > self.data.len()) {
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds { offset: addr, len: SIZE, max: self.data.len() }));
        }

        #[cfg(feature = "unsafe")]
        // WebAssembly doesn't require alignment for loads
        // SAFETY: we checked that `end` is in bounds above. All types that implement `Into<RawWasmValue>` are valid
        // to load from unaligned addresses.
        let val = unsafe { core::ptr::read_unaligned(self.data[addr..end].as_ptr() as *const T) };

        #[cfg(not(feature = "unsafe"))]
        let val = T::from_le_bytes(self.data[addr..end].try_into().expect("slice size mismatch"));

        Ok(val)
    }

    pub(crate) fn page_count(&self) -> usize {
        self.page_count
    }

    pub(crate) fn fill(&mut self, addr: usize, len: usize, val: u8) -> Result<()> {
        let end = addr
            .checked_add(len)
            .ok_or_else(|| Error::Trap(crate::Trap::MemoryOutOfBounds { offset: addr, len, max: self.data.len() }))?;
        if unlikely(end > self.data.len()) {
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds { offset: addr, len, max: self.data.len() }));
        }

        self.data[addr..end].fill(val);
        Ok(())
    }

    pub(crate) fn copy_from_slice(&mut self, dst: usize, src: &[u8]) -> Result<()> {
        let end = dst.checked_add(src.len()).ok_or_else(|| {
            Error::Trap(crate::Trap::MemoryOutOfBounds { offset: dst, len: src.len(), max: self.data.len() })
        })?;
        if unlikely(end > self.data.len()) {
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds {
                offset: dst,
                len: src.len(),
                max: self.data.len(),
            }));
        }

        self.data[dst..end].copy_from_slice(src);
        Ok(())
    }

    pub(crate) fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Result<()> {
        // Calculate the end of the source slice
        let src_end = src
            .checked_add(len)
            .ok_or_else(|| Error::Trap(crate::Trap::MemoryOutOfBounds { offset: src, len, max: self.data.len() }))?;
        if src_end > self.data.len() {
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds { offset: src, len, max: self.data.len() }));
        }

        // Calculate the end of the destination slice
        let dst_end = dst
            .checked_add(len)
            .ok_or_else(|| Error::Trap(crate::Trap::MemoryOutOfBounds { offset: dst, len, max: self.data.len() }))?;
        if dst_end > self.data.len() {
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds { offset: dst, len, max: self.data.len() }));
        }

        // Perform the copy
        self.data.copy_within(src..src_end, dst);
        Ok(())
    }

    pub(crate) fn grow(&mut self, pages_delta: i32) -> Option<i32> {
        let current_pages = self.page_count();
        let new_pages = current_pages as i64 + pages_delta as i64;

        if new_pages < 0 || new_pages > MAX_PAGES as i64 {
            return None;
        }

        if new_pages as usize > self.max_pages() {
            log::info!("memory size out of bounds: {}", new_pages);
            return None;
        }

        let new_size = new_pages as usize * PAGE_SIZE;
        if new_size as u64 > MAX_SIZE {
            return None;
        }

        // Zero initialize the new pages
        self.data.resize(new_size, 0);
        self.page_count = new_pages as usize;

        log::debug!("memory was {} pages", current_pages);
        log::debug!("memory grown by {} pages", pages_delta);
        log::debug!("memory grown to {} pages", self.page_count);

        Some(current_pages.try_into().expect("memory size out of bounds, this should have been caught earlier"))
    }
}

#[allow(unsafe_code)]
/// A trait for types that can be loaded from memory
///
/// # Safety
/// Only implemented for primitive types, unsafe to not allow it for other types.
/// Only actually unsafe to implement if the `unsafe` feature is enabled since there might be
/// UB for loading things things like packed structs
pub(crate) unsafe trait MemLoadable<const T: usize>: Sized + Copy {
    /// Load a value from memory
    fn from_le_bytes(bytes: [u8; T]) -> Self;
    /// Load a value from memory
    fn from_be_bytes(bytes: [u8; T]) -> Self;
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<1> for u8 {
    fn from_le_bytes(bytes: [u8; 1]) -> Self {
        bytes[0]
    }
    fn from_be_bytes(bytes: [u8; 1]) -> Self {
        bytes[0]
    }
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<2> for u16 {
    fn from_le_bytes(bytes: [u8; 2]) -> Self {
        Self::from_le_bytes(bytes)
    }
    fn from_be_bytes(bytes: [u8; 2]) -> Self {
        Self::from_be_bytes(bytes)
    }
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<4> for u32 {
    fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self::from_le_bytes(bytes)
    }
    fn from_be_bytes(bytes: [u8; 4]) -> Self {
        Self::from_be_bytes(bytes)
    }
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<8> for u64 {
    fn from_le_bytes(bytes: [u8; 8]) -> Self {
        Self::from_le_bytes(bytes)
    }
    fn from_be_bytes(bytes: [u8; 8]) -> Self {
        Self::from_be_bytes(bytes)
    }
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<16> for u128 {
    fn from_le_bytes(bytes: [u8; 16]) -> Self {
        Self::from_le_bytes(bytes)
    }
    fn from_be_bytes(bytes: [u8; 16]) -> Self {
        Self::from_be_bytes(bytes)
    }
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<1> for i8 {
    fn from_le_bytes(bytes: [u8; 1]) -> Self {
        bytes[0] as i8
    }
    fn from_be_bytes(bytes: [u8; 1]) -> Self {
        bytes[0] as i8
    }
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<2> for i16 {
    fn from_le_bytes(bytes: [u8; 2]) -> Self {
        Self::from_le_bytes(bytes)
    }
    fn from_be_bytes(bytes: [u8; 2]) -> Self {
        Self::from_be_bytes(bytes)
    }
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<4> for i32 {
    fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self::from_le_bytes(bytes)
    }
    fn from_be_bytes(bytes: [u8; 4]) -> Self {
        Self::from_be_bytes(bytes)
    }
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<8> for i64 {
    fn from_le_bytes(bytes: [u8; 8]) -> Self {
        Self::from_le_bytes(bytes)
    }
    fn from_be_bytes(bytes: [u8; 8]) -> Self {
        Self::from_be_bytes(bytes)
    }
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<16> for i128 {
    fn from_le_bytes(bytes: [u8; 16]) -> Self {
        Self::from_le_bytes(bytes)
    }
    fn from_be_bytes(bytes: [u8; 16]) -> Self {
        Self::from_be_bytes(bytes)
    }
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<4> for f32 {
    fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self::from_le_bytes(bytes)
    }
    fn from_be_bytes(bytes: [u8; 4]) -> Self {
        Self::from_be_bytes(bytes)
    }
}

#[allow(unsafe_code)]
unsafe impl MemLoadable<8> for f64 {
    fn from_le_bytes(bytes: [u8; 8]) -> Self {
        Self::from_le_bytes(bytes)
    }
    fn from_be_bytes(bytes: [u8; 8]) -> Self {
        Self::from_be_bytes(bytes)
    }
}

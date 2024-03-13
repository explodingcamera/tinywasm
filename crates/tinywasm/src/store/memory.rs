use alloc::vec;
use alloc::vec::Vec;
use tinywasm_types::{MemoryType, ModuleInstanceAddr};

use crate::{log, Error, Result};

const PAGE_SIZE: usize = 65536;
const MAX_PAGES: usize = 65536;
const MAX_SIZE: u64 = PAGE_SIZE as u64 * MAX_PAGES as u64;

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

    #[cold]
    fn trap_oob(&self, addr: usize, len: usize) -> Error {
        Error::Trap(crate::Trap::MemoryOutOfBounds { offset: addr, len, max: self.data.len() })
    }

    pub(crate) fn store(&mut self, addr: usize, len: usize, data: &[u8]) -> Result<()> {
        let Some(end) = addr.checked_add(len) else {
            return Err(self.trap_oob(addr, data.len()));
        };

        if end > self.data.len() || end < addr {
            return Err(self.trap_oob(addr, data.len()));
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

    pub(crate) fn load(&self, addr: usize, len: usize) -> Result<&[u8]> {
        let Some(end) = addr.checked_add(len) else {
            return Err(self.trap_oob(addr, len));
        };

        if end > self.data.len() || end < addr {
            return Err(self.trap_oob(addr, len));
        }

        Ok(&self.data[addr..end])
    }

    // this is a workaround since we can't use generic const expressions yet (https://github.com/rust-lang/rust/issues/76560)
    pub(crate) fn load_as<const SIZE: usize, T: MemLoadable<SIZE>>(&self, addr: usize) -> Result<T> {
        let Some(end) = addr.checked_add(SIZE) else {
            return Err(self.trap_oob(addr, SIZE));
        };

        if end > self.data.len() {
            return Err(self.trap_oob(addr, SIZE));
        }

        #[cfg(not(feature = "unsafe"))]
        let val = T::from_le_bytes(self.data[addr..end].try_into().expect("slice size mismatch"));

        #[cfg(feature = "unsafe")]
        // SAFETY: we checked that `end` is in bounds above. All types that implement `Into<RawWasmValue>` are valid
        // to load from unaligned addresses.
        let val = unsafe { core::ptr::read_unaligned(self.data[addr..end].as_ptr() as *const T) };

        Ok(val)
    }

    #[inline]
    pub(crate) fn page_count(&self) -> usize {
        self.page_count
    }

    pub(crate) fn fill(&mut self, addr: usize, len: usize, val: u8) -> Result<()> {
        let end = addr.checked_add(len).ok_or_else(|| self.trap_oob(addr, len))?;
        if end > self.data.len() {
            return Err(self.trap_oob(addr, len));
        }

        self.data[addr..end].fill(val);
        Ok(())
    }

    pub(crate) fn copy_from_slice(&mut self, dst: usize, src: &[u8]) -> Result<()> {
        let end = dst.checked_add(src.len()).ok_or_else(|| self.trap_oob(dst, src.len()))?;
        if end > self.data.len() {
            return Err(self.trap_oob(dst, src.len()));
        }

        self.data[dst..end].copy_from_slice(src);
        Ok(())
    }

    pub(crate) fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Result<()> {
        // Calculate the end of the source slice
        let src_end = src.checked_add(len).ok_or_else(|| self.trap_oob(src, len))?;
        if src_end > self.data.len() {
            return Err(self.trap_oob(src, len));
        }

        // Calculate the end of the destination slice
        let dst_end = dst.checked_add(len).ok_or_else(|| self.trap_oob(dst, len))?;
        if dst_end > self.data.len() {
            return Err(self.trap_oob(dst, len));
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
            return None;
        }

        let new_size = new_pages as usize * PAGE_SIZE;
        if new_size as u64 > MAX_SIZE {
            return None;
        }

        // Zero initialize the new pages
        self.data.resize(new_size, 0);
        self.page_count = new_pages as usize;
        debug_assert!(current_pages <= i32::MAX as usize, "page count should never be greater than i32::MAX");
        Some(current_pages as i32)
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
    #[allow(unused)]
    fn from_le_bytes(bytes: [u8; T]) -> Self;
    /// Load a value from memory
    #[allow(unused)]
    fn from_be_bytes(bytes: [u8; T]) -> Self;
}

macro_rules! impl_mem_loadable_for_primitive {
    ($($type:ty, $size:expr),*) => {
        $(
            #[allow(unused)]
            #[allow(unsafe_code)]
            unsafe impl MemLoadable<$size> for $type {
                #[inline]
                fn from_le_bytes(bytes: [u8; $size]) -> Self {
                    <$type>::from_le_bytes(bytes)
                }

                #[inline]
                fn from_be_bytes(bytes: [u8; $size]) -> Self {
                    <$type>::from_be_bytes(bytes)
                }
            }
        )*
    }
}

impl_mem_loadable_for_primitive!(
    u8, 1, i8, 1, u16, 2, i16, 2, u32, 4, i32, 4, f32, 4, u64, 8, i64, 8, f64, 8, u128, 16, i128, 16
);

#[cfg(test)]
mod memory_instance_tests {
    use super::*;
    use tinywasm_types::MemoryArch;

    fn create_test_memory() -> MemoryInstance {
        let kind = MemoryType { arch: MemoryArch::I32, page_count_initial: 1, page_count_max: Some(2) };
        let owner = ModuleInstanceAddr::default();
        MemoryInstance::new(kind, owner)
    }

    #[test]
    fn test_memory_store_and_load() {
        let mut memory = create_test_memory();
        let data_to_store = [1, 2, 3, 4];
        assert!(memory.store(0, data_to_store.len(), &data_to_store).is_ok());
        let loaded_data = memory.load(0, data_to_store.len()).unwrap();
        assert_eq!(loaded_data, &data_to_store);
    }

    #[test]
    fn test_memory_store_out_of_bounds() {
        let mut memory = create_test_memory();
        let data_to_store = [1, 2, 3, 4];
        assert!(memory.store(memory.data.len(), data_to_store.len(), &data_to_store).is_err());
    }

    #[test]
    fn test_memory_fill() {
        let mut memory = create_test_memory();
        assert!(memory.fill(0, 10, 42).is_ok());
        assert_eq!(&memory.data[0..10], &[42; 10]);
    }

    #[test]
    fn test_memory_fill_out_of_bounds() {
        let mut memory = create_test_memory();
        assert!(memory.fill(memory.data.len(), 10, 42).is_err());
    }

    #[test]
    fn test_memory_copy_within() {
        let mut memory = create_test_memory();
        memory.fill(0, 10, 1).unwrap();
        assert!(memory.copy_within(10, 0, 10).is_ok());
        assert_eq!(&memory.data[10..20], &[1; 10]);
    }

    #[test]
    fn test_memory_copy_within_out_of_bounds() {
        let mut memory = create_test_memory();
        assert!(memory.copy_within(memory.data.len(), 0, 10).is_err());
    }

    #[test]
    fn test_memory_grow() {
        let mut memory = create_test_memory();
        let original_pages = memory.page_count();
        assert_eq!(memory.grow(1), Some(original_pages as i32));
        assert_eq!(memory.page_count(), original_pages + 1);
    }

    #[test]
    fn test_memory_grow_out_of_bounds() {
        let mut memory = create_test_memory();
        assert!(memory.grow(MAX_PAGES as i32 + 1).is_none());
    }

    #[test]
    fn test_memory_grow_max_pages() {
        let mut memory = create_test_memory();
        assert_eq!(memory.grow(1), Some(1));
        assert_eq!(memory.grow(1), None);
    }
}

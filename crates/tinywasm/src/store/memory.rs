use alloc::vec;
use alloc::vec::Vec;
use tinywasm_types::{MemoryArch, MemoryType, ModuleInstanceAddr};

use crate::{Error, Result, cold, interpreter::Value128, log};

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
        assert!(kind.page_count_initial() <= kind.page_count_max());
        log::debug!("initializing memory with {} pages of {} bytes", kind.page_count_initial(), kind.page_size());

        Self {
            kind,
            data: vec![0; kind.initial_size() as usize],
            page_count: kind.page_count_initial() as usize,
            _owner: owner,
        }
    }

    pub(crate) fn is_64bit(&self) -> bool {
        matches!(self.kind.arch(), MemoryArch::I64)
    }

    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }

    fn trap_oob(&self, addr: usize, len: usize) -> Error {
        Error::Trap(crate::Trap::MemoryOutOfBounds { offset: addr, len, max: self.data.len() })
    }

    pub(crate) fn store(&mut self, addr: usize, len: usize, data: &[u8]) -> Result<()> {
        let Some(end) = addr.checked_add(len) else {
            cold();
            return Err(self.trap_oob(addr, data.len()));
        };

        if end > self.data.len() || end < addr {
            cold();
            return Err(self.trap_oob(addr, data.len()));
        }
        self.data[addr..end].copy_from_slice(data);
        Ok(())
    }

    pub(crate) fn load(&self, addr: usize, len: usize) -> Result<&[u8]> {
        let Some(end) = addr.checked_add(len) else {
            cold();
            return Err(self.trap_oob(addr, len));
        };

        if end > self.data.len() || end < addr {
            cold();
            return Err(self.trap_oob(addr, len));
        }

        Ok(&self.data[addr..end])
    }

    pub(crate) fn load_as<const SIZE: usize, T: MemValue<SIZE>>(&self, addr: usize) -> Result<T> {
        let Some(end) = addr.checked_add(SIZE) else {
            return Err(self.trap_oob(addr, SIZE));
        };

        if end > self.data.len() {
            return Err(self.trap_oob(addr, SIZE));
        }

        Ok(T::from_mem_bytes(match self.data[addr..end].try_into() {
            Ok(bytes) => bytes,
            Err(_) => return Err(self.trap_oob(addr, SIZE)),
        }))
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

    pub(crate) fn grow(&mut self, pages_delta: i64) -> Option<i64> {
        if pages_delta < 0 {
            log::debug!("memory.grow failed: negative delta {}", pages_delta);
            return None;
        }

        let current_pages = self.page_count;
        let pages_delta = usize::try_from(pages_delta).ok()?;
        let new_pages = current_pages.checked_add(pages_delta)?;
        let max_pages = self.kind.page_count_max().try_into().unwrap_or(usize::MAX);

        if new_pages > max_pages {
            log::debug!("memory.grow failed: new_pages={}, max_pages={}", new_pages, max_pages);
            return None;
        }

        let new_size = (new_pages as u64).checked_mul(self.kind.page_size())?;
        if new_size > self.kind.max_size() {
            log::debug!("memory.grow failed: new_size={}, max_size={}", new_size, self.kind.max_size());
            return None;
        }

        let new_size = usize::try_from(new_size).ok()?;
        if new_size == self.data.len() {
            return i64::try_from(current_pages).ok();
        }

        self.data.resize(new_size, 0);
        self.page_count = new_pages;
        i64::try_from(current_pages).ok()
    }
}

/// A trait for types that can be converted to and from static byte arrays
pub(crate) trait MemValue<const N: usize>: Copy + Sized {
    /// Store a value in memory
    fn to_mem_bytes(self) -> [u8; N];

    /// Load a value from memory
    fn from_mem_bytes(bytes: [u8; N]) -> Self;
}

macro_rules! impl_mem_traits {
    ($($ty:ty, $size:expr),*) => {
        $(
            impl MemValue<$size> for $ty {
                fn from_mem_bytes(bytes: [u8; $size]) -> Self {
                    <$ty>::from_le_bytes(bytes.into())
                }

                fn to_mem_bytes(self) -> [u8; $size] {
                    self.to_le_bytes().into()
                }
            }
        )*
    }
}

impl_mem_traits!(u8, 1, i8, 1, u16, 2, i16, 2, u32, 4, i32, 4, f32, 4, u64, 8, i64, 8, f64, 8, Value128, 16);

#[cfg(test)]
mod memory_instance_tests {
    use super::*;
    use tinywasm_types::MemoryArch;

    fn create_test_memory() -> MemoryInstance {
        let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), None);
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
        let original_pages = memory.page_count;
        assert_eq!(memory.grow(1), Some(original_pages as i64));
        assert_eq!(memory.page_count, original_pages + 1);
    }

    #[test]
    fn test_memory_grow_out_of_bounds() {
        let mut memory = create_test_memory();
        assert!(memory.grow(memory.kind.max_size() as i64 + 1).is_none());
    }

    #[test]
    fn test_memory_grow_max_pages() {
        let mut memory = create_test_memory();
        assert_eq!(memory.grow(1), Some(1));
        assert_eq!(memory.grow(1), None);
    }

    #[test]
    fn test_memory_grow_negative_delta() {
        let mut memory = create_test_memory();
        let original_pages = memory.page_count;

        assert_eq!(memory.grow(-1), None);
        assert_eq!(memory.page_count, original_pages);
    }

    #[test]
    fn test_memory_custom_page_size_out_of_bounds() {
        let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), Some(1));
        let owner = ModuleInstanceAddr::default();
        let mut memory = MemoryInstance::new(kind, owner);

        let data_to_store = [1, 2];
        assert!(memory.store(0, data_to_store.len(), &data_to_store).is_err());
    }

    #[test]
    fn test_memory_custom_page_size_grow() {
        let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), Some(1));
        let owner = ModuleInstanceAddr::default();
        let mut memory = MemoryInstance::new(kind, owner);

        assert_eq!(memory.grow(1), Some(1));

        let data_to_store = [1, 2];
        assert!(memory.store(0, data_to_store.len(), &data_to_store).is_ok());

        let loaded_data = memory.load(0, data_to_store.len()).unwrap();
        assert_eq!(loaded_data, &data_to_store);
    }
}

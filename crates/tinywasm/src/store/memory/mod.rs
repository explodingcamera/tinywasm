use alloc::boxed::Box;
use alloc::format;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::hint::cold_path;
use core::ops::DerefMut;
use core::{cmp::min, ops::Deref};

use tinywasm_types::MemoryType;

use crate::{Error, Result};

mod instance;

mod paged;
#[path = "vec.rs"]
mod vec_memory;

pub(crate) use instance::MemoryInstance;
pub use {paged::PagedMemory, vec_memory::VecMemory};

/// Backend storage for a linear memory.
pub trait LinearMemory {
    /// Returns the current memory length in bytes.
    fn len(&self) -> usize;

    /// Returns true if the memory is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Grows the memory to `new_len` bytes.
    ///
    /// The runtime only calls this with lengths that are exact multiples of the Wasm page size for
    /// the owning memory.
    fn grow_to(&mut self, new_len: usize) -> Option<()>;

    /// Reads up to `dst.len()` bytes starting at `addr` and returns the number of bytes read.
    ///
    /// Backends may return fewer bytes than requested even when more data is available. This lets
    /// non-contiguous backends stop at a natural boundary such as the end of a chunk.
    fn read(&self, addr: usize, dst: &mut [u8]) -> usize;

    /// Writes up to `src.len()` bytes starting at `addr` and returns the number of bytes written.
    ///
    /// Backends may return fewer bytes than requested even when more space is available. This lets
    /// non-contiguous backends stop at a natural boundary such as the end of a chunk.
    fn write(&mut self, addr: usize, src: &[u8]) -> usize;

    /// Writes all bytes in `src` starting at `addr`, or returns `None` if any byte could not be written.
    fn write_all(&mut self, addr: usize, src: &[u8]) -> Option<()> {
        let end = addr.checked_add(src.len())?;
        if end > self.len() {
            return None;
        }

        let mut offset = 0;
        while offset < src.len() {
            let written = self.write(addr + offset, &src[offset..]);
            if written == 0 {
                return None;
            }
            offset += written;
        }

        Some(())
    }

    /// Fills the range `[addr, addr + len)` with `val`.
    fn fill(&mut self, addr: usize, len: usize, val: u8) -> Option<()> {
        let end = addr.checked_add(len)?;
        if end > self.len() {
            return None;
        }

        let mut offset = 0;
        while offset < len {
            let chunk_len = min(len - offset, 1024);
            let chunk = vec![val; chunk_len];
            self.write_all(addr + offset, &chunk)?;
            offset += chunk_len;
        }

        Some(())
    }

    /// Copies `len` bytes from `src` to `dst` within the same memory.
    fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Option<()> {
        let src_end = src.checked_add(len)?;
        let dst_end = dst.checked_add(len)?;
        if src_end > self.len() || dst_end > self.len() {
            return None;
        }

        if len == 0 || dst == src {
            return Some(());
        }

        // If the source and destination ranges are disjoint, we can copy forward without a temporary buffer.
        if dst < src || dst >= src_end {
            let mut offset = 0;
            while offset < len {
                let chunk_len = min(len - offset, 1024);
                let chunk = vec![0; chunk_len];
                self.read_exact(src + offset, &mut chunk.clone())?;
                self.write_all(dst + offset, &chunk)?;
                offset += chunk_len;
            }
        } else {
            // Otherwise, we need to copy backward to avoid overwriting the source data before it's read.
            let mut offset = len;
            while offset > 0 {
                let chunk_len = min(offset, 1024);
                offset -= chunk_len;
                let chunk = vec![0; chunk_len];
                self.read_exact(src + offset, &mut chunk.clone())?;
                self.write_all(dst + offset, &chunk)?;
            }
        }

        Some(())
    }

    /// Reads exactly `dst.len()` bytes starting at `addr`.
    fn read_exact(&self, addr: usize, dst: &mut [u8]) -> Option<()> {
        let end = addr.checked_add(dst.len())?;
        if end > self.len() {
            return None;
        }

        let mut offset = 0;
        while offset < dst.len() {
            let read = self.read(addr + offset, &mut dst[offset..]);
            if read == 0 {
                return None;
            }
            offset += read;
        }

        Some(())
    }

    /// Reads `len` bytes starting at `addr` into a newly allocated buffer.
    fn read_vec(&self, addr: usize, len: usize) -> Option<Vec<u8>> {
        let end = addr.checked_add(len)?;
        if end > self.len() {
            return None;
        }

        let mut data = vec![0; len];
        self.read_exact(addr, &mut data)?;
        Some(data)
    }

    /// Reads exactly 1 byte at the effective address `base + offset`.
    fn read_8(&self, base: u64, offset: u64) -> core::result::Result<u8, crate::Trap> {
        let addr = checked_effective_addr::<1>(self.len(), base, offset)?;
        let mut bytes = [0; 1];
        self.read_exact(addr, &mut bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 1, self.len())
        })?;
        Ok(bytes[0])
    }

    /// Reads exactly 2 bytes at the effective address `base + offset`.
    fn read_16(&self, base: u64, offset: u64) -> core::result::Result<[u8; 2], crate::Trap> {
        let addr = checked_effective_addr::<2>(self.len(), base, offset)?;
        let mut bytes = [0; 2];
        self.read_exact(addr, &mut bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 2, self.len())
        })?;
        Ok(bytes)
    }

    /// Reads exactly 4 bytes at the effective address `base + offset`.
    fn read_32(&self, base: u64, offset: u64) -> core::result::Result<[u8; 4], crate::Trap> {
        let addr = checked_effective_addr::<4>(self.len(), base, offset)?;
        let mut bytes = [0; 4];
        self.read_exact(addr, &mut bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 4, self.len())
        })?;
        Ok(bytes)
    }

    /// Reads exactly 8 bytes at the effective address `base + offset`.
    fn read_64(&self, base: u64, offset: u64) -> core::result::Result<[u8; 8], crate::Trap> {
        let addr = checked_effective_addr::<8>(self.len(), base, offset)?;
        let mut bytes = [0; 8];
        self.read_exact(addr, &mut bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 8, self.len())
        })?;
        Ok(bytes)
    }

    /// Reads exactly 16 bytes at the effective address `base + offset`.
    fn read_128(&self, base: u64, offset: u64) -> core::result::Result<[u8; 16], crate::Trap> {
        let addr = checked_effective_addr::<16>(self.len(), base, offset)?;
        let mut bytes = [0; 16];
        self.read_exact(addr, &mut bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 16, self.len())
        })?;
        Ok(bytes)
    }

    /// Writes exactly 1 byte at the effective address `base + offset`.
    fn write_8(&mut self, base: u64, offset: u64, byte: u8) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<1>(self.len(), base, offset)?;
        self.write(addr, &[byte]);
        Ok(())
    }

    /// Writes exactly 2 bytes at the effective address `base + offset`.
    fn write_16(&mut self, base: u64, offset: u64, bytes: [u8; 2]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<2>(self.len(), base, offset)?;
        self.write_all(addr, &bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 2, self.len())
        })
    }

    /// Writes exactly 4 bytes at the effective address `base + offset`.
    fn write_32(&mut self, base: u64, offset: u64, bytes: [u8; 4]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<4>(self.len(), base, offset)?;
        self.write_all(addr, &bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 4, self.len())
        })
    }

    /// Writes exactly 8 bytes at the effective address `base + offset`.
    fn write_64(&mut self, base: u64, offset: u64, bytes: [u8; 8]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<8>(self.len(), base, offset)?;
        self.write_all(addr, &bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 8, self.len())
        })
    }

    /// Writes exactly 16 bytes at the effective address `base + offset`.
    fn write_128(&mut self, base: u64, offset: u64, bytes: [u8; 16]) -> core::result::Result<(), crate::Trap> {
        let addr = checked_effective_addr::<16>(self.len(), base, offset)?;
        self.write_all(addr, &bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 16, self.len())
        })
    }
}

type MemoryFactory = dyn Fn(MemoryType) -> Result<Box<dyn LinearMemory>> + Send + Sync;

/// Configures how runtime memory instances are created.
#[derive(Clone, Default)]
pub struct MemoryBackend {
    kind: MemoryBackendKind,
}

#[derive(Clone, Default)]
enum MemoryBackendKind {
    #[default]
    Vec,
    Paged {
        chunk_size: usize,
    },
    Custom(Arc<MemoryFactory>),
}

impl MemoryBackend {
    /// Uses a contiguous [`VecMemory`] for each memory instance.
    ///
    /// This is usually the fastest option for reads and writes, but large grows can be expensive
    /// because they may reallocate and copy the entire buffer.
    pub const fn vec() -> Self {
        Self { kind: MemoryBackendKind::Vec }
    }

    /// Uses sparse chunked storage for each memory instance.
    ///
    /// `chunk_size` is the backend chunk size in bytes. It is independent from the Wasm page size.
    ///
    /// This generally makes growth cheaper than [`Self::vec`], but read and write operations do a
    /// little more work and may be slightly slower.
    pub fn paged(chunk_size: usize) -> Self {
        assert!(chunk_size != 0, "chunk_size must be greater than zero");
        Self { kind: MemoryBackendKind::Paged { chunk_size } }
    }

    /// Uses a custom factory to create memory instances.
    pub fn custom<F, M>(factory: F) -> Self
    where
        F: Fn(MemoryType) -> Result<M> + Send + Sync + 'static,
        M: LinearMemory + 'static,
    {
        Self {
            kind: MemoryBackendKind::Custom(Arc::new(move |ty| {
                let memory = factory(ty)?;
                Ok(Box::new(memory) as Box<dyn LinearMemory>)
            })),
        }
    }

    pub(crate) fn create(&self, ty: MemoryType, initial_len: usize) -> Result<MemoryStorage> {
        let storage = match &self.kind {
            MemoryBackendKind::Vec => Box::new(VecMemory::new(initial_len)) as Box<dyn LinearMemory>,
            MemoryBackendKind::Paged { chunk_size } => {
                Box::new(PagedMemory::new(initial_len, *chunk_size)) as Box<dyn LinearMemory>
            }
            MemoryBackendKind::Custom(factory) => factory(ty)?,
        };

        if storage.len() < initial_len {
            return Err(Error::Other(format!(
                "memory backend returned {} bytes for a memory that requires at least {initial_len}",
                storage.len()
            )));
        }

        Ok(MemoryStorage(storage))
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for MemoryBackend {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self.kind {
            MemoryBackendKind::Vec => f.debug_tuple("MemoryBackend::Vec").finish(),
            MemoryBackendKind::Paged { chunk_size } => {
                f.debug_struct("MemoryBackend::Paged").field("chunk_size", chunk_size).finish()
            }
            MemoryBackendKind::Custom(_) => f.debug_tuple("MemoryBackend::Custom").finish(),
        }
    }
}

pub(crate) struct MemoryStorage(Box<dyn LinearMemory>);

impl Deref for MemoryStorage {
    type Target = dyn LinearMemory;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl DerefMut for MemoryStorage {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.0
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for MemoryStorage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("MemoryStorage").field(&format!("{} bytes", self.len())).finish()
    }
}

/// A trait for types that can be converted to and from static byte arrays
pub(crate) trait MemValue<const N: usize>: Copy + Default {
    /// Store a value in memory
    fn to_mem_bytes(self) -> [u8; N];

    /// Load a value from memory
    fn from_mem_bytes(bytes: [u8; N]) -> Self;
}

macro_rules! impl_mem_traits {
    ($($ty:ty, $size:expr),*) => {
        $(
            impl MemValue<$size> for $ty {
                #[inline(always)]
                fn from_mem_bytes(bytes: [u8; $size]) -> Self {
                    <$ty>::from_le_bytes(bytes)
                }

                #[inline(always)]
                fn to_mem_bytes(self) -> [u8; $size] {
                    self.to_le_bytes()
                }
            }
        )*
    }
}

impl_mem_traits!(u8, 1, i8, 1, u16, 2, i16, 2, u32, 4, i32, 4, f32, 4, u64, 8, i64, 8, f64, 8);

fn memory_oob(offset: usize, len: usize, max: usize) -> crate::Trap {
    crate::Trap::MemoryOutOfBounds { offset, len, max }
}

fn checked_effective_addr<const LEN: usize>(
    max: usize,
    base: u64,
    offset: u64,
) -> core::result::Result<usize, crate::Trap> {
    let Some(max_addr) = max.checked_sub(LEN).map(|max_addr| max_addr as u64) else {
        cold_path();
        return Err(memory_oob(usize::try_from(base).unwrap_or(usize::MAX), LEN, max));
    };

    let addr = base.wrapping_add(offset);
    if addr < base || addr > max_addr {
        cold_path();
        return Err(memory_oob(usize::try_from(addr).unwrap_or(usize::MAX), LEN, max));
    }

    Ok(addr as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinywasm_types::MemoryArch;

    fn create_test_memory(kind: MemoryType, backend: MemoryBackend) -> MemoryInstance {
        MemoryInstance::new(kind, &backend).unwrap()
    }

    fn test_backends() -> [MemoryBackend; 2] {
        [MemoryBackend::vec(), MemoryBackend::paged(4)]
    }

    #[test]
    fn memory_copy_from_slice_and_read_vec_work() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), None);
            let mut memory = create_test_memory(kind, backend);
            let data = [1, 2, 3, 4];
            assert!(memory.inner.write_all(0, &data).is_some());
            assert_eq!(memory.inner.read_vec(0, data.len()).unwrap(), data);
        }
    }

    #[test]
    fn memory_read_returns_partial_count() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(1), Some(4));
            let memory = create_test_memory(kind, backend);
            let mut dst = [9; 8];
            assert_eq!(memory.inner.read(2, &mut dst), 2);
            assert_eq!(&dst[..2], &[0, 0]);
            assert_eq!(&dst[2..], &[9; 6]);
        }
    }

    #[test]
    fn memory_copy_from_slice_out_of_bounds_fails() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), None);
            let mut memory = create_test_memory(kind, backend);
            let data = [1, 2, 3, 4];
            let len = memory.inner.len();
            assert!(memory.inner.write_all(len, &data).is_none());
        }
    }

    #[test]
    fn memory_fill_works() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), None);
            let mut memory = create_test_memory(kind, backend);
            assert!(memory.inner.fill(0, 10, 42).is_some());
            assert_eq!(memory.inner.read_vec(0, 10).unwrap(), vec![42; 10]);
        }
    }

    #[test]
    fn memory_fill_out_of_bounds_fails() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), None);
            let mut memory = create_test_memory(kind, backend);
            let len = memory.inner.len();
            assert!(memory.inner.fill(len, 10, 42).is_none());
        }
    }

    #[test]
    fn memory_copy_within_works() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), None);
            let mut memory = create_test_memory(kind, backend);
            memory.inner.fill(0, 10, 1).unwrap();
            assert!(memory.copy_within(10, 0, 10).is_ok());
            assert_eq!(memory.inner.read_vec(10, 10).unwrap(), vec![1; 10]);
        }
    }

    #[test]
    fn memory_copy_within_out_of_bounds_fails() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), None);
            let mut memory = create_test_memory(kind, backend);
            assert!(memory.copy_within(memory.inner.len(), 0, 10).is_err());
        }
    }

    #[test]
    fn memory_grow_works() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), None);
            let mut memory = create_test_memory(kind, backend);
            let original_pages = memory.page_count;
            assert_eq!(memory.grow(1), Some(original_pages as i64));
            assert_eq!(memory.page_count, original_pages + 1);
        }
    }

    #[test]
    fn memory_grow_out_of_bounds_fails() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), None);
            let mut memory = create_test_memory(kind, backend);
            assert!(memory.grow(memory.kind.max_size() as i64 + 1).is_none());
        }
    }

    #[test]
    fn memory_grow_respects_max_pages() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), None);
            let mut memory = create_test_memory(kind, backend);
            assert_eq!(memory.grow(1), Some(1));
            assert_eq!(memory.grow(1), None);
        }
    }

    #[test]
    fn memory_grow_negative_delta_fails() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), None);
            let mut memory = create_test_memory(kind, backend);
            let original_pages = memory.page_count;
            assert_eq!(memory.grow(-1), None);
            assert_eq!(memory.page_count, original_pages);
        }
    }

    #[test]
    fn memory_custom_page_size_out_of_bounds_fails() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), Some(1));
            let mut memory = create_test_memory(kind, backend);
            let data = [1, 2];
            assert!(memory.inner.write_all(0, &data).is_none());
        }
    }

    #[test]
    fn memory_custom_page_size_grow_works() {
        for backend in test_backends() {
            let kind = MemoryType::new(MemoryArch::I32, 1, Some(2), Some(1));
            let mut memory = create_test_memory(kind, backend);
            assert_eq!(memory.grow(1), Some(1));
            let data = [1, 2];
            assert!(memory.inner.write_all(0, &data).is_some());
            assert_eq!(memory.inner.read_vec(0, data.len()).unwrap(), data);
        }
    }
}

use alloc::{boxed::Box, format, sync::Arc};
use alloc::{vec, vec::Vec};
use core::cmp::min;
use core::hint::cold_path;

use tinywasm_types::MemoryType;

use crate::interpreter::Value128;
use crate::{Error, Result};

mod instance;
mod lazy;

mod paged;
#[path = "vec.rs"]
mod vec_memory;

pub(crate) use instance::MemoryInstance;
pub use {lazy::LazyLinearMemory, paged::PagedMemory, vec_memory::VecMemory};

/// Backend storage for a linear memory
///
/// This is a low-level trait that abstracts over the actual storage mechanism for linear memory.
/// This will probably change in the future to allow more efficient implementations.
/// See [`MemoryBackend`] for a higher-level interface to configuring memory storage.
/// The runtime passes slices of the exact indicated width to the fixed-width `write_*` methods.
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
    fn grow_to(&mut self, new_len: usize) -> core::result::Result<(), crate::Trap>;

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
        let Some(end) = addr.checked_add(src.len()) else {
            cold_path();
            return None;
        };

        if end > self.len() {
            cold_path();
            return None;
        }

        let mut offset = 0;
        while offset < src.len() {
            let written = self.write(addr + offset, &src[offset..]);
            if written == 0 {
                cold_path();
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
                let mut chunk = vec![0; chunk_len];
                self.read_exact(src + offset, &mut chunk)?;
                self.write_all(dst + offset, &chunk)?;
                offset += chunk_len;
            }
        } else {
            // Otherwise, we need to copy backward to avoid overwriting the source data before it's read.
            let mut offset = len;
            while offset > 0 {
                let chunk_len = min(offset, 1024);
                offset -= chunk_len;
                let mut chunk = vec![0; chunk_len];
                self.read_exact(src + offset, &mut chunk)?;
                self.write_all(dst + offset, &chunk)?;
            }
        }

        Some(())
    }

    /// Reads exactly `dst.len()` bytes starting at `addr`.
    fn read_exact(&self, addr: usize, dst: &mut [u8]) -> Option<()> {
        let Some(end) = addr.checked_add(dst.len()) else {
            cold_path();
            return None;
        };

        if end > self.len() {
            cold_path();
            return None;
        }

        let mut offset = 0;
        while offset < dst.len() {
            let read = self.read(addr + offset, &mut dst[offset..]);
            if read == 0 {
                cold_path();
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

    /// Reads exactly 1 byte at `addr`.
    fn read_8(&self, addr: usize) -> core::result::Result<[u8; 1], crate::Trap> {
        let mut bytes = [0; 1];
        self.read_exact(addr, &mut bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 1, self.len())
        })?;
        Ok(bytes)
    }

    /// Reads exactly 2 bytes at `addr`.
    fn read_16(&self, addr: usize) -> core::result::Result<[u8; 2], crate::Trap> {
        let mut bytes = [0; 2];
        self.read_exact(addr, &mut bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 2, self.len())
        })?;
        Ok(bytes)
    }

    /// Reads exactly 4 bytes at `addr`.
    fn read_32(&self, addr: usize) -> core::result::Result<[u8; 4], crate::Trap> {
        let mut bytes = [0; 4];
        self.read_exact(addr, &mut bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 4, self.len())
        })?;
        Ok(bytes)
    }

    /// Reads exactly 8 bytes at `addr`.
    fn read_64(&self, addr: usize) -> core::result::Result<[u8; 8], crate::Trap> {
        let mut bytes = [0; 8];
        self.read_exact(addr, &mut bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 8, self.len())
        })?;
        Ok(bytes)
    }

    /// Reads exactly 16 bytes at `addr`.
    fn read_128(&self, addr: usize) -> core::result::Result<[u8; 16], crate::Trap> {
        let mut bytes = [0; 16];
        self.read_exact(addr, &mut bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 16, self.len())
        })?;
        Ok(bytes)
    }

    /// Writes exactly 1 byte at `addr`.
    fn write_8(&mut self, addr: usize, bytes: &[u8]) -> core::result::Result<(), crate::Trap> {
        self.write_all(addr, bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 1, self.len())
        })
    }

    /// Writes exactly 2 bytes at `addr`.
    fn write_16(&mut self, addr: usize, bytes: &[u8]) -> core::result::Result<(), crate::Trap> {
        self.write_all(addr, bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 2, self.len())
        })
    }

    /// Writes exactly 4 bytes at `addr`.
    fn write_32(&mut self, addr: usize, bytes: &[u8]) -> core::result::Result<(), crate::Trap> {
        self.write_all(addr, bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 4, self.len())
        })
    }

    /// Writes exactly 8 bytes at `addr`.
    fn write_64(&mut self, addr: usize, bytes: &[u8]) -> core::result::Result<(), crate::Trap> {
        self.write_all(addr, bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 8, self.len())
        })
    }

    /// Writes exactly 16 bytes at `addr`.
    fn write_128(&mut self, addr: usize, bytes: &[u8]) -> core::result::Result<(), crate::Trap> {
        self.write_all(addr, bytes).ok_or_else(|| {
            cold_path();
            memory_oob(addr, 16, self.len())
        })
    }
}

type MemoryFactory = dyn Fn(MemoryType) -> Result<Box<dyn LinearMemory>> + Send + Sync;

/// Configures how runtime memory instances are created.
#[derive(Clone, Default)]
pub struct MemoryBackend(MemoryBackendInner);

#[derive(Clone, Default)]
enum MemoryBackendInner {
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
        Self(MemoryBackendInner::Vec)
    }

    /// Uses sparse chunked storage for each memory instance.
    ///
    /// `chunk_size` is the backend chunk size in bytes. It must be a non-zero power
    /// of two and is independent from the Wasm page size.
    ///
    /// This generally makes growth cheaper than [`Self::vec`], but read and write operations do a
    /// little more work and may be slightly slower.
    pub fn paged(chunk_size: usize) -> Self {
        assert!(chunk_size.is_power_of_two(), "chunk_size must be a non-zero power of two");
        Self(MemoryBackendInner::Paged { chunk_size })
    }

    /// Uses a custom factory to create memory instances.
    pub fn custom<F, M>(factory: F) -> Self
    where
        F: Fn(MemoryType) -> Result<M> + Send + Sync + 'static,
        M: LinearMemory + 'static,
    {
        Self(MemoryBackendInner::Custom(Arc::new(move |ty| {
            let memory = factory(ty)?;
            Ok(Box::new(memory) as Box<dyn LinearMemory>)
        })))
    }

    pub(crate) fn create(&self, ty: MemoryType, initial_len: usize) -> Result<MemoryStorage> {
        let storage = match &self.0 {
            MemoryBackendInner::Vec => {
                Box::new(VecMemory::try_new(initial_len).map_err(Error::Trap)?) as Box<dyn LinearMemory>
            }
            MemoryBackendInner::Paged { chunk_size } => {
                Box::new(PagedMemory::try_new(initial_len, *chunk_size).map_err(Error::Trap)?) as Box<dyn LinearMemory>
            }
            MemoryBackendInner::Custom(factory) => factory(ty)?,
        };

        if storage.len() < initial_len {
            return Err(Error::Other(format!(
                "memory backend returned {} bytes for a memory that requires at least {initial_len}",
                storage.len()
            )));
        }

        Ok(storage)
    }

    pub(crate) fn create_lazy(&self, ty: MemoryType, initial_len: usize) -> Result<MemoryStorage> {
        Ok(Box::new(LazyLinearMemory::new_with_initial_len(ty, initial_len, self.clone())))
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for MemoryBackend {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self.0 {
            MemoryBackendInner::Vec => f.debug_tuple("MemoryBackend::Vec").finish(),
            MemoryBackendInner::Paged { chunk_size } => {
                f.debug_struct("MemoryBackend::Paged").field("chunk_size", chunk_size).finish()
            }
            MemoryBackendInner::Custom(_) => f.debug_tuple("MemoryBackend::Custom").finish(),
        }
    }
}

pub(crate) type MemoryStorage = Box<dyn LinearMemory>;

/// A trait for types that can be converted to and from static byte arrays
pub(crate) trait MemValue<const N: usize>: Copy + Default {
    /// Store a value in memory
    fn to_mem_bytes(self) -> [u8; N];

    fn from_mem_bytes(bytes: [u8; N]) -> Self;

    fn load_at(mem: &dyn LinearMemory, addr: usize) -> core::result::Result<Self, crate::Trap>;

    fn store_at(self, mem: &mut dyn LinearMemory, addr: usize) -> core::result::Result<(), crate::Trap>;
}

macro_rules! impl_mem_traits {
    ($($ty:ty, $size:expr, $read:ident, $write:ident),* $(,)?) => {
        $(
            impl MemValue<$size> for $ty {
                #[inline(always)]
                fn to_mem_bytes(self) -> [u8; $size] {
                    self.to_le_bytes()
                }

                #[inline(always)]
                fn from_mem_bytes(bytes: [u8; $size]) -> Self {
                    Self::from_le_bytes(bytes)
                }

                #[inline(always)]
                fn load_at(mem: &dyn LinearMemory, addr: usize) -> core::result::Result<Self, crate::Trap> {
                    match mem.$read(addr) {
                        Ok(bytes) => Ok(Self::from_le_bytes(bytes)),
                        Err(trap) => {
                            cold_path();
                            Err(trap)
                        }
                    }
                }

                #[inline(always)]
                fn store_at(
                    self,
                    mem: &mut dyn LinearMemory,
                    addr: usize,
                ) -> core::result::Result<(), crate::Trap> {
                    mem.$write(addr, &self.to_mem_bytes())
                }
            }
        )*
    };
}

impl_mem_traits!(
    u8, 1, read_8, write_8, i8, 1, read_8, write_8, u16, 2, read_16, write_16, i16, 2, read_16, write_16, u32, 4,
    read_32, write_32, i32, 4, read_32, write_32, f32, 4, read_32, write_32, u64, 8, read_64, write_64, i64, 8,
    read_64, write_64, f64, 8, read_64, write_64
);

impl MemValue<16> for Value128 {
    #[inline(always)]
    fn to_mem_bytes(self) -> [u8; 16] {
        self.0
    }

    #[inline(always)]
    fn from_mem_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    #[inline(always)]
    fn load_at(mem: &dyn LinearMemory, addr: usize) -> core::result::Result<Self, crate::Trap> {
        match mem.read_128(addr) {
            Ok(bytes) => Ok(Self(bytes)),
            Err(trap) => {
                cold_path();
                Err(trap)
            }
        }
    }

    #[inline(always)]
    fn store_at(self, mem: &mut dyn LinearMemory, addr: usize) -> core::result::Result<(), crate::Trap> {
        mem.write_128(addr, &self.0)
    }
}

const fn memory_oob(offset: usize, len: usize, max: usize) -> crate::Trap {
    crate::Trap::MemoryOutOfBounds { offset, len, max }
}

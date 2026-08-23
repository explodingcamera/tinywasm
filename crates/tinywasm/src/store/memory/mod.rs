use crate::interpreter::Value128;

mod instance;
mod vec;

pub(crate) use instance::MemoryInstance;
pub(crate) use vec::VecMemory;

/// Internal storage for a linear memory.
///
/// This is the boundary between the interpreter and the backing representation. Keeping it a
/// concrete type selected at compile time means the executor's load and store paths stay the same
/// whether the memory is `Vec`-backed or, later, mmap-backed.
pub(crate) type MemoryStorage = VecMemory;

/// A trait for types that can be converted to and from static byte arrays.
pub(crate) trait MemValue<const N: usize>: Copy + Default {
    /// Store a value in memory.
    fn to_mem_bytes(self) -> [u8; N];
    fn from_mem_bytes(bytes: [u8; N]) -> Self;
    fn load_at(mem: &MemoryStorage, addr: usize) -> core::result::Result<Self, crate::Trap>;
    fn store_at(self, mem: &mut MemoryStorage, addr: usize) -> core::result::Result<(), crate::Trap>;
}

macro_rules! impl_mem_traits {
    ($($ty:ty, $size:expr),* $(,)?) => {
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
                fn load_at(mem: &MemoryStorage, addr: usize) -> core::result::Result<Self, crate::Trap> {
                    Ok(Self::from_le_bytes(cold_err!(mem.read_fixed::<$size>(addr))?))
                }

                #[inline(always)]
                fn store_at(self, mem: &mut MemoryStorage, addr: usize) -> core::result::Result<(), crate::Trap> {
                    mem.write_fixed::<$size>(addr, &self.to_mem_bytes())
                }
            }
        )*
    };
}

impl_mem_traits!(u8, 1, i8, 1, u16, 2, i16, 2, u32, 4, i32, 4, f32, 4, u64, 8, i64, 8, f64, 8);

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
    fn load_at(mem: &MemoryStorage, addr: usize) -> core::result::Result<Self, crate::Trap> {
        Ok(Self(cold_err!(mem.read_fixed::<16>(addr))?))
    }

    #[inline(always)]
    fn store_at(self, mem: &mut MemoryStorage, addr: usize) -> core::result::Result<(), crate::Trap> {
        mem.write_fixed::<16>(addr, &self.0)
    }
}

const fn memory_oob(offset: usize, len: usize, max: usize) -> crate::Trap {
    crate::Trap::MemoryOutOfBounds { offset, len, max }
}

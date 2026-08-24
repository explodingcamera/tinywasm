use alloc::vec::Vec;
use tinywasm_types::*;

use crate::interpreter::{RuntimeValue, Value32, Value64, Value128};

struct GlobalLane<T> {
    values: Vec<T>,
    types: Vec<GlobalType>,
}

impl<T> Default for GlobalLane<T> {
    fn default() -> Self {
        Self { values: Vec::new(), types: Vec::new() }
    }
}

impl<T: Copy> GlobalLane<T> {
    fn reserve(&mut self, additional: usize) {
        self.values.reserve_exact(additional);
        self.types.reserve_exact(additional);
    }

    fn push(&mut self, ty: GlobalType, value: T) -> usize {
        debug_assert_eq!(self.values.len(), self.types.len());
        let index = self.values.len();
        self.values.push(value);
        self.types.push(ty);
        index
    }

    #[inline(always)]
    fn get(&self, index: usize, addr: GlobalAddr) -> T {
        *self.values.get(index).unwrap_or_else(|| unreachable!("invalid global address: {addr}"))
    }

    #[inline(always)]
    fn set(&mut self, index: usize, addr: GlobalAddr, value: T) {
        *self.values.get_mut(index).unwrap_or_else(|| unreachable!("invalid global address: {addr}")) = value;
    }

    fn ty(&self, index: usize, addr: GlobalAddr) -> GlobalType {
        *self.types.get(index).unwrap_or_else(|| unreachable!("invalid global address: {addr}"))
    }
}

/// Global instances split into their physical value lanes.
///
/// Guest values should normally be accessed through
/// [`InternalValue`](crate::interpreter::InternalValue). Dynamic access is
/// reserved for initialization and host boundaries.
#[derive(Default)]
pub(crate) struct Globals {
    globals_32: GlobalLane<Value32>,
    globals_64: GlobalLane<Value64>,
    globals_128: GlobalLane<Value128>,
}

impl Globals {
    const LANE_SHIFT: u32 = 30;
    const INDEX_MASK: u32 = (1 << Self::LANE_SHIFT) - 1;
    const LANE_32: u32 = 0;
    const LANE_64: u32 = 1 << Self::LANE_SHIFT;
    const LANE_128: u32 = 2 << Self::LANE_SHIFT;

    pub(crate) fn reserve(&mut self, globals: &[Global]) {
        let (mut count_32, mut count_64, mut count_128) = (0, 0, 0);
        for global in globals {
            match global.ty.ty {
                WasmType::I32 | WasmType::F32 | WasmType::Ref(_) => count_32 += 1,
                WasmType::I64 | WasmType::F64 => count_64 += 1,
                WasmType::V128 => count_128 += 1,
            }
        }
        self.globals_32.reserve(count_32);
        self.globals_64.reserve(count_64);
        self.globals_128.reserve(count_128);
    }

    fn addr(lane: u32, index: usize) -> GlobalAddr {
        assert!(index <= Self::INDEX_MASK as usize, "too many globals in one value lane");
        lane | index as u32
    }

    #[inline(always)]
    fn index(addr: GlobalAddr, lane: u32) -> usize {
        debug_assert_eq!(addr & !Self::INDEX_MASK, lane, "global address has the wrong value lane");
        (addr & Self::INDEX_MASK) as usize
    }

    /// Returns a global's logical type and mutability.
    pub(crate) fn ty(&self, addr: GlobalAddr) -> GlobalType {
        match addr & !Self::INDEX_MASK {
            Self::LANE_32 => self.globals_32.ty(Self::index(addr, Self::LANE_32), addr),
            Self::LANE_64 => self.globals_64.ty(Self::index(addr, Self::LANE_64), addr),
            Self::LANE_128 => self.globals_128.ty(Self::index(addr, Self::LANE_128), addr),
            _ => unreachable!("invalid global address: {addr}"),
        }
    }

    /// Adds a global and returns its packed store address.
    pub(crate) fn push(&mut self, ty: GlobalType, value: RuntimeValue) -> GlobalAddr {
        match (ty.ty, value) {
            (WasmType::I32 | WasmType::F32, RuntimeValue::Value32(value)) => {
                Self::addr(Self::LANE_32, self.globals_32.push(ty, value))
            }
            (WasmType::Ref(_), RuntimeValue::ValueRef(value)) => {
                Self::addr(Self::LANE_32, self.globals_32.push(ty, value.raw()))
            }
            (WasmType::I64 | WasmType::F64, RuntimeValue::Value64(value)) => {
                Self::addr(Self::LANE_64, self.globals_64.push(ty, value))
            }
            (WasmType::V128, RuntimeValue::Value128(value)) => {
                Self::addr(Self::LANE_128, self.globals_128.push(ty, value))
            }
            _ => unreachable!("global value does not match its declared type"),
        }
    }

    /// Returns a raw value from the 32-bit lane.
    #[inline(always)]
    pub(crate) fn get_32(&self, addr: GlobalAddr) -> Value32 {
        self.globals_32.get(Self::index(addr, Self::LANE_32), addr)
    }

    /// Returns a raw value from the 64-bit lane.
    #[inline(always)]
    pub(crate) fn get_64(&self, addr: GlobalAddr) -> Value64 {
        self.globals_64.get(Self::index(addr, Self::LANE_64), addr)
    }

    /// Returns a raw value from the 128-bit lane.
    #[inline(always)]
    pub(crate) fn get_128(&self, addr: GlobalAddr) -> Value128 {
        self.globals_128.get(Self::index(addr, Self::LANE_128), addr)
    }

    /// Sets a raw value in the 32-bit lane.
    #[inline(always)]
    pub(crate) fn set_32(&mut self, addr: GlobalAddr, value: Value32) {
        self.globals_32.set(Self::index(addr, Self::LANE_32), addr, value);
    }

    /// Sets a raw value in the 64-bit lane.
    #[inline(always)]
    pub(crate) fn set_64(&mut self, addr: GlobalAddr, value: Value64) {
        self.globals_64.set(Self::index(addr, Self::LANE_64), addr, value);
    }

    /// Sets a raw value in the 128-bit lane.
    #[inline(always)]
    pub(crate) fn set_128(&mut self, addr: GlobalAddr, value: Value128) {
        self.globals_128.set(Self::index(addr, Self::LANE_128), addr, value);
    }

    /// Iterates over globals in the 32-bit lane for root tracing.
    pub(crate) fn globals_32(&self) -> impl Iterator<Item = (&Value32, &GlobalType)> {
        self.globals_32.values.iter().zip(&self.globals_32.types)
    }
}

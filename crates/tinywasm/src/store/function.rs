use alloc::vec::Vec;
use core::ops::Range;
use tinywasm_types::*;

use crate::func::HostFunction;

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct HostFunctionInstance {
    pub(crate) type_addr: TypeAddr,
    pub(crate) func: HostFunction,
}

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct WasmFunctionInstance {
    pub(crate) type_addr: TypeAddr,
    pub(crate) func: Shared<WasmFunction>,
    pub(crate) owner: ModuleInstanceId,
}

#[derive(Default)]
pub(crate) struct Functions {
    wasm: Vec<WasmFunctionInstance>,
    host: Vec<HostFunctionInstance>,
}

impl Functions {
    const HOST_TAG: u32 = 1 << 29;
    const INDEX_MASK: u32 = Self::HOST_TAG - 1;
    const MAX_ADDR: u32 = Self::HOST_TAG | Self::INDEX_MASK;

    #[inline(always)]
    pub(crate) const fn is_host(&self, addr: FuncAddr) -> bool {
        addr & Self::HOST_TAG != 0
    }

    #[inline(always)]
    fn index(addr: FuncAddr) -> usize {
        (addr & Self::INDEX_MASK) as usize
    }

    pub(crate) fn extend_wasm(
        &mut self,
        funcs: impl ExactSizeIterator<Item = (TypeAddr, Shared<WasmFunction>)>,
        owner: ModuleInstanceId,
    ) -> Range<FuncAddr> {
        let start = self.wasm.len();
        let end = start.checked_add(funcs.len()).expect("too many Wasm functions");
        assert!(end <= Self::HOST_TAG as usize, "too many Wasm functions");
        self.wasm.reserve_exact(funcs.len());
        self.wasm.extend(funcs.map(|(type_addr, func)| WasmFunctionInstance { type_addr, func, owner }));
        start as FuncAddr..end as FuncAddr
    }

    pub(crate) fn push_host(&mut self, type_addr: TypeAddr, func: HostFunction) -> FuncAddr {
        let index = self.host.len();
        assert!(index <= Self::INDEX_MASK as usize, "too many host functions");
        self.host.push(HostFunctionInstance { type_addr, func });
        Self::HOST_TAG | index as FuncAddr
    }

    #[inline]
    pub(crate) fn type_addr(&self, addr: FuncAddr) -> TypeAddr {
        if self.is_host(addr) { self.host[Self::index(addr)].type_addr } else { self.wasm[Self::index(addr)].type_addr }
    }

    pub(crate) fn contains(&self, addr: FuncAddr) -> bool {
        if addr > Self::MAX_ADDR {
            return false;
        }
        if self.is_host(addr) { Self::index(addr) < self.host.len() } else { Self::index(addr) < self.wasm.len() }
    }

    #[inline]
    pub(crate) fn wasm(&self, addr: FuncAddr) -> &WasmFunctionInstance {
        debug_assert!(!self.is_host(addr), "host address used for a Wasm function");
        &self.wasm[Self::index(addr)]
    }

    #[inline]
    pub(crate) fn host(&self, addr: FuncAddr) -> &HostFunctionInstance {
        debug_assert!(self.is_host(addr), "Wasm address used for a host function");
        &self.host[Self::index(addr)]
    }
}

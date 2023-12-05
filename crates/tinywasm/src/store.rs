use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::{format, vec::Vec};
use tinywasm_types::{FuncAddr, Function, Instruction, ModuleInstanceAddr, TypeAddr, ValType};

use crate::{runtime::Runtime, Error, ModuleInstance, Result};

// global store id counter
static STORE_ID: AtomicUsize = AtomicUsize::new(0);

/// Global state that can be manipulated by WebAssembly programs
///
/// Data should only be addressable by the module that owns it
///
/// Note that the state doesn't do any garbage collection - so it will grow
/// indefinitely if you keep adding modules to it. When calling temporary
/// functions, you should create a new store and then drop it when you're done (e.g. in a request handler)
///
///  See also: https://webassembly.github.io/spec/core/exec/runtime.html#store
#[derive(Debug)]
pub struct Store {
    id: usize,
    module_instances: Vec<ModuleInstance>,
    module_instance_count: usize,

    pub(crate) data: StoreData,
    pub(crate) runtime: Runtime<true>,
}

impl PartialEq for Store {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Default for Store {
    fn default() -> Self {
        let id = STORE_ID.fetch_add(1, Ordering::Relaxed);

        Self {
            id,
            module_instances: Vec::new(),
            module_instance_count: 0,
            data: StoreData::default(),
            runtime: Runtime::default(),
        }
    }
}

#[derive(Debug)]
pub struct FunctionInstance {
    pub(crate) func: Function,
    pub(crate) _module_instance: ModuleInstanceAddr, // index into store.module_instances
}

impl FunctionInstance {
    pub(crate) fn _module_instance_addr(&self) -> ModuleInstanceAddr {
        self._module_instance
    }

    pub(crate) fn locals(&self) -> &[ValType] {
        &self.func.locals
    }

    pub(crate) fn instructions(&self) -> &[Instruction] {
        &self.func.instructions
    }

    pub(crate) fn ty_addr(&self) -> TypeAddr {
        self.func.ty
    }
}

#[derive(Debug, Default)]
pub struct StoreData {
    pub(crate) funcs: Vec<FunctionInstance>,
    // pub tables: Vec<TableAddr>,
    // pub mems: Vec<MemAddr>,
    // pub globals: Vec<GlobalAddr>,
    // pub elems: Vec<ElmAddr>,
    // pub datas: Vec<DataAddr>,
}

impl Store {
    pub fn id(&self) -> usize {
        self.id
    }

    pub(crate) fn next_module_instance_idx(&self) -> ModuleInstanceAddr {
        self.module_instance_count as ModuleInstanceAddr
    }

    /// Initialize the store with global state from the given module
    pub(crate) fn add_instance(&mut self, instance: ModuleInstance) -> Result<()> {
        self.module_instances.push(instance);
        self.module_instance_count += 1;
        Ok(())
    }

    pub(crate) fn add_funcs(&mut self, funcs: Vec<Function>, idx: ModuleInstanceAddr) -> Vec<FuncAddr> {
        let mut func_addrs = Vec::with_capacity(funcs.len());
        for func in funcs.into_iter() {
            self.data.funcs.push(FunctionInstance {
                func,
                _module_instance: idx,
            });
            func_addrs.push(idx as FuncAddr);
        }
        func_addrs
    }

    pub(crate) fn get_func(&self, addr: usize) -> Result<&FunctionInstance> {
        self.data
            .funcs
            .get(addr)
            .ok_or_else(|| Error::Other(format!("function {} not found", addr)))
    }
}

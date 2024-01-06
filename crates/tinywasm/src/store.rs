use core::{
    cell::RefCell,
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::{format, rc::Rc, vec::Vec};
use tinywasm_types::{
    Addr, Data, Element, ElementKind, FuncAddr, Function, Global, GlobalType, Import, Instruction, MemAddr, MemoryType,
    ModuleInstanceAddr, TableAddr, TableType, TypeAddr, ValType,
};

use crate::{
    runtime::{self, DefaultRuntime},
    Error, Extern, Imports, ModuleInstance, RawWasmValue, Result,
};

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
///  See <https://webassembly.github.io/spec/core/exec/runtime.html#store>
#[derive(Debug)]
pub struct Store {
    id: usize,
    module_instances: Vec<ModuleInstance>,
    module_instance_count: usize,

    pub(crate) data: StoreData,
    pub(crate) runtime: Runtime,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Runtime {
    Default,
}

impl Store {
    /// Create a new store
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new store with the given runtime
    pub(crate) fn runtime(&self) -> runtime::DefaultRuntime {
        match self.runtime {
            Runtime::Default => DefaultRuntime::default(),
        }
    }
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
            runtime: Runtime::Default,
        }
    }
}

#[derive(Debug, Default)]
/// Global state that can be manipulated by WebAssembly programs
///
/// Data should only be addressable by the module that owns it
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#store>
// TODO: Arena allocate these?
pub(crate) struct StoreData {
    pub(crate) funcs: Vec<Rc<FunctionInstance>>,
    pub(crate) tables: Vec<TableInstance>,
    pub(crate) mems: Vec<Rc<MemoryInstance>>,
    pub(crate) globals: Vec<Rc<RefCell<GlobalInstance>>>,
    pub(crate) elems: Vec<ElemInstance>,
    pub(crate) datas: Vec<DataInstance>,
}

impl Store {
    /// Get the store's ID (unique per process)
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

    /// Add functions to the store, returning their addresses in the store
    pub(crate) fn add_funcs(&mut self, funcs: Vec<Function>, idx: ModuleInstanceAddr) -> Vec<FuncAddr> {
        let func_count = self.data.funcs.len();
        let mut func_addrs = Vec::with_capacity(func_count);
        for (i, func) in funcs.into_iter().enumerate() {
            self.data.funcs.push(Rc::new(FunctionInstance { func, owner: idx }));
            func_addrs.push((i + func_count) as FuncAddr);
        }
        func_addrs
    }

    /// Add tables to the store, returning their addresses in the store
    pub(crate) fn add_tables(&mut self, tables: Vec<TableType>, idx: ModuleInstanceAddr) -> Vec<TableAddr> {
        let table_count = self.data.tables.len();
        let mut table_addrs = Vec::with_capacity(table_count);
        for (i, table) in tables.into_iter().enumerate() {
            self.data.tables.push(TableInstance::new(table, idx));
            table_addrs.push((i + table_count) as TableAddr);
        }
        table_addrs
    }

    /// Add memories to the store, returning their addresses in the store
    pub(crate) fn add_mems(&mut self, mems: Vec<MemoryType>, idx: ModuleInstanceAddr) -> Vec<MemAddr> {
        let mem_count = self.data.mems.len();
        let mut mem_addrs = Vec::with_capacity(mem_count);
        for (i, mem) in mems.into_iter().enumerate() {
            self.data.mems.push(Rc::new(MemoryInstance::new(mem, idx)));
            mem_addrs.push((i + mem_count) as MemAddr);
        }
        mem_addrs
    }

    /// Add globals to the store, returning their addresses in the store
    pub(crate) fn add_globals(
        &mut self,
        globals: Vec<Global>,
        wasm_imports: &[Import],
        user_imports: &Imports,
        idx: ModuleInstanceAddr,
    ) -> Result<Vec<Addr>> {
        // TODO: initialize imported globals
        let imported_globals = wasm_imports
            .iter()
            .filter_map(|import| match &import.kind {
                tinywasm_types::ImportKind::Global(_) => Some(import),
                _ => None,
            })
            .map(|import| {
                let Some(global) = user_imports.get(&import.module, &import.name) else {
                    return Err(Error::Other(format!(
                        "global import not found for {}::{}",
                        import.module, import.name
                    )));
                };
                match global {
                    Extern::Global(global) => Ok(global),
                    _ => Err(Error::Other(format!(
                        "expected global import for {}::{}",
                        import.module, import.name
                    ))),
                }
            })
            .collect::<Result<Vec<_>>>()?;

        let global_count = self.data.globals.len();
        let mut global_addrs = Vec::with_capacity(global_count);

        log::debug!("globals: {:?}", globals);
        let globals = globals.into_iter();
        let iterator = imported_globals.into_iter().chain(globals.as_ref());

        for (i, global) in iterator.enumerate() {
            use tinywasm_types::ConstInstruction::*;
            let val = match global.init {
                F32Const(f) => RawWasmValue::from(f),
                F64Const(f) => RawWasmValue::from(f),
                I32Const(i) => RawWasmValue::from(i),
                I64Const(i) => RawWasmValue::from(i),
                GlobalGet(addr) => {
                    let addr = global_addrs[addr as usize];
                    let global = self.data.globals[addr as usize].clone();
                    let val = global.borrow().value;
                    val
                }
                RefNull(_) => RawWasmValue::default(),
                RefFunc(idx) => RawWasmValue::from(idx as i64),
            };

            self.data
                .globals
                .push(Rc::new(RefCell::new(GlobalInstance::new(global.ty, val, idx))));

            global_addrs.push((i + global_count) as Addr);
        }
        log::debug!("global_addrs: {:?}", global_addrs);
        Ok(global_addrs)
    }

    /// Add elements to the store, returning their addresses in the store
    pub(crate) fn add_elems(&mut self, elems: Vec<Element>, idx: ModuleInstanceAddr) -> Vec<Addr> {
        let elem_count = self.data.elems.len();
        let mut elem_addrs = Vec::with_capacity(elem_count);
        for (i, elem) in elems.into_iter().enumerate() {
            self.data.elems.push(ElemInstance::new(elem.kind, idx));
            elem_addrs.push((i + elem_count) as Addr);
        }
        elem_addrs
    }

    /// Add data to the store, returning their addresses in the store
    pub(crate) fn add_datas(&mut self, datas: Vec<Data>, idx: ModuleInstanceAddr) -> Vec<Addr> {
        let data_count = self.data.datas.len();
        let mut data_addrs = Vec::with_capacity(data_count);
        for (i, data) in datas.into_iter().enumerate() {
            self.data.datas.push(DataInstance::new(data.data.to_vec(), idx));
            data_addrs.push((i + data_count) as Addr);
        }
        data_addrs
    }

    /// Get the function at the actual index in the store
    pub(crate) fn get_func(&self, addr: usize) -> Result<&Rc<FunctionInstance>> {
        self.data
            .funcs
            .get(addr)
            .ok_or_else(|| Error::Other(format!("function {} not found", addr)))
    }

    pub(crate) fn get_global_val(&self, addr: usize) -> Result<RawWasmValue> {
        self.data
            .globals
            .get(addr)
            .ok_or_else(|| Error::Other(format!("global {} not found", addr)))
            .map(|global| global.borrow().value)
    }

    pub(crate) fn set_global_val(&mut self, addr: usize, value: RawWasmValue) -> Result<()> {
        self.data
            .globals
            .get(addr)
            .ok_or_else(|| Error::Other(format!("global {} not found", addr)))
            .map(|global| global.borrow_mut().value = value)
    }
}

#[derive(Debug)]
/// A WebAssembly Function Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#function-instances>
pub struct FunctionInstance {
    pub(crate) func: Function,
    pub(crate) owner: ModuleInstanceAddr, // index into store.module_instances
}

impl FunctionInstance {
    pub(crate) fn _module_instance_addr(&self) -> ModuleInstanceAddr {
        self.owner
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

/// A WebAssembly Table Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#table-instances>
#[derive(Debug)]
pub(crate) struct TableInstance {
    pub(crate) kind: TableType,
    pub(crate) elements: Vec<Addr>,
    pub(crate) owner: ModuleInstanceAddr, // index into store.module_instances
}

impl TableInstance {
    pub(crate) fn new(kind: TableType, owner: ModuleInstanceAddr) -> Self {
        Self {
            kind,
            elements: Vec::new(),
            owner,
        }
    }
}

/// A WebAssembly Memory Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#memory-instances>
#[derive(Debug)]
pub(crate) struct MemoryInstance {
    pub(crate) kind: MemoryType,
    pub(crate) data: Vec<u8>,
    pub(crate) owner: ModuleInstanceAddr, // index into store.module_instances
}

impl MemoryInstance {
    pub(crate) fn new(kind: MemoryType, owner: ModuleInstanceAddr) -> Self {
        Self {
            kind,
            data: Vec::new(),
            owner,
        }
    }
}

/// A WebAssembly Global Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#global-instances>
#[derive(Debug)]
pub(crate) struct GlobalInstance {
    pub(crate) ty: GlobalType,
    pub(crate) value: RawWasmValue,
    owner: ModuleInstanceAddr, // index into store.module_instances
}

impl GlobalInstance {
    pub(crate) fn new(ty: GlobalType, value: RawWasmValue, owner: ModuleInstanceAddr) -> Self {
        Self { ty, value, owner }
    }
}

/// A WebAssembly Element Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#element-instances>
#[derive(Debug)]
pub(crate) struct ElemInstance {
    kind: ElementKind,
    owner: ModuleInstanceAddr, // index into store.module_instances
}

impl ElemInstance {
    pub(crate) fn new(kind: ElementKind, owner: ModuleInstanceAddr) -> Self {
        Self { kind, owner }
    }
}

/// A WebAssembly Data Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#data-instances>
#[derive(Debug)]
pub(crate) struct DataInstance {
    pub(crate) data: Vec<u8>,
    owner: ModuleInstanceAddr, // index into store.module_instances
}

impl DataInstance {
    pub(crate) fn new(data: Vec<u8>, owner: ModuleInstanceAddr) -> Self {
        Self { data, owner }
    }
}

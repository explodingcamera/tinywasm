use alloc::{boxed::Box, format, string::ToString, vec::Vec};
use core::fmt::Debug;
use core::sync::atomic::{AtomicUsize, Ordering};
use tinywasm_types::*;

use crate::interpreter::{self, InterpreterRuntime, TinyWasmValue};
use crate::{cold, Error, Function, ModuleInstance, Result, Trap};

mod data;
mod element;
mod function;
mod global;
mod memory;
mod suspend_conditions;
mod table;

pub use suspend_conditions::*;
pub(crate) use {data::*, element::*, function::*, global::*, memory::*, table::*};

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
pub struct Store {
    id: usize,
    module_instances: Vec<ModuleInstance>,

    pub(crate) data: StoreData,
    pub(crate) runtime: Runtime,

    // idk where really to put it, but it should be accessible to host environment (obviously)
    // and (less obviously) to host functions called from store - for calling wasm callbacks and propagating this config to them
    // (or just complying with suspend conditions themselves)
    // alternatively it could be passed to function handles and passend into function context
    pub(crate) suspend_cond: SuspendConditions,
}

impl Debug for Store {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Store")
            .field("id", &self.id)
            .field("module_instances", &self.module_instances)
            .field("data", &"...")
            .field("runtime", &self.runtime)
            .finish()
    }
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

    /// Get a module instance by the internal id
    pub fn get_module_instance(&self, addr: ModuleInstanceAddr) -> Option<&ModuleInstance> {
        self.module_instances.get(addr as usize)
    }

    pub(crate) fn get_module_instance_raw(&self, addr: ModuleInstanceAddr) -> ModuleInstance {
        self.module_instances[addr as usize].clone()
    }

    /// Create a new store with the given runtime
    pub(crate) fn runtime(&self) -> interpreter::InterpreterRuntime {
        match self.runtime {
            Runtime::Default => InterpreterRuntime::default(),
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
            data: StoreData::default(),
            runtime: Runtime::Default,
            suspend_cond: SuspendConditions::default(),
        }
    }
}

#[derive(Default)]
/// Global state that can be manipulated by WebAssembly programs
///
/// Data should only be addressable by the module that owns it
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#store>
pub(crate) struct StoreData {
    pub(crate) funcs: Vec<FunctionInstance>,
    pub(crate) tables: Vec<TableInstance>,
    pub(crate) memories: Vec<MemoryInstance>,
    pub(crate) globals: Vec<GlobalInstance>,
    pub(crate) elements: Vec<ElementInstance>,
    pub(crate) datas: Vec<DataInstance>,
}

impl Store {
    /// Get the store's ID (unique per process)
    pub fn id(&self) -> usize {
        self.id
    }

    pub(crate) fn next_module_instance_idx(&self) -> ModuleInstanceAddr {
        self.module_instances.len() as ModuleInstanceAddr
    }

    pub(crate) fn add_instance(&mut self, instance: ModuleInstance) {
        assert!(instance.id() == self.module_instances.len() as ModuleInstanceAddr);
        self.module_instances.push(instance);
    }

    #[cold]
    fn not_found_error(name: &str) -> Error {
        Error::Other(format!("{name} not found"))
    }

    /// Get the function at the actual index in the store
    #[inline]
    pub(crate) fn get_func(&self, addr: FuncAddr) -> &FunctionInstance {
        &self.data.funcs[addr as usize]
    }

    /// Get the memory at the actual index in the store
    #[inline]
    pub(crate) fn get_mem(&self, addr: MemAddr) -> &MemoryInstance {
        &self.data.memories[addr as usize]
    }

    /// Get the memory at the actual index in the store
    #[inline(always)]
    pub(crate) fn get_mem_mut(&mut self, addr: MemAddr) -> &mut MemoryInstance {
        &mut self.data.memories[addr as usize]
    }

    /// Get the memory at the actual index in the store
    #[inline(always)]
    pub(crate) fn get_mems_mut(
        &mut self,
        addr: MemAddr,
        addr2: MemAddr,
    ) -> Result<(&mut MemoryInstance, &mut MemoryInstance)> {
        match get_pair_mut(&mut self.data.memories, addr as usize, addr2 as usize) {
            Some(mems) => Ok(mems),
            None => {
                cold();
                Err(Self::not_found_error("memory"))
            }
        }
    }

    /// Get the table at the actual index in the store
    #[inline]
    pub(crate) fn get_table(&self, addr: TableAddr) -> &TableInstance {
        &self.data.tables[addr as usize]
    }

    /// Get the table at the actual index in the store
    #[inline]
    pub(crate) fn get_table_mut(&mut self, addr: TableAddr) -> &mut TableInstance {
        &mut self.data.tables[addr as usize]
    }

    /// Get two mutable tables at the actual index in the store
    #[inline]
    pub(crate) fn get_tables_mut(
        &mut self,
        addr: TableAddr,
        addr2: TableAddr,
    ) -> Result<(&mut TableInstance, &mut TableInstance)> {
        match get_pair_mut(&mut self.data.tables, addr as usize, addr2 as usize) {
            Some(tables) => Ok(tables),
            None => {
                cold();
                Err(Self::not_found_error("table"))
            }
        }
    }

    /// Get the data at the actual index in the store
    #[inline]
    pub(crate) fn get_data_mut(&mut self, addr: DataAddr) -> &mut DataInstance {
        &mut self.data.datas[addr as usize]
    }

    /// Get the element at the actual index in the store
    #[inline]
    pub(crate) fn get_elem_mut(&mut self, addr: ElemAddr) -> &mut ElementInstance {
        &mut self.data.elements[addr as usize]
    }

    /// Get the global at the actual index in the store
    #[inline]
    pub(crate) fn get_global(&self, addr: GlobalAddr) -> &GlobalInstance {
        &self.data.globals[addr as usize]
    }

    /// Get the global at the actual index in the store
    #[doc(hidden)]
    pub fn get_global_val(&self, addr: MemAddr) -> TinyWasmValue {
        self.data.globals[addr as usize].value.get()
    }

    /// Set the global at the actual index in the store
    #[doc(hidden)]
    pub fn set_global_val(&mut self, addr: MemAddr, value: TinyWasmValue) {
        self.data.globals[addr as usize].value.set(value);
    }
}

// Linking related functions
impl Store {
    /// Add functions to the store, returning their addresses in the store
    pub(crate) fn init_funcs(&mut self, funcs: Vec<WasmFunction>, idx: ModuleInstanceAddr) -> Result<Vec<FuncAddr>> {
        let func_count = self.data.funcs.len();
        let mut func_addrs = Vec::with_capacity(func_count);
        for (i, func) in funcs.into_iter().enumerate() {
            self.data.funcs.push(FunctionInstance::new_wasm(func, idx));
            func_addrs.push((i + func_count) as FuncAddr);
        }
        Ok(func_addrs)
    }

    /// Add tables to the store, returning their addresses in the store
    pub(crate) fn init_tables(&mut self, tables: Vec<TableType>, idx: ModuleInstanceAddr) -> Result<Vec<TableAddr>> {
        let table_count = self.data.tables.len();
        let mut table_addrs = Vec::with_capacity(table_count);
        for (i, table) in tables.into_iter().enumerate() {
            self.data.tables.push(TableInstance::new(table, idx));
            table_addrs.push((i + table_count) as TableAddr);
        }
        Ok(table_addrs)
    }

    /// Add memories to the store, returning their addresses in the store
    pub(crate) fn init_memories(&mut self, memories: Vec<MemoryType>, idx: ModuleInstanceAddr) -> Result<Vec<MemAddr>> {
        let mem_count = self.data.memories.len();
        let mut mem_addrs = Vec::with_capacity(mem_count);
        for (i, mem) in memories.into_iter().enumerate() {
            if let MemoryArch::I64 = mem.arch() {
                return Err(Error::UnsupportedFeature("64-bit memories".to_string()));
            }
            self.data.memories.push(MemoryInstance::new(mem, idx));
            mem_addrs.push((i + mem_count) as MemAddr);
        }
        Ok(mem_addrs)
    }

    /// Add globals to the store, returning their addresses in the store
    pub(crate) fn init_globals(
        &mut self,
        mut imported_globals: Vec<GlobalAddr>,
        new_globals: Vec<Global>,
        func_addrs: &[FuncAddr],
        idx: ModuleInstanceAddr,
    ) -> Result<Vec<Addr>> {
        let global_count = self.data.globals.len();
        imported_globals.reserve_exact(new_globals.len());
        let mut global_addrs = imported_globals;

        for (i, global) in new_globals.iter().enumerate() {
            self.data.globals.push(GlobalInstance::new(
                global.ty,
                self.eval_const(&global.init, &global_addrs, func_addrs)?,
                idx,
            ));
            global_addrs.push((i + global_count) as Addr);
        }

        Ok(global_addrs)
    }

    fn elem_addr(&self, item: &ElementItem, globals: &[Addr], funcs: &[FuncAddr]) -> Result<Option<u32>> {
        let res = match item {
            ElementItem::Func(addr) | ElementItem::Expr(ConstInstruction::RefFunc(Some(addr))) => {
                Some(funcs.get(*addr as usize).copied().ok_or_else(|| {
                    Error::Other(format!("function {addr} not found. This should have been caught by the validator"))
                })?)
            }
            ElementItem::Expr(ConstInstruction::RefFunc(None)) => None,
            ElementItem::Expr(ConstInstruction::RefExtern(None)) => None,
            ElementItem::Expr(ConstInstruction::GlobalGet(addr)) => {
                let addr = globals.get(*addr as usize).copied().ok_or_else(|| {
                    Error::Other(format!("global {addr} not found. This should have been caught by the validator"))
                })?;
                self.data.globals[addr as usize].value.get().unwrap_ref()
            }
            _ => return Err(Error::UnsupportedFeature(format!("const expression other than ref: {item:?}"))),
        };

        Ok(res)
    }

    /// Add elements to the store, returning their addresses in the store
    /// Should be called after the tables have been added
    pub(crate) fn init_elements(
        &mut self,
        table_addrs: &[TableAddr],
        func_addrs: &[FuncAddr],
        global_addrs: &[Addr],
        elements: &[Element],
        idx: ModuleInstanceAddr,
    ) -> Result<(Box<[Addr]>, Option<Trap>)> {
        let elem_count = self.data.elements.len();
        let mut elem_addrs = Vec::with_capacity(elem_count);
        for (i, element) in elements.iter().enumerate() {
            let init = element
                .items
                .iter()
                .map(|item| Ok(TableElement::from(self.elem_addr(item, global_addrs, func_addrs)?)))
                .collect::<Result<Vec<_>>>()?;

            let items = match element.kind {
                // doesn't need to be initialized, can be initialized lazily using the `table.init` instruction
                ElementKind::Passive => Some(init),

                // this one is not available to the runtime but needs to be initialized to declare references
                ElementKind::Declared => None, // a. Execute the instruction elm.drop i

                // this one is active, so we need to initialize it (essentially a `table.init` instruction)
                ElementKind::Active { offset, table } => {
                    let offset = self.eval_i32_const(offset)?;
                    let table_addr = table_addrs
                        .get(table as usize)
                        .copied()
                        .ok_or_else(|| Error::Other(format!("table {table} not found for element {i}")))?;

                    let Some(table) = self.data.tables.get_mut(table_addr as usize) else {
                        return Err(Error::Other(format!("table {table} not found for element {i}")));
                    };

                    // In wasm 2.0, it's possible to call a function that hasn't been instantiated yet,
                    // when using a partially initialized active element segments.
                    // This isn't mentioned in the spec, but the "unofficial" testsuite has a test for it:
                    // https://github.com/WebAssembly/testsuite/blob/5a1a590603d81f40ef471abba70a90a9ae5f4627/linking.wast#L264-L276
                    // I have NO IDEA why this is allowed, but it is.
                    if let Err(Error::Trap(trap)) = table.init(offset, &init) {
                        return Ok((elem_addrs.into_boxed_slice(), Some(trap)));
                    }

                    // f. Execute the instruction elm.drop i
                    None
                }
            };

            self.data.elements.push(ElementInstance::new(element.kind, idx, items));
            elem_addrs.push((i + elem_count) as Addr);
        }

        // this should be optimized out by the compiler
        Ok((elem_addrs.into_boxed_slice(), None))
    }

    /// Add data to the store, returning their addresses in the store
    pub(crate) fn init_datas(
        &mut self,
        mem_addrs: &[MemAddr],
        datas: Vec<Data>,
        idx: ModuleInstanceAddr,
    ) -> Result<(Box<[Addr]>, Option<Trap>)> {
        let data_count = self.data.datas.len();
        let mut data_addrs = Vec::with_capacity(data_count);
        for (i, data) in datas.into_iter().enumerate() {
            let data_val = match data.kind {
                tinywasm_types::DataKind::Active { mem: mem_addr, offset } => {
                    let Some(mem_addr) = mem_addrs.get(mem_addr as usize) else {
                        return Err(Error::Other(format!("memory {mem_addr} not found for data segment {i}")));
                    };

                    let offset = self.eval_i32_const(offset)?;
                    let Some(mem) = self.data.memories.get_mut(*mem_addr as usize) else {
                        return Err(Error::Other(format!("memory {mem_addr} not found for data segment {i}")));
                    };

                    match mem.store(offset as usize, data.data.len(), &data.data) {
                        Ok(()) => None,
                        Err(Error::Trap(trap)) => return Ok((data_addrs.into_boxed_slice(), Some(trap))),
                        Err(e) => return Err(e),
                    }
                }
                tinywasm_types::DataKind::Passive => Some(data.data.to_vec()),
            };

            self.data.datas.push(DataInstance::new(data_val, idx));
            data_addrs.push((i + data_count) as Addr);
        }

        // this should be optimized out by the compiler
        Ok((data_addrs.into_boxed_slice(), None))
    }

    pub(crate) fn add_global(&mut self, ty: GlobalType, value: TinyWasmValue, idx: ModuleInstanceAddr) -> Result<Addr> {
        self.data.globals.push(GlobalInstance::new(ty, value, idx));
        Ok(self.data.globals.len() as Addr - 1)
    }

    pub(crate) fn add_table(&mut self, table: TableType, idx: ModuleInstanceAddr) -> Result<TableAddr> {
        self.data.tables.push(TableInstance::new(table, idx));
        Ok(self.data.tables.len() as TableAddr - 1)
    }

    pub(crate) fn add_mem(&mut self, mem: MemoryType, idx: ModuleInstanceAddr) -> Result<MemAddr> {
        if let MemoryArch::I64 = mem.arch() {
            return Err(Error::UnsupportedFeature("64-bit memories".to_string()));
        }
        self.data.memories.push(MemoryInstance::new(mem, idx));
        Ok(self.data.memories.len() as MemAddr - 1)
    }

    pub(crate) fn add_func(&mut self, func: Function, idx: ModuleInstanceAddr) -> Result<FuncAddr> {
        self.data.funcs.push(FunctionInstance { func, owner: idx });
        Ok(self.data.funcs.len() as FuncAddr - 1)
    }

    /// Evaluate a constant expression, only supporting i32 globals and i32.const
    pub(crate) fn eval_i32_const(&self, const_instr: tinywasm_types::ConstInstruction) -> Result<i32> {
        use tinywasm_types::ConstInstruction::*;
        let val = match const_instr {
            I32Const(i) => i,
            GlobalGet(addr) => self.data.globals[addr as usize].value.get().unwrap_32() as i32,
            _ => return Err(Error::Other("expected i32".to_string())),
        };
        Ok(val)
    }

    /// Evaluate a constant expression
    pub(crate) fn eval_const(
        &self,
        const_instr: &tinywasm_types::ConstInstruction,
        module_global_addrs: &[Addr],
        module_func_addrs: &[FuncAddr],
    ) -> Result<TinyWasmValue> {
        use tinywasm_types::ConstInstruction::*;
        let val = match const_instr {
            F32Const(f) => (*f).into(),
            F64Const(f) => (*f).into(),
            I32Const(i) => (*i).into(),
            I64Const(i) => (*i).into(),
            GlobalGet(addr) => {
                let addr = module_global_addrs.get(*addr as usize).ok_or_else(|| {
                    Error::Other(format!("global {addr} not found. This should have been caught by the validator"))
                })?;

                let global =
                    self.data.globals.get(*addr as usize).expect("global not found. This should be unreachable");
                global.value.get()
            }
            RefFunc(None) => TinyWasmValue::ValueRef(None),
            RefExtern(None) => TinyWasmValue::ValueRef(None),
            RefFunc(Some(idx)) => {
                TinyWasmValue::ValueRef(Some(*module_func_addrs.get(*idx as usize).ok_or_else(|| {
                    Error::Other(format!("function {idx} not found. This should have been caught by the validator"))
                })?))
            }
            _ => return Err(Error::Other("unsupported const instruction".to_string())),
        };
        Ok(val)
    }
}

// remove this when the `get_many_mut` function is stabilized
fn get_pair_mut<T>(slice: &mut [T], i: usize, j: usize) -> Option<(&mut T, &mut T)> {
    let (first, second) = (core::cmp::min(i, j), core::cmp::max(i, j));
    if i == j || second >= slice.len() {
        return None;
    }
    let (_, tmp) = slice.split_at_mut(first);
    let (x, rest) = tmp.split_at_mut(1);
    let (_, y) = rest.split_at_mut(second - first - 1);
    let pair = if i < j { (&mut x[0], &mut y[0]) } else { (&mut y[0], &mut x[0]) };
    Some(pair)
}

// suspend_conditions-related functions
impl Store {
    /// sets suspend conditions for store
    pub fn set_suspend_conditions(&mut self, val: SuspendConditions) {
        self.suspend_cond = val;
    }
    /// gets suspend conditions of store
    pub fn get_suspend_conditions(&self) -> &SuspendConditions {
        &self.suspend_cond
    }
    /// transforms suspend conditions for store using user-provided function
    pub fn update_suspend_conditions(&mut self, mapper: impl FnOnce(SuspendConditions) -> SuspendConditions) {
        let temp = core::mem::take(&mut self.suspend_cond);
        self.suspend_cond = mapper(temp);
    }
}

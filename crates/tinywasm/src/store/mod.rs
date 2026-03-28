use alloc::rc::Rc;
use alloc::{boxed::Box, format, string::ToString, vec::Vec};
use core::fmt::Debug;
use core::sync::atomic::{AtomicUsize, Ordering};
use tinywasm_types::*;

use crate::instance::ModuleInstanceInner;
use crate::interpreter::TinyWasmValue;
use crate::interpreter::stack::Stack;
use crate::{Engine, Error, Function, ModuleInstance, Result, Trap};

mod data;
mod element;
mod function;
mod global;
mod memory;
mod table;

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
    module_instances: Vec<Rc<ModuleInstanceInner>>,

    pub(crate) engine: Engine,
    pub(crate) state: State,
    pub(crate) stack: Stack,
}

impl Debug for Store {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Store")
            .field("id", &self.id)
            .field("module_instances", &self.module_instances)
            .field("data", &"...")
            .field("engine", &self.engine)
            .finish()
    }
}

impl Store {
    /// Create a new store
    pub fn new(engine: Engine) -> Self {
        let id = STORE_ID.fetch_add(1, Ordering::Relaxed);
        Self { id, module_instances: Vec::new(), state: State::default(), stack: Stack::new(engine.config()), engine }
    }

    /// Get a module instance by the internal id
    pub fn get_module_instance(&self, addr: ModuleInstanceAddr) -> Option<ModuleInstance> {
        Some(ModuleInstance(self.module_instances.get(addr as usize)?.clone()))
    }

    pub(crate) fn get_module_instance_raw(&self, addr: ModuleInstanceAddr) -> &Rc<ModuleInstanceInner> {
        &self.module_instances[addr as usize]
    }
}

impl PartialEq for Store {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new(Engine::default())
    }
}

#[derive(Default)]
/// Global state that can be manipulated by WebAssembly programs
///
/// Data should only be addressable by the module that owns it
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#store>
pub(crate) struct State {
    pub(crate) funcs: Vec<FunctionInstance>,
    pub(crate) tables: Vec<TableInstance>,
    pub(crate) memories: Vec<MemoryInstance>,
    pub(crate) globals: Vec<GlobalInstance>,
    pub(crate) elements: Vec<ElementInstance>,
    pub(crate) data: Vec<DataInstance>,
}

impl State {
    /// Get the function at the actual index in the store
    pub(crate) fn get_func(&self, addr: FuncAddr) -> &FunctionInstance {
        match self.funcs.get(addr as usize) {
            Some(func) => func,
            None => unreachable!("function {addr} not found. This should be unreachable"),
        }
    }

    /// Get a wasm function at the actual index in the store, panicking if it's a host function (which should be guaranteed by the validator)
    pub(crate) fn get_wasm_func(&self, addr: FuncAddr) -> &Rc<WasmFunction> {
        match self.funcs.get(addr as usize) {
            Some(func) => match &func.func {
                Function::Wasm(wasm_func) => wasm_func,
                Function::Host(_) => unreachable!(
                    "expected a wasm function at address {addr}, but found a host function. This should be unreachable"
                ),
            },
            None => unreachable!("function {addr} not found. This should be unreachable"),
        }
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mem(&self, addr: MemAddr) -> &MemoryInstance {
        match self.memories.get(addr as usize) {
            Some(mem) => mem,
            None => unreachable!("memory {addr} not found. This should be unreachable"),
        }
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mem_mut(&mut self, addr: MemAddr) -> &mut MemoryInstance {
        match self.memories.get_mut(addr as usize) {
            Some(mem) => mem,
            None => unreachable!("memory {addr} not found. This should be unreachable"),
        }
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mems_mut(
        &mut self,
        addr: MemAddr,
        addr2: MemAddr,
    ) -> Result<(&mut MemoryInstance, &mut MemoryInstance)> {
        match get_pair_mut(&mut self.memories, addr as usize, addr2 as usize) {
            Some(mems) => Ok(mems),
            None => unreachable!("memory {addr} or {addr2} not found. This should be unreachable"),
        }
    }

    /// Get the table at the actual index in the store
    pub(crate) fn get_table(&self, addr: TableAddr) -> &TableInstance {
        match self.tables.get(addr as usize) {
            Some(table) => table,
            None => unreachable!("table {addr} not found. This should be unreachable"),
        }
    }

    /// Get the table at the actual index in the store
    pub(crate) fn get_table_mut(&mut self, addr: TableAddr) -> &mut TableInstance {
        match self.tables.get_mut(addr as usize) {
            Some(table) => table,
            None => unreachable!("table {addr} not found. This should be unreachable"),
        }
    }

    /// Get two mutable tables at the actual index in the store
    pub(crate) fn get_tables_mut(
        &mut self,
        addr: TableAddr,
        addr2: TableAddr,
    ) -> Result<(&mut TableInstance, &mut TableInstance)> {
        match get_pair_mut(&mut self.tables, addr as usize, addr2 as usize) {
            Some(tables) => Ok(tables),
            None => unreachable!("table {addr} or {addr2} not found. This should be unreachable"),
        }
    }

    /// Get the data at the actual index in the store
    pub(crate) fn get_data_mut(&mut self, addr: DataAddr) -> &mut DataInstance {
        match self.data.get_mut(addr as usize) {
            Some(data) => data,
            None => unreachable!("data {addr} not found. This should be unreachable"),
        }
    }

    /// Get the element at the actual index in the store
    pub(crate) fn get_elem_mut(&mut self, addr: ElemAddr) -> &mut ElementInstance {
        match self.elements.get_mut(addr as usize) {
            Some(elem) => elem,
            None => unreachable!("element {addr} not found. This should be unreachable"),
        }
    }

    /// Get the global at the actual index in the store
    pub(crate) fn get_global(&self, addr: GlobalAddr) -> &GlobalInstance {
        match self.globals.get(addr as usize) {
            Some(global) => global,
            None => unreachable!("global {addr} not found. This should be unreachable"),
        }
    }

    /// Get the global at the actual index in the store
    pub(crate) fn get_global_val(&self, addr: MemAddr) -> TinyWasmValue {
        match self.globals.get(addr as usize) {
            Some(global) => global.value.get(),
            None => unreachable!("global {addr} not found. This should be unreachable"),
        }
    }

    /// Set the global at the actual index in the store
    pub(crate) fn set_global_val(&mut self, addr: MemAddr, value: TinyWasmValue) {
        match self.globals.get_mut(addr as usize) {
            Some(global) => global.value.set(value),
            None => unreachable!("global {addr} not found. This should be unreachable"),
        }
    }
}

impl Store {
    /// Get the store's ID (unique per process)
    pub fn id(&self) -> usize {
        self.id
    }

    pub(crate) fn next_module_instance_idx(&self) -> ModuleInstanceAddr {
        self.module_instances.len() as ModuleInstanceAddr
    }

    pub(crate) fn add_instance(&mut self, instance: Rc<ModuleInstanceInner>) {
        assert!(instance.idx == self.module_instances.len() as ModuleInstanceAddr);
        self.module_instances.push(instance);
    }

    /// Get the global at the actual index in the store
    #[doc(hidden)]
    pub fn get_global_val(&self, addr: MemAddr) -> TinyWasmValue {
        self.state.get_global_val(addr)
    }

    /// Set the global at the actual index in the store
    #[doc(hidden)]
    pub fn set_global_val(&mut self, addr: MemAddr, value: TinyWasmValue) {
        self.state.set_global_val(addr, value);
    }
}

// Linking related functions
impl Store {
    /// Add functions to the store, returning their addresses in the store
    pub(crate) fn init_funcs(&mut self, funcs: &[WasmFunction], idx: ModuleInstanceAddr) -> Result<Vec<FuncAddr>> {
        let func_count = self.state.funcs.len();
        let mut func_addrs = Vec::with_capacity(func_count);
        for (i, func) in funcs.iter().enumerate() {
            self.state.funcs.push(FunctionInstance::new_wasm(func.clone(), idx));
            func_addrs.push((i + func_count) as FuncAddr);
        }
        Ok(func_addrs)
    }

    /// Add tables to the store, returning their addresses in the store
    pub(crate) fn init_tables(&mut self, tables: &[TableType], idx: ModuleInstanceAddr) -> Result<Vec<TableAddr>> {
        let table_count = self.state.tables.len();
        let mut table_addrs = Vec::with_capacity(table_count);
        for (i, table) in tables.iter().enumerate() {
            self.state.tables.push(TableInstance::new(table.clone(), idx));
            table_addrs.push((i + table_count) as TableAddr);
        }
        Ok(table_addrs)
    }

    /// Add memories to the store, returning their addresses in the store
    pub(crate) fn init_memories(&mut self, memories: &[MemoryType], idx: ModuleInstanceAddr) -> Result<Vec<MemAddr>> {
        let mem_count = self.state.memories.len();
        let mut mem_addrs = Vec::with_capacity(mem_count);
        for (i, mem) in memories.iter().enumerate() {
            self.state.memories.push(MemoryInstance::new(*mem, idx));
            mem_addrs.push((i + mem_count) as MemAddr);
        }
        Ok(mem_addrs)
    }

    /// Add globals to the store, returning their addresses in the store
    pub(crate) fn init_globals(
        &mut self,
        mut imported_globals: Vec<GlobalAddr>,
        new_globals: &[Global],
        func_addrs: &[FuncAddr],
        idx: ModuleInstanceAddr,
    ) -> Result<Vec<Addr>> {
        let global_count = self.state.globals.len();
        imported_globals.reserve_exact(new_globals.len());
        let mut global_addrs = imported_globals;

        for (i, global) in new_globals.iter().enumerate() {
            self.state.globals.push(GlobalInstance::new(
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
                self.state.globals[addr as usize].value.get().unwrap_ref()
            }
            ElementItem::Expr(item) => {
                return Err(Error::UnsupportedFeature(format!("const expression other than ref: {item:?}")));
            }
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
        let elem_count = self.state.elements.len();
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
                    let offset = self.eval_size_const(offset)?;
                    let table_addr = table_addrs
                        .get(table as usize)
                        .copied()
                        .ok_or_else(|| Error::Other(format!("table {table} not found for element {i}")))?;

                    let Some(table) = self.state.tables.get_mut(table_addr as usize) else {
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

            self.state.elements.push(ElementInstance::new(element.kind, idx, items));
            elem_addrs.push((i + elem_count) as Addr);
        }

        // this should be optimized out by the compiler
        Ok((elem_addrs.into_boxed_slice(), None))
    }

    /// Add data to the store, returning their addresses in the store
    pub(crate) fn init_data(
        &mut self,
        mem_addrs: &[MemAddr],
        data: &[Data],
        idx: ModuleInstanceAddr,
    ) -> Result<(Box<[Addr]>, Option<Trap>)> {
        let data_count = self.state.data.len();
        let mut data_addrs = Vec::with_capacity(data_count);
        for (i, data) in data.iter().enumerate() {
            let data_val = match data.kind {
                tinywasm_types::DataKind::Active { mem: mem_addr, offset } => {
                    let Some(mem_addr) = mem_addrs.get(mem_addr as usize) else {
                        return Err(Error::Other(format!("memory {mem_addr} not found for data segment {i}")));
                    };

                    let offset = self.eval_size_const(offset)?;
                    let Some(mem) = self.state.memories.get_mut(*mem_addr as usize) else {
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

            self.state.data.push(DataInstance::new(data_val, idx));
            data_addrs.push((i + data_count) as Addr);
        }

        // this should be optimized out by the compiler
        Ok((data_addrs.into_boxed_slice(), None))
    }

    pub(crate) fn add_global(&mut self, ty: GlobalType, value: TinyWasmValue, idx: ModuleInstanceAddr) -> Result<Addr> {
        self.state.globals.push(GlobalInstance::new(ty, value, idx));
        Ok(self.state.globals.len() as Addr - 1)
    }

    pub(crate) fn add_table(&mut self, table: TableType, idx: ModuleInstanceAddr) -> Result<TableAddr> {
        self.state.tables.push(TableInstance::new(table, idx));
        Ok(self.state.tables.len() as TableAddr - 1)
    }

    pub(crate) fn add_mem(&mut self, mem: MemoryType, idx: ModuleInstanceAddr) -> Result<MemAddr> {
        if let MemoryArch::I64 = mem.arch() {
            return Err(Error::UnsupportedFeature("64-bit memories".to_string()));
        }
        self.state.memories.push(MemoryInstance::new(mem, idx));
        Ok(self.state.memories.len() as MemAddr - 1)
    }

    pub(crate) fn add_func(&mut self, func: Function, idx: ModuleInstanceAddr) -> Result<FuncAddr> {
        self.state.funcs.push(FunctionInstance { func, owner: idx });
        Ok(self.state.funcs.len() as FuncAddr - 1)
    }

    /// Evaluate a constant expression that's either a i32 or a i64 as a global or a const instruction
    fn eval_size_const(&self, const_instr: tinywasm_types::ConstInstruction) -> Result<i64> {
        Ok(match const_instr {
            ConstInstruction::I32Const(i) => i64::from(i),
            ConstInstruction::I64Const(i) => i,
            ConstInstruction::GlobalGet(addr) => match self.state.globals[addr as usize].value.get() {
                TinyWasmValue::Value32(i) => i64::from(i),
                TinyWasmValue::Value64(i) => i as i64,
                o => return Err(Error::Other(format!("expected i32 or i64, got {o:?}"))),
            },
            o => return Err(Error::Other(format!("expected i32, got {o:?}"))),
        })
    }

    /// Evaluate a constant expression
    fn eval_const(
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
            V128Const(i) => (*i).into(),
            GlobalGet(addr) => {
                let addr = module_global_addrs.get(*addr as usize).ok_or_else(|| {
                    Error::Other(format!("global {addr} not found. This should have been caught by the validator"))
                })?;

                let global =
                    self.state.globals.get(*addr as usize).expect("global not found. This should be unreachable");
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

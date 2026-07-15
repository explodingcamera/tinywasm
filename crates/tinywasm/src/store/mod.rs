use alloc::sync::Arc;
use alloc::{boxed::Box, format, vec::Vec};
use core::hint::cold_path;
use core::sync::atomic::{AtomicUsize, Ordering};
use tinywasm_types::*;

use crate::interpreter::stack::{CallStack, ValueStack};
use crate::interpreter::{TinyWasmValue, ValueRef};
use crate::{Engine, Error, ModuleInstance, Result, Trap};

mod data;
mod element;
mod function;
mod global;
mod memory;
mod table;

pub use memory::{LazyLinearMemory, LinearMemory, MemoryBackend, PagedMemory, VecMemory};
pub(crate) use memory::{MemValue, MemoryInstance};
pub(crate) use {data::*, element::*, function::*, global::*, table::*};

// global store id counter
static STORE_ID: AtomicUsize = AtomicUsize::new(0);

/// Global state that can be manipulated by WebAssembly programs
///
/// Note that the state doesn't do any garbage collection - so it will grow
/// indefinitely if you keep adding modules to it. When calling temporary
/// functions, you should create a new store and then drop it when you're done (e.g. in a request handler).
///
/// ## Example
/// ```rust
/// use tinywasm::engine::{Config, StackConfig};
/// use tinywasm::{Engine, Store};
///
/// let engine = Engine::new(Config::new().with_call_stack(StackConfig::dynamic(64, 512)));
/// let store = Store::new(engine);
/// # _ = store;
/// ```
///
///  See <https://webassembly.github.io/spec/core/exec/runtime.html#store>
pub struct Store {
    id: usize,
    module_instances: Vec<ModuleInstance>,

    pub(crate) engine: Engine,
    pub(crate) execution_fuel: u32,
    pub(crate) execution_active: bool,
    pub(crate) state: State,
    pub(crate) call_stack: CallStack,
    pub(crate) value_stack: ValueStack,
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for Store {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Store")
            .field("id", &self.id)
            .field("module_instances", &self.module_instances)
            .field("engine", &self.engine)
            .finish()
    }
}

impl Store {
    /// Create a new store
    pub fn new(engine: Engine) -> Self {
        let id = STORE_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            module_instances: Vec::new(),
            state: State::default(),
            call_stack: CallStack::new(engine.config()),
            value_stack: ValueStack::new(engine.config()),
            engine,
            execution_fuel: 0,
            execution_active: false,
        }
    }

    /// Get a module instance by the internal id
    pub fn get_module_instance(&self, addr: ModuleInstanceAddr) -> Option<ModuleInstance> {
        self.module_instances.get(addr as usize).cloned()
    }

    #[inline]
    pub(crate) fn get_module_instance_internal(&self, addr: ModuleInstanceAddr) -> ModuleInstance {
        self.get_module_instance(addr).unwrap_or_else(|| unreachable!("invalid module instance: {addr}"))
    }

    pub(crate) fn enter_execution(&mut self) -> Result<()> {
        if self.execution_active {
            return Err(Trap::Other(
                "cannot call a function while another invocation is active; use FuncContext::call from host functions",
            )
            .into());
        }
        self.execution_active = true;
        Ok(())
    }

    pub(crate) fn exit_execution(&mut self) {
        self.execution_active = false;
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
    fn get<'a, T>(items: &'a [T], addr: Addr, kind: &str) -> &'a T {
        items.get(addr as usize).unwrap_or_else(|| unreachable!("invalid {kind} address: {addr}"))
    }

    fn get_mut<'a, T>(items: &'a mut [T], addr: Addr, kind: &str) -> &'a mut T {
        items.get_mut(addr as usize).unwrap_or_else(|| unreachable!("invalid {kind} address: {addr}"))
    }

    fn get_disjoint_mut<'a, T>(items: &'a mut [T], addr: Addr, addr2: Addr, kind: &str) -> (&'a mut T, &'a mut T) {
        let [item_a, item_b] = items
            .get_disjoint_mut([addr as usize, addr2 as usize])
            .unwrap_or_else(|_| unreachable!("invalid {kind} addresses: {addr}, {addr2}"));
        (item_a, item_b)
    }

    /// Get the function at the actual index in the store
    pub(crate) fn get_func(&self, addr: FuncAddr) -> &FunctionInstance {
        Self::get(&self.funcs, addr, "function")
    }

    /// Get a wasm function at the actual index in the store, panicking if it's a host function (which should be guaranteed by the validator)
    pub(crate) fn get_wasm_func(&self, addr: FuncAddr) -> &WasmFunctionInstance {
        match self.funcs.get(addr as usize) {
            Some(FunctionInstance::Wasm(wasm_func)) => wasm_func,
            _ => unreachable!("invalid wasm function address: {addr}"),
        }
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mem(&self, addr: MemAddr) -> &MemoryInstance {
        Self::get(&self.memories, addr, "memory")
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mem_mut(&mut self, addr: MemAddr) -> &mut MemoryInstance {
        Self::get_mut(&mut self.memories, addr, "memory")
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mems_mut(&mut self, addr: MemAddr, addr2: MemAddr) -> (&mut MemoryInstance, &mut MemoryInstance) {
        Self::get_disjoint_mut(&mut self.memories, addr, addr2, "memory")
    }

    /// Get the table at the actual index in the store
    pub(crate) fn get_table(&self, addr: TableAddr) -> &TableInstance {
        Self::get(&self.tables, addr, "table")
    }

    /// Get the table at the actual index in the store
    pub(crate) fn get_table_mut(&mut self, addr: TableAddr) -> &mut TableInstance {
        Self::get_mut(&mut self.tables, addr, "table")
    }

    /// Get two mutable tables at the actual index in the store
    pub(crate) fn get_tables_mut(
        &mut self,
        addr: TableAddr,
        addr2: TableAddr,
    ) -> (&mut TableInstance, &mut TableInstance) {
        Self::get_disjoint_mut(&mut self.tables, addr, addr2, "table")
    }

    /// Get the data at the actual index in the store
    pub(crate) fn get_data_mut(&mut self, addr: DataAddr) -> &mut DataInstance {
        Self::get_mut(&mut self.data, addr, "data")
    }

    /// Get the element at the actual index in the store
    pub(crate) fn get_elem_mut(&mut self, addr: ElemAddr) -> &mut ElementInstance {
        Self::get_mut(&mut self.elements, addr, "element")
    }

    /// Get the global at the actual index in the store
    pub(crate) fn get_global(&self, addr: GlobalAddr) -> &GlobalInstance {
        Self::get(&self.globals, addr, "global")
    }

    /// Get the global at the actual index in the store
    pub(crate) fn get_global_mut(&mut self, addr: GlobalAddr) -> &mut GlobalInstance {
        Self::get_mut(&mut self.globals, addr, "global")
    }

    /// Get the global at the actual index in the store
    pub(crate) fn get_global_val(&self, addr: GlobalAddr) -> TinyWasmValue {
        self.get_global(addr).value.get()
    }

    /// Set the global at the actual index in the store
    pub(crate) fn set_global_val(&mut self, addr: GlobalAddr, value: TinyWasmValue) {
        self.get_global_mut(addr).value.set(value);
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

    pub(crate) fn add_instance(&mut self, instance: ModuleInstance) {
        debug_assert!(instance.idx() == self.module_instances.len() as ModuleInstanceAddr);
        self.module_instances.push(instance);
    }

    /// Get the global at the actual index in the store
    #[doc(hidden)]
    pub fn get_global_val(&self, addr: GlobalAddr) -> TinyWasmValue {
        self.state.get_global_val(addr)
    }

    /// Set the global at the actual index in the store
    #[doc(hidden)]
    pub fn set_global_val(&mut self, addr: GlobalAddr, value: TinyWasmValue) {
        self.state.set_global_val(addr, value);
    }
}

// Linking related functions
impl Store {
    /// Add functions to the store, returning their addresses in the store
    pub(crate) fn init_funcs(
        &mut self,
        funcs: &[Arc<WasmFunction>],
        idx: ModuleInstanceAddr,
    ) -> impl ExactSizeIterator<Item = FuncAddr> {
        let start = self.state.funcs.len() as FuncAddr;
        self.state.funcs.extend(
            funcs.iter().map(|func| FunctionInstance::Wasm(WasmFunctionInstance { func: func.clone(), owner: idx })),
        );
        start..start + funcs.len() as FuncAddr
    }

    /// Add tables to the store, returning their addresses in the store
    pub(crate) fn init_tables(&mut self, tables: &[TableType]) -> Result<impl ExactSizeIterator<Item = TableAddr>> {
        let start = self.state.tables.len() as TableAddr;
        self.state.tables.reserve_exact(tables.len());
        for &table in tables {
            self.state.tables.push(TableInstance::new(table)?);
        }
        Ok(start..start + tables.len() as TableAddr)
    }

    /// Add memories to the store, returning their addresses in the store
    pub(crate) fn init_memories(
        &mut self,
        memories: &[MemoryType],
        init: impl Fn(MemoryType, &MemoryBackend) -> Result<MemoryInstance>,
    ) -> Result<impl ExactSizeIterator<Item = MemAddr>> {
        let start = self.state.memories.len() as MemAddr;
        for mem in memories {
            self.state.memories.push(match init(*mem, &self.engine.config().memory_backend) {
                Ok(mem) => mem,
                Err(e) => {
                    cold_path();
                    return Err(e);
                }
            });
        }
        Ok(start..start + memories.len() as MemAddr)
    }

    /// Add globals to the store, returning their addresses in the store
    pub(crate) fn init_globals(
        &mut self,
        out: &mut Vec<Addr>,
        new_globals: &[Global],
        func_addrs: &[FuncAddr],
    ) -> Result<()> {
        let start = self.state.globals.len() as Addr;
        out.reserve_exact(new_globals.len());
        self.state.globals.reserve_exact(new_globals.len());

        for (i, global) in new_globals.iter().enumerate() {
            let value = match self.eval_const(&global.init, out, func_addrs) {
                Ok(val) => val,
                Err(e) => {
                    cold_path();
                    return Err(e);
                }
            };

            self.state.globals.push(GlobalInstance::new(global.ty, value));
            out.push(start + i as Addr);
        }

        Ok(())
    }

    fn elem_addr(&self, item: &ElementItem, globals: &[Addr], funcs: &[FuncAddr]) -> Result<Option<u32>> {
        let res = match item {
            ElementItem::Func(addr) => match funcs.get(*addr as usize) {
                Some(func_addr) => Some(*func_addr),
                None => {
                    cold_path();
                    return Err(Error::Other(format!(
                        "function {addr} not found. This should have been caught by the validator"
                    )));
                }
            },
            ElementItem::Expr(expr) => self.eval_ref_const(expr, globals, funcs)?,
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
    ) -> Result<(Box<[Addr]>, Option<Trap>)> {
        let elem_count = self.state.elements.len();
        let mut elem_addrs = Vec::with_capacity(elem_count);
        for (i, element) in elements.iter().enumerate() {
            let init = element
                .items
                .iter()
                .map(|item| Ok(TableElement::from(self.elem_addr(item, global_addrs, func_addrs)?)))
                .collect::<Result<Vec<_>>>()?;

            let items = match &element.kind {
                // doesn't need to be initialized, can be initialized lazily using the `table.init` instruction
                ElementKind::Passive => Some(init),

                // this one is not available to the runtime but needs to be initialized to declare references
                ElementKind::Declared => None, // a. Execute the instruction elm.drop i

                // this one is active, so we need to initialize it (essentially a `table.init` instruction)
                ElementKind::Active { offset, table } => {
                    let offset = self.eval_size_const(offset, global_addrs, func_addrs)?;
                    let table_addr = table_addrs
                        .get(*table as usize)
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
                    let Ok(offset) = usize::try_from(offset) else {
                        return Ok((
                            elem_addrs.into_boxed_slice(),
                            Some(Trap::TableOutOfBounds { offset: usize::MAX, len: init.len(), max: table.size() }),
                        ));
                    };

                    if let Err(trap) = table.init(offset, &init) {
                        return Ok((elem_addrs.into_boxed_slice(), Some(trap)));
                    }

                    // f. Execute the instruction elm.drop i
                    None
                }
            };

            self.state.elements.push(ElementInstance { kind: element.kind.clone(), items });
            elem_addrs.push((i + elem_count) as Addr);
        }

        // this should be optimized out by the compiler
        Ok((elem_addrs.into_boxed_slice(), None))
    }

    /// Add data to the store, returning their addresses in the store
    pub(crate) fn init_data(
        &mut self,
        mem_addrs: &[MemAddr],
        global_addrs: &[Addr],
        func_addrs: &[FuncAddr],
        data: &[Data],
    ) -> Result<(Box<[Addr]>, Option<Trap>)> {
        let data_count = self.state.data.len();
        let mut data_addrs = Vec::with_capacity(data_count);
        for (i, data) in data.iter().enumerate() {
            let data_val = match &data.kind {
                tinywasm_types::DataKind::Active { mem: mem_addr, offset } => {
                    let Some(mem_addr) = mem_addrs.get(*mem_addr as usize) else {
                        return Err(Error::Other(format!("memory {mem_addr} not found for data segment {i}")));
                    };

                    let offset = self.eval_size_const(offset, global_addrs, func_addrs)?;
                    let Some(mem) = self.state.memories.get_mut(*mem_addr as usize) else {
                        return Err(Error::Other(format!("memory {mem_addr} not found for data segment {i}")));
                    };

                    match mem.inner.write_all(offset as usize, &data.data) {
                        Some(()) => None,
                        None => {
                            return Ok((
                                data_addrs.into_boxed_slice(),
                                Some(crate::Trap::MemoryOutOfBounds {
                                    offset: offset as usize,
                                    len: data.data.len(),
                                    max: mem.inner.len(),
                                }),
                            ));
                        }
                    }
                }
                tinywasm_types::DataKind::Passive => Some(data.data.to_vec()),
            };

            self.state.data.push(DataInstance { data: data_val });
            data_addrs.push((i + data_count) as Addr);
        }

        // this should be optimized out by the compiler
        Ok((data_addrs.into_boxed_slice(), None))
    }

    pub(crate) fn add_func(&mut self, func: FunctionInstance) -> FuncAddr {
        self.state.funcs.push(func);
        self.state.funcs.len() as FuncAddr - 1
    }

    /// Evaluate a constant expression that's either a i32 or a i64 as a global or a const instruction
    fn eval_size_const(
        &self,
        const_instrs: &[tinywasm_types::ConstInstruction],
        module_global_addrs: &[Addr],
        module_func_addrs: &[FuncAddr],
    ) -> Result<u64> {
        let value = self.eval_const(const_instrs, module_global_addrs, module_func_addrs)?;
        match value {
            TinyWasmValue::Value32(i) => Ok(u64::from(i)),
            TinyWasmValue::Value64(i) => Ok(i),
            other => Err(Error::Other(format!("expected i32 or i64, got {other:?}"))),
        }
    }

    /// Evaluate a constant expression
    fn eval_const(
        &self,
        const_instrs: &[tinywasm_types::ConstInstruction],
        module_global_addrs: &[Addr],
        module_func_addrs: &[FuncAddr],
    ) -> Result<TinyWasmValue> {
        use tinywasm_types::ConstInstruction::*;

        let resolve_global = |idx: u32| -> Result<TinyWasmValue> {
            let Some(addr) = module_global_addrs.get(idx as usize) else {
                cold_path();
                return Err(Error::Other(format!(
                    "global {idx} not found. This should have been caught by the validator"
                )));
            };

            let Some(global) = self.state.globals.get(*addr as usize) else {
                cold_path();
                return Err(Error::Other(format!("global {addr} not found")));
            };

            Ok(global.value.get())
        };

        let resolve_func = |idx: u32| -> Result<u32> {
            match module_func_addrs.get(idx as usize).copied() {
                Some(func_addr) => Ok(func_addr),
                None => {
                    cold_path();
                    Err(Error::Other(format!(
                        "function {idx} not found. This should have been caught by the validator"
                    )))
                }
            }
        };

        if const_instrs.len() == 1 {
            let val = match &const_instrs[0] {
                F32Const(f) => (*f).into(),
                F64Const(f) => (*f).into(),
                I32Const(i) => (*i).into(),
                I64Const(i) => (*i).into(),
                V128Const(i) => (*i).into(),
                GlobalGet(addr) => resolve_global(*addr)?,
                RefFunc(None) => TinyWasmValue::ValueRef(ValueRef::NULL),
                RefExtern(None) => TinyWasmValue::ValueRef(ValueRef::NULL),
                RefFunc(Some(idx)) => TinyWasmValue::ValueRef(ValueRef::from_addr(Some(resolve_func(*idx)?))),
                _ => {
                    cold_path();
                    return Err(Error::other("unsupported const instruction"));
                }
            };

            return Ok(val);
        }

        let mut stack = Vec::with_capacity(const_instrs.len());
        for instr in const_instrs {
            match instr {
                I32Const(i) => stack.push(TinyWasmValue::Value32(*i as u32)),
                I64Const(i) => stack.push(TinyWasmValue::Value64(*i as u64)),
                F32Const(f) => stack.push(TinyWasmValue::Value32(f.to_bits())),
                F64Const(f) => stack.push(TinyWasmValue::Value64(f.to_bits())),
                V128Const(i) => stack.push(TinyWasmValue::Value128((*i).into())),
                GlobalGet(addr) => stack.push(resolve_global(*addr)?),
                RefFunc(None) | RefExtern(None) => stack.push(TinyWasmValue::ValueRef(ValueRef::NULL)),
                RefFunc(Some(idx)) => {
                    stack.push(TinyWasmValue::ValueRef(ValueRef::from_addr(Some(resolve_func(*idx)?))))
                }
                RefExtern(Some(_)) => {
                    cold_path();
                    return Err(Error::other("ref.extern constants are not supported in init expressions"));
                }
                I32Add | I32Sub | I32Mul => {
                    let rhs = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
                    let lhs = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
                    let (TinyWasmValue::Value32(lhs), TinyWasmValue::Value32(rhs)) = (lhs, rhs) else {
                        cold_path();
                        return Err(Error::other("type mismatch in const i32 op"));
                    };
                    let lhs = lhs as i32;
                    let rhs = rhs as i32;
                    let out = match instr {
                        I32Add => lhs.wrapping_add(rhs),
                        I32Sub => lhs.wrapping_sub(rhs),
                        I32Mul => lhs.wrapping_mul(rhs),
                        _ => unreachable!("invalid const instruction in i32 op"),
                    };
                    stack.push(TinyWasmValue::Value32(out as u32));
                }
                I64Add | I64Sub | I64Mul => {
                    let rhs = stack.pop();
                    let lhs = stack.pop();
                    let (Some(TinyWasmValue::Value64(lhs)), Some(TinyWasmValue::Value64(rhs))) = (lhs, rhs) else {
                        cold_path();
                        return Err(Error::other("type mismatch in const i64 op"));
                    };

                    let lhs = lhs as i64;
                    let rhs = rhs as i64;
                    let out = match instr {
                        I64Add => lhs.wrapping_add(rhs),
                        I64Sub => lhs.wrapping_sub(rhs),
                        I64Mul => lhs.wrapping_mul(rhs),
                        _ => unreachable!("invalid const instruction in i64 op"),
                    };
                    stack.push(TinyWasmValue::Value64(out as u64));
                }
            }
        }

        let Some(value) = stack.pop() else {
            cold_path();
            return Err(Error::other("empty const expression"));
        };

        if !stack.is_empty() {
            cold_path();
            return Err(Error::other("const expression did not reduce to single value"));
        }
        Ok(value)
    }

    fn eval_ref_const(
        &self,
        const_instrs: &[tinywasm_types::ConstInstruction],
        module_global_addrs: &[Addr],
        module_func_addrs: &[FuncAddr],
    ) -> Result<Option<u32>> {
        let value = self.eval_const(const_instrs, module_global_addrs, module_func_addrs)?;
        match value {
            TinyWasmValue::ValueRef(v) => Ok(v.addr()),
            other => Err(Error::Other(format!("expected reference const value, got {other:?}"))),
        }
    }
}

use alloc::rc::Rc;
use alloc::{boxed::Box, format, string::ToString, vec::Vec};
use core::sync::atomic::{AtomicUsize, Ordering};
use tinywasm_types::*;

use crate::instance::ModuleInstanceInner;
use crate::interpreter::stack::{CallStack, ValueStack};
use crate::interpreter::{TinyWasmValue, ValueRef};
use crate::{Engine, Error, ModuleInstance, Result, Trap};

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
    pub(crate) execution_fuel: u32,
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
        }
    }

    /// Get a module instance by the internal id
    pub fn get_module_instance(&self, addr: ModuleInstanceAddr) -> Option<ModuleInstance> {
        Some(ModuleInstance(self.module_instances.get(addr as usize)?.clone()))
    }

    #[inline]
    pub(crate) fn get_module_instance_raw(&self, addr: ModuleInstanceAddr) -> &Rc<ModuleInstanceInner> {
        match self.module_instances.get(addr as usize) {
            Some(instance) => instance,
            None => unreachable!("module instance {addr} not found. This should be unreachable"),
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
            Some(func) => match func {
                FunctionInstance::Wasm(wasm_func) => &wasm_func.func,
                FunctionInstance::Host(_) => unreachable!(
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
    pub(crate) fn get_mems_mut(&mut self, addr: MemAddr, addr2: MemAddr) -> (&mut MemoryInstance, &mut MemoryInstance) {
        match self.memories.get_disjoint_mut([addr as usize, addr2 as usize]) {
            Ok([mem_a, mem_b]) => (mem_a, mem_b),
            Err(_) => unreachable!("memory {addr} or {addr2} not found. This should be unreachable"),
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
    ) -> (&mut TableInstance, &mut TableInstance) {
        match self.tables.get_disjoint_mut([addr as usize, addr2 as usize]) {
            Ok([table_a, table_b]) => (table_a, table_b),
            Err(_) => unreachable!("table {addr} or {addr2} not found. This should be unreachable"),
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
    pub(crate) fn get_global_mut(&mut self, addr: GlobalAddr) -> &mut GlobalInstance {
        match self.globals.get_mut(addr as usize) {
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
    pub(crate) fn init_funcs(&mut self, funcs: &[WasmFunction], idx: ModuleInstanceAddr) -> Vec<FuncAddr> {
        let func_count = self.state.funcs.len();
        let mut func_addrs = Vec::with_capacity(func_count);
        for (i, func) in funcs.iter().enumerate() {
            self.state.funcs.push(FunctionInstance::new_wasm(func.clone(), idx));
            func_addrs.push((i + func_count) as FuncAddr);
        }
        func_addrs
    }

    /// Add tables to the store, returning their addresses in the store
    pub(crate) fn init_tables(&mut self, tables: &[TableType], _idx: ModuleInstanceAddr) -> Vec<TableAddr> {
        let table_count = self.state.tables.len();
        let mut table_addrs = Vec::with_capacity(table_count);
        for (i, table) in tables.iter().enumerate() {
            self.state.tables.push(TableInstance::new(table.clone()));
            table_addrs.push((i + table_count) as TableAddr);
        }
        table_addrs
    }

    /// Add memories to the store, returning their addresses in the store
    pub(crate) fn init_memories(&mut self, memories: &[MemoryType], _idx: ModuleInstanceAddr) -> Vec<MemAddr> {
        let mem_count = self.state.memories.len();
        let mut mem_addrs = Vec::with_capacity(mem_count);
        for (i, mem) in memories.iter().enumerate() {
            self.state.memories.push(MemoryInstance::new(*mem));
            mem_addrs.push((i + mem_count) as MemAddr);
        }
        mem_addrs
    }

    /// Add globals to the store, returning their addresses in the store
    pub(crate) fn init_globals(
        &mut self,
        mut imported_globals: Vec<GlobalAddr>,
        new_globals: &[Global],
        func_addrs: &[FuncAddr],
        _idx: ModuleInstanceAddr,
    ) -> Result<Vec<Addr>> {
        let global_count = self.state.globals.len();
        imported_globals.reserve_exact(new_globals.len());
        let mut global_addrs = imported_globals;

        for (i, global) in new_globals.iter().enumerate() {
            let value = self.eval_const(&global.init, &global_addrs, func_addrs)?;
            self.state.globals.push(GlobalInstance::new(global.ty, value));
            global_addrs.push((i + global_count) as Addr);
        }

        Ok(global_addrs)
    }

    fn elem_addr(&self, item: &ElementItem, globals: &[Addr], funcs: &[FuncAddr]) -> Result<Option<u32>> {
        let res = match item {
            ElementItem::Func(addr) => Some(funcs.get(*addr as usize).copied().ok_or_else(|| {
                Error::Other(format!("function {addr} not found. This should have been caught by the validator"))
            })?),
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
        _idx: ModuleInstanceAddr,
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
                    if let Err(Error::Trap(trap)) = table.init(offset, &init) {
                        return Ok((elem_addrs.into_boxed_slice(), Some(trap)));
                    }

                    // f. Execute the instruction elm.drop i
                    None
                }
            };

            self.state.elements.push(ElementInstance::new(element.kind.clone(), items));
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
        _idx: ModuleInstanceAddr,
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

                    match mem.store(offset as usize, &data.data) {
                        Ok(()) => None,
                        Err(Error::Trap(trap)) => return Ok((data_addrs.into_boxed_slice(), Some(trap))),
                        Err(e) => return Err(e),
                    }
                }
                tinywasm_types::DataKind::Passive => Some(data.data.to_vec()),
            };

            self.state.data.push(DataInstance::new(data_val));
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
    ) -> Result<i64> {
        let value = self.eval_const(const_instrs, module_global_addrs, module_func_addrs)?;
        match value {
            TinyWasmValue::Value32(i) => Ok(i64::from(i)),
            TinyWasmValue::Value64(i) => Ok(i as i64),
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
            let addr = module_global_addrs.get(idx as usize).ok_or_else(|| {
                Error::Other(format!("global {idx} not found. This should have been caught by the validator"))
            })?;
            let global = self
                .state
                .globals
                .get(*addr as usize)
                .ok_or_else(|| Error::Other(format!("global {addr} not found")))?;
            Ok(global.value.get())
        };

        let resolve_func = |idx: u32| -> Result<u32> {
            module_func_addrs.get(idx as usize).copied().ok_or_else(|| {
                Error::Other(format!("function {idx} not found. This should have been caught by the validator"))
            })
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
                _ => return Err(Error::Other("unsupported const instruction".to_string())),
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
                    return Err(Error::Other("ref.extern constants are not supported in init expressions".to_string()));
                }
                I32Add | I32Sub | I32Mul => {
                    let rhs = stack.pop().ok_or_else(|| Error::Other("const stack underflow".to_string()))?;
                    let lhs = stack.pop().ok_or_else(|| Error::Other("const stack underflow".to_string()))?;
                    let (TinyWasmValue::Value32(lhs), TinyWasmValue::Value32(rhs)) = (lhs, rhs) else {
                        return Err(Error::Other("type mismatch in const i32 op".to_string()));
                    };
                    let lhs = lhs as i32;
                    let rhs = rhs as i32;
                    let out = match instr {
                        I32Add => lhs.wrapping_add(rhs),
                        I32Sub => lhs.wrapping_sub(rhs),
                        I32Mul => lhs.wrapping_mul(rhs),
                        _ => unreachable!(),
                    };
                    stack.push(TinyWasmValue::Value32(out as u32));
                }
                I64Add | I64Sub | I64Mul => {
                    let rhs = stack.pop().ok_or_else(|| Error::Other("const stack underflow".to_string()))?;
                    let lhs = stack.pop().ok_or_else(|| Error::Other("const stack underflow".to_string()))?;
                    let (TinyWasmValue::Value64(lhs), TinyWasmValue::Value64(rhs)) = (lhs, rhs) else {
                        return Err(Error::Other("type mismatch in const i64 op".to_string()));
                    };
                    let lhs = lhs as i64;
                    let rhs = rhs as i64;
                    let out = match instr {
                        I64Add => lhs.wrapping_add(rhs),
                        I64Sub => lhs.wrapping_sub(rhs),
                        I64Mul => lhs.wrapping_mul(rhs),
                        _ => unreachable!(),
                    };
                    stack.push(TinyWasmValue::Value64(out as u64));
                }
            }
        }

        let value = stack.pop().ok_or_else(|| Error::Other("empty const expression".to_string()))?;
        if !stack.is_empty() {
            return Err(Error::Other("const expression did not reduce to single value".to_string()));
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

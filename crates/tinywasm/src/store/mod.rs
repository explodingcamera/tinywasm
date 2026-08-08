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
mod gc;
mod global;
mod memory;
mod state;
mod table;
mod types;

pub(crate) use gc::{decode_data, default_value, pop_value, push_value};
pub use memory::{LazyLinearMemory, LinearMemory, MemoryBackend, PagedMemory, VecMemory};
pub(crate) use memory::{MemValue, MemoryInstance};
pub(crate) use state::State;
pub(crate) use types::{canonicalize_ref_type, canonicalize_value_type};
pub(crate) use {data::*, element::*, function::*, global::*, table::*};

// global store id counter
static STORE_ID: AtomicUsize = AtomicUsize::new(0);

/// Global state that can be manipulated by WebAssembly programs
///
/// Managed WebAssembly GC objects are collected automatically. Other Store
/// instances, such as modules, functions, memories, and tables, live until the
/// Store is dropped. GC references exposed through the copyable host value API
/// are retained for the Store's lifetime.
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
    pub(crate) module_instances: Vec<ModuleInstance>,

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
        let state =
            State { gc: Box::new(gc::GcHeap::new(engine.config().gc_collection_threshold)), ..State::default() };
        Self {
            id,
            module_instances: Vec::new(),
            state,
            call_stack: CallStack::new(engine.config()),
            value_stack: ValueStack::new(engine.config()),
            engine,
            execution_fuel: 0,
            execution_active: false,
        }
    }

    /// Get a module instance by the internal id
    pub fn get_module_instance(&self, id: ModuleInstanceId) -> Option<ModuleInstance> {
        self.module_instances.get(id as usize).cloned()
    }

    #[inline]
    pub(crate) fn get_module_instance_internal(&self, id: ModuleInstanceId) -> ModuleInstance {
        self.module_instances.get(id as usize).unwrap_or_else(|| unreachable!("invalid module instance: {id}")).clone()
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

impl Store {
    /// Get the store's ID (unique per process)
    pub fn id(&self) -> usize {
        self.id
    }

    pub(crate) fn next_module_instance_id(&self) -> ModuleInstanceId {
        self.module_instances.len() as ModuleInstanceId
    }

    pub(crate) fn add_instance(&mut self, instance: ModuleInstance) {
        debug_assert!(instance.id() == self.module_instances.len() as ModuleInstanceId);
        self.module_instances.push(instance);
    }

    /// Gets a global by its opaque store address.
    #[doc(hidden)]
    pub fn get_global_val(&self, addr: GlobalAddr) -> TinyWasmValue {
        self.state.globals.get(addr)
    }

    /// Sets a global by its opaque store address.
    #[doc(hidden)]
    pub fn set_global_val(&mut self, addr: GlobalAddr, value: TinyWasmValue) {
        self.state.globals.set(addr, value);
    }

    /// Returns whether a public value has the requested runtime type.
    #[doc(hidden)]
    pub fn value_matches_type(&self, value: WasmValue, ty: WasmType) -> bool {
        self.state.value_matches_type(value, ty)
    }
}

impl Store {
    /// Add functions to the store, returning their addresses in the store
    pub(crate) fn init_funcs(
        &mut self,
        funcs: &[alloc::sync::Arc<WasmFunction>],
        owner: ModuleInstanceId,
        type_addrs: &[TypeAddr],
    ) -> impl ExactSizeIterator<Item = FuncAddr> {
        let start = self.state.funcs.len() as FuncAddr;
        debug_assert_eq!(funcs.len(), type_addrs.len());
        for (func, &type_addr) in funcs.iter().zip(type_addrs) {
            self.state.funcs.push(FunctionInstance {
                type_addr,
                gc: self.state.func_gc_metadata(type_addr),
                kind: FunctionKind::Wasm(WasmFunctionInstance { func: func.clone(), owner }),
            });
        }
        start..start + funcs.len() as FuncAddr
    }

    /// Add tables to the store, returning their addresses in the store
    pub(crate) fn init_tables(
        &mut self,
        tables: &[TableDefinition],
        global_addrs: &[GlobalAddr],
        func_addrs: &[FuncAddr],
        type_addrs: &[TypeAddr],
    ) -> Result<impl ExactSizeIterator<Item = TableAddr>> {
        let start = self.state.tables.len() as TableAddr;
        self.state.tables.reserve_exact(tables.len());
        for table in tables {
            let init = match &table.init {
                Some(expr) => match self.eval_const(expr, global_addrs, func_addrs, type_addrs)? {
                    TinyWasmValue::ValueRef(value) => value,
                    _ => return Err(Error::other("table initializer is not a reference value")),
                },
                None => ValueRef::NULL,
            };
            let element_type = canonicalize_ref_type(table.ty.element_type, type_addrs);
            let ty = match table.ty.arch() {
                MemoryArch::I32 => TableType::new(element_type, table.ty.size_initial, table.ty.size_max),
                MemoryArch::I64 => TableType::new64(element_type, table.ty.size_initial, table.ty.size_max),
            };
            self.state.tables.push(TableInstance::new_with_init(ty, init)?);
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
        globals: &[Global],
        func_addrs: &[FuncAddr],
        type_addrs: &[TypeAddr],
    ) -> Result<()> {
        for global in globals {
            let value = match self.eval_const(&global.init, out, func_addrs, type_addrs) {
                Ok(val) => val,
                Err(e) => {
                    cold_path();
                    return Err(e);
                }
            };
            let ty = global.ty.with_ty(canonicalize_value_type(global.ty.ty, type_addrs));
            out.push(self.state.globals.push(ty, value));
        }

        Ok(())
    }

    fn elem_value(
        &mut self,
        item: &ElementItem,
        globals: &[Addr],
        funcs: &[FuncAddr],
        type_addrs: &[TypeAddr],
    ) -> Result<ValueRef> {
        match item {
            ElementItem::Expr(expr) => match self.eval_const(expr, globals, funcs, type_addrs)? {
                TinyWasmValue::ValueRef(value) => Ok(value),
                other => {
                    cold_path();
                    Err(Error::Other(format!("expected ref type, got {other:?}")))
                }
            },
            ElementItem::Func(addr) => match funcs.get(*addr as usize) {
                Some(func_addr) => Ok(ValueRef::from_category_addr(*func_addr)),
                None => {
                    cold_path();
                    Err(Error::Other(format!(
                        "function {addr} not found. This should have been caught by the validator"
                    )))
                }
            },
        }
    }

    /// Add elements to the store, returning their addresses in the store
    /// Should be called after the tables have been added
    pub(crate) fn init_elements(
        &mut self,
        table_addrs: &[TableAddr],
        func_addrs: &[FuncAddr],
        global_addrs: &[Addr],
        elements: &[Element],
        type_addrs: &[TypeAddr],
    ) -> Result<(Box<[Addr]>, Option<Trap>)> {
        let elem_count = self.state.elements.len();
        let mut elem_addrs = Vec::with_capacity(elements.len());
        for (i, element) in elements.iter().enumerate() {
            let elem_addr = self.state.elements.len();
            self.state.elements.push(ElementInstance {
                items: Some(Vec::with_capacity(element.items.len())),
                ty: canonicalize_ref_type(element.ty, type_addrs),
            });
            for item in &element.items {
                let value = self.elem_value(item, global_addrs, func_addrs, type_addrs)?;
                self.state.elements[elem_addr].items.as_mut().unwrap().push(value);
            }

            match &element.kind {
                // doesn't need to be initialized, can be initialized lazily using the `table.init` instruction
                ElementKind::Passive => {}

                // this one is not available to the runtime but needs to be initialized to declare references
                ElementKind::Declared => self.state.elements[elem_addr].drop(),

                // this one is active, so we need to initialize it (essentially a `table.init` instruction)
                ElementKind::Active { offset, table } => {
                    let offset = self.eval_size_const(offset, global_addrs, func_addrs, type_addrs)?;
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
                            Some(Trap::TableOutOfBounds {
                                offset: usize::MAX,
                                len: self.state.elements[elem_addr].items.as_ref().unwrap().len(),
                                max: table.size(),
                            }),
                        ));
                    };

                    let State { elements, tables, .. } = &mut self.state;
                    let init = elements[elem_addr].items.as_deref().unwrap();
                    let table = &mut tables[table_addr as usize];
                    if let Err(trap) = table.init(offset, init) {
                        return Ok((elem_addrs.into_boxed_slice(), Some(trap)));
                    }

                    // f. Execute the instruction elm.drop i
                    elements[elem_addr].drop();
                }
            }

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
        type_addrs: &[TypeAddr],
    ) -> Result<(Box<[Addr]>, Option<Trap>)> {
        let data_count = self.state.data.len();
        let mut data_addrs = Vec::with_capacity(data_count);
        for (i, data) in data.iter().enumerate() {
            let data_val = match &data.kind {
                tinywasm_types::DataKind::Active { mem: mem_addr, offset } => {
                    let Some(mem_addr) = mem_addrs.get(*mem_addr as usize) else {
                        return Err(Error::Other(format!("memory {mem_addr} not found for data segment {i}")));
                    };

                    let offset = self.eval_size_const(offset, global_addrs, func_addrs, type_addrs)?;
                    let Some(mem) = self.state.memories.get_mut(*mem_addr as usize) else {
                        return Err(Error::Other(format!("memory {mem_addr} not found for data segment {i}")));
                    };

                    let offset = usize::try_from(offset).unwrap_or(usize::MAX);
                    match mem.inner.write_all(offset, &data.data) {
                        Some(()) => None,
                        None => {
                            return Ok((
                                data_addrs.into_boxed_slice(),
                                Some(crate::Trap::MemoryOutOfBounds {
                                    offset,
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
        &mut self,
        const_instrs: &[tinywasm_types::ConstInstruction],
        module_global_addrs: &[Addr],
        module_func_addrs: &[FuncAddr],
        module_type_addrs: &[TypeAddr],
    ) -> Result<u64> {
        let value = self.eval_const(const_instrs, module_global_addrs, module_func_addrs, module_type_addrs)?;
        match value {
            TinyWasmValue::Value32(i) => Ok(u64::from(i)),
            TinyWasmValue::Value64(i) => Ok(i),
            other => Err(Error::Other(format!("expected i32 or i64, got {other:?}"))),
        }
    }

    /// Evaluate a constant expression
    #[inline]
    fn eval_const(
        &mut self,
        const_instrs: &[tinywasm_types::ConstInstruction],
        module_global_addrs: &[Addr],
        module_func_addrs: &[FuncAddr],
        module_type_addrs: &[TypeAddr],
    ) -> Result<TinyWasmValue> {
        use tinywasm_types::ConstInstruction::*;

        let global_value = |state: &State, index: u32| -> Result<TinyWasmValue> {
            let addr = *module_global_addrs
                .get(index as usize)
                .ok_or_else(|| Error::Other(format!("global {index} not found")))?;
            Ok(state.globals.get(addr))
        };
        let func_ref = |index: u32| -> Result<ValueRef> {
            let addr = *module_func_addrs
                .get(index as usize)
                .ok_or_else(|| Error::Other(format!("function {index} not found")))?;
            Ok(ValueRef::from_category_addr(addr))
        };
        let type_addr = |index: TypeAddr| -> Result<TypeAddr> {
            module_type_addrs.get(index as usize).copied().ok_or_else(|| Error::other("GC constant type not found"))
        };
        let pop_value = |stack: &mut Vec<TinyWasmValue>, storage: StorageType| -> Result<TinyWasmValue> {
            let value = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
            match (storage, value) {
                (StorageType::I8, TinyWasmValue::Value32(value)) => Ok(TinyWasmValue::Value32(value as u8 as u32)),
                (StorageType::I16, TinyWasmValue::Value32(value)) => Ok(TinyWasmValue::Value32(value as u16 as u32)),
                (StorageType::Value(WasmType::I32 | WasmType::F32), value @ TinyWasmValue::Value32(_))
                | (StorageType::Value(WasmType::I64 | WasmType::F64), value @ TinyWasmValue::Value64(_))
                | (StorageType::Value(WasmType::V128), value @ TinyWasmValue::Value128(_))
                | (StorageType::Value(WasmType::Ref(_)), value @ TinyWasmValue::ValueRef(_)) => Ok(value),
                _ => Err(Error::other("type mismatch in GC constant")),
            }
        };

        if let [instr] = const_instrs {
            match instr {
                I32Const(value) => return Ok(TinyWasmValue::Value32(*value as u32)),
                I64Const(value) => return Ok(TinyWasmValue::Value64(*value as u64)),
                F32Const(value) => return Ok(TinyWasmValue::Value32(value.to_bits())),
                F64Const(value) => return Ok(TinyWasmValue::Value64(value.to_bits())),
                V128Const(value) => return Ok(TinyWasmValue::Value128((*value).into())),
                GlobalGet(index) => return global_value(&self.state, *index),
                Ref(tinywasm_types::RefValue::Null) => return Ok(TinyWasmValue::ValueRef(ValueRef::NULL)),
                Ref(tinywasm_types::RefValue::Func(func)) => {
                    return Ok(TinyWasmValue::ValueRef(func_ref(func.addr())?));
                }
                _ => {}
            }
        }

        let mut stack = Vec::new();
        for instr in const_instrs {
            match instr {
                I32Const(i) => stack.push(TinyWasmValue::Value32(*i as u32)),
                I64Const(i) => stack.push(TinyWasmValue::Value64(*i as u64)),
                F32Const(f) => stack.push(TinyWasmValue::Value32(f.to_bits())),
                F64Const(f) => stack.push(TinyWasmValue::Value64(f.to_bits())),
                V128Const(i) => stack.push(TinyWasmValue::Value128((*i).into())),
                GlobalGet(index) => stack.push(global_value(&self.state, *index)?),
                Ref(tinywasm_types::RefValue::Null) => stack.push(TinyWasmValue::ValueRef(ValueRef::NULL)),
                Ref(tinywasm_types::RefValue::Func(func)) => {
                    stack.push(TinyWasmValue::ValueRef(func_ref(func.addr())?))
                }
                Ref(_) => {
                    cold_path();
                    return Err(Error::other("unsupported reference constant"));
                }
                RefI31 => {
                    let value = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
                    let TinyWasmValue::Value32(value) = value else {
                        return Err(Error::other("type mismatch in const ref.i31"));
                    };
                    stack.push(TinyWasmValue::ValueRef(ValueRef::from_i31(value as i32)));
                }
                AnyConvertExtern | ExternConvertAny => {
                    let value = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
                    if !matches!(value, TinyWasmValue::ValueRef(_)) {
                        return Err(Error::other("type mismatch in const reference conversion"));
                    }
                    stack.push(value);
                }
                StructNew(type_index) | StructNewDefault(type_index) => {
                    let type_addr = type_addr(*type_index)?;
                    let fields = self
                        .state
                        .get_type(type_addr)
                        .as_struct()
                        .ok_or_else(|| Error::other("GC constant type is not a struct"))?
                        .fields
                        .clone();
                    let default = matches!(instr, StructNewDefault(_));
                    let mut values = Vec::new();
                    values.try_reserve_exact(fields.len()).map_err(|_| Trap::OutOfMemory)?;
                    if default {
                        values.extend(fields.iter().map(|field| default_value(field.storage)));
                    } else {
                        for field in fields.iter().rev() {
                            values.push(pop_value(&mut stack, field.storage)?);
                        }
                        values.reverse();
                    }
                    let roots = stack.iter().filter_map(|value| value.as_ref()).map(ValueRef::raw);
                    let reference = self.state.alloc_gc_object(type_addr, values, roots)?;
                    stack.push(TinyWasmValue::ValueRef(reference));
                }
                ArrayNew(type_index) | ArrayNewDefault(type_index) => {
                    let type_addr = type_addr(*type_index)?;
                    let storage = self
                        .state
                        .get_type(type_addr)
                        .as_array()
                        .ok_or_else(|| Error::other("GC constant type is not an array"))?
                        .field
                        .storage;
                    let Some(TinyWasmValue::Value32(len)) = stack.pop() else {
                        return Err(Error::other("type mismatch in const array length"));
                    };
                    let value = if matches!(instr, ArrayNewDefault(_)) {
                        default_value(storage)
                    } else {
                        pop_value(&mut stack, storage)?
                    };
                    let mut values = Vec::new();
                    values.try_reserve_exact(len as usize).map_err(|_| Trap::OutOfMemory)?;
                    values.resize(len as usize, value);
                    let roots = stack.iter().filter_map(|value| value.as_ref()).map(ValueRef::raw);
                    let reference = self.state.alloc_gc_object(type_addr, values, roots)?;
                    stack.push(TinyWasmValue::ValueRef(reference));
                }
                ArrayNewFixed(type_index, len) => {
                    let type_addr = type_addr(*type_index)?;
                    let storage = self
                        .state
                        .get_type(type_addr)
                        .as_array()
                        .ok_or_else(|| Error::other("GC constant type is not an array"))?
                        .field
                        .storage;
                    let mut values = Vec::new();
                    values.try_reserve_exact(*len as usize).map_err(|_| Trap::OutOfMemory)?;
                    for _ in 0..*len {
                        values.push(pop_value(&mut stack, storage)?);
                    }
                    values.reverse();
                    let roots = stack.iter().filter_map(|value| value.as_ref()).map(ValueRef::raw);
                    let reference = self.state.alloc_gc_object(type_addr, values, roots)?;
                    stack.push(TinyWasmValue::ValueRef(reference));
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
                        _ => {
                            cold_path();
                            return Err(Error::other("invalid const instruction in i32 op"));
                        }
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
                        _ => {
                            cold_path();
                            return Err(Error::other("invalid const instruction in i64 op"));
                        }
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
}

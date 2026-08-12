use alloc::{boxed::Box, format, vec::Vec};
use core::hint::cold_path;
use core::sync::atomic::{AtomicU32, Ordering};
use tinywasm_types::*;

use crate::func::FromWasmValues;
use crate::interpreter::stack::{CallStack, StackBase, ValueStack};
use crate::interpreter::{TinyWasmValue, ValueRef};
use crate::{Engine, Error, ModuleInstance, Result, Trap};

mod const_expr;
mod data;
mod element;
mod exception;
mod function;
mod gc;
mod global;
mod memory;
mod state;
mod table;
mod tag;
mod types;

use const_expr::eval_const;
pub(crate) use gc::{decode_data, default_value, pop_value, push_value};
pub use memory::{LazyLinearMemory, LinearMemory, MemoryBackend, PagedMemory, VecMemory};
pub(crate) use memory::{MemValue, MemoryInstance};
pub(crate) use state::State;
pub(crate) use types::{canonicalize_ref_type, canonicalize_value_type};
pub(crate) use {data::*, element::*, exception::*, function::*, global::*, table::*, tag::*};

// global store id counter
static STORE_ID: AtomicU32 = AtomicU32::new(0);

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
    id: u32,
    pub(crate) module_instances: Vec<ModuleInstance>,

    pub(crate) engine: Engine,
    pub(crate) execution_fuel: u32,
    pub(crate) execution_active: bool,
    pub(crate) state: State,
    pub(crate) call_stack: CallStack,
    pub(crate) value_stack: ValueStack,
    pub(crate) host_params: Vec<WasmValue>,
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
    /// Create a new store
    pub fn new(engine: Engine) -> Self {
        let id =
            STORE_ID.try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1)).expect("too many stores");
        let state = State::new(engine.config().gc_collection_threshold);
        Self {
            id,
            module_instances: Vec::new(),
            state,
            call_stack: CallStack::new(engine.config()),
            value_stack: ValueStack::new(engine.config()),
            host_params: Vec::new(),
            engine,
            execution_fuel: 0,
            execution_active: false,
        }
    }

    /// Get the store's ID (unique per process)
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Get a module instance by the internal id
    pub fn get_module_instance(&self, id: ModuleInstanceId) -> Option<&ModuleInstance> {
        self.module_instances.get(id as usize)
    }

    pub(crate) fn next_module_instance_id(&self) -> ModuleInstanceId {
        self.module_instances.len() as ModuleInstanceId
    }

    pub(crate) fn add_instance(&mut self, instance: ModuleInstance) {
        debug_assert!(instance.id() == self.module_instances.len() as ModuleInstanceId);
        self.module_instances.push(instance);
    }

    /// Returns whether a public value has the requested runtime type.
    #[doc(hidden)]
    pub fn value_matches_type(&self, value: WasmValue, ty: WasmType) -> bool {
        self.state.value_matches_type(value, ty)
    }

    /// Marks the store as executing and rejects nested root calls.
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

    /// Marks the current root execution as complete.
    pub(crate) fn exit_execution(&mut self) {
        self.execution_active = false;
    }

    /// Validates and pushes typed parameters or results onto the value stack.
    pub(crate) fn push_typed_values<const RESULTS: bool>(
        &mut self,
        type_addr: TypeAddr,
        values: impl Iterator<Item = WasmValue>,
        stack_base: StackBase,
    ) -> Result<()> {
        let result: Result<()> = (|| {
            let ty = self.state.get_canonical_func_type(type_addr);
            let expected = if RESULTS { ty.results() } else { ty.params() };
            let mut values = values;
            for &ty in expected {
                let value = values.next().ok_or_else(|| Error::other("not enough typed function values"))?;
                if !self.state.value_matches_type(value, ty) {
                    return Err(Error::other("typed function value does not match its signature"));
                }
                self.value_stack.extend_wasmvalues(core::iter::once(value))?;
            }
            if values.next().is_some() {
                return Err(Error::other("too many typed function values"));
            }
            Ok(())
        })();
        if result.is_err() {
            self.value_stack.truncate_to_base(stack_base);
        }
        result
    }

    /// Reads typed results from the value stack and restores its previous base.
    pub(crate) fn take_typed_results<R: FromWasmValues>(
        &mut self,
        type_addr: TypeAddr,
        stack_base: StackBase,
        pin_refs: bool,
    ) -> Result<R> {
        let types = self.state.get_canonical_func_type(type_addr).results();
        let mut values = self.value_stack.wasm_values(&self.state, types, stack_base, pin_refs);
        let result = R::from_wasm_values(&mut values).and_then(|result| {
            if values.next().is_some() {
                Err(Error::other("typed conversion did not consume all WebAssembly values"))
            } else {
                Ok(result)
            }
        });
        drop(values);
        self.value_stack.truncate_to_base(stack_base);
        result
    }

    /// Add functions to the store, returning their addresses in the store
    pub(crate) fn init_funcs(
        &mut self,
        funcs: &[alloc::sync::Arc<WasmFunction>],
        owner: ModuleInstanceId,
        module_type_idxs: &[TypeAddr],
        type_addrs: &[TypeAddr],
    ) -> impl ExactSizeIterator<Item = FuncAddr> {
        let start = self.state.funcs.len() as FuncAddr;
        debug_assert_eq!(funcs.len(), module_type_idxs.len());
        self.state.funcs.reserve_exact(funcs.len());
        for (func, &type_idx) in funcs.iter().cloned().zip(module_type_idxs) {
            let type_addr = type_addrs[type_idx as usize];
            self.state.funcs.push(FunctionInstance {
                type_addr,
                gc: self.state.func_gc_metadata(type_addr),
                kind: FunctionKind::Wasm(WasmFunctionInstance { func, owner }),
            });
        }
        start..start + funcs.len() as FuncAddr
    }

    /// Add tags to the store, returning their addresses in the store.
    pub(crate) fn init_tags(
        &mut self,
        tags: &[TagType],
        type_addrs: &[TypeAddr],
    ) -> impl ExactSizeIterator<Item = TagAddr> {
        let start = self.state.tags.len() as TagAddr;
        self.state.tags.reserve_exact(tags.len());
        self.state.tags.extend(tags.iter().map(|tag| TagInstance { type_addr: type_addrs[tag.type_idx as usize] }));
        start..start + tags.len() as TagAddr
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
                Some(expr) => match eval_const(&mut self.state, expr, global_addrs, func_addrs, type_addrs)? {
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
            self.state.tables.push(TableInstance::new(ty, init)?);
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
        self.state.memories.reserve_exact(memories.len());
        for mem in memories {
            self.state.memories.push(cold_err!(init(*mem, &self.engine.config().memory_backend))?);
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
        self.state.globals.reserve(globals);
        for global in globals {
            let value = cold_err!(eval_const(&mut self.state, &global.init, out, func_addrs, type_addrs))?;
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
            ElementItem::Expr(expr) => match eval_const(&mut self.state, expr, globals, funcs, type_addrs)? {
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
    ) -> Result<(Box<[ElemAddr]>, Option<Trap>)> {
        let elem_count = self.state.elements.len();
        let mut elem_addrs = Vec::with_capacity(elements.len());
        self.state.elements.reserve_exact(elements.len());
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
                    let offset = match eval_const(&mut self.state, offset, global_addrs, func_addrs, type_addrs)? {
                        TinyWasmValue::Value32(value) => u64::from(value),
                        TinyWasmValue::Value64(value) => value,
                        other => return Err(Error::Other(format!("expected i32 or i64, got {other:?}"))),
                    };
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
            elem_addrs.push((i + elem_count) as ElemAddr);
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
    ) -> Result<(Box<[DataAddr]>, Option<Trap>)> {
        let data_count = self.state.data.len();
        let mut data_addrs = Vec::with_capacity(data.len());
        self.state.data.reserve_exact(data.len());
        for (i, data) in data.iter().enumerate() {
            let data_val = match &data.kind {
                tinywasm_types::DataKind::Active { mem: mem_addr, offset } => {
                    let Some(mem_addr) = mem_addrs.get(*mem_addr as usize) else {
                        return Err(Error::Other(format!("memory {mem_addr} not found for data segment {i}")));
                    };

                    let offset = match eval_const(&mut self.state, offset, global_addrs, func_addrs, type_addrs)? {
                        TinyWasmValue::Value32(value) => u64::from(value),
                        TinyWasmValue::Value64(value) => value,
                        other => return Err(Error::Other(format!("expected i32 or i64, got {other:?}"))),
                    };
                    let Some(mem) = self.state.memories.get_mut(*mem_addr as usize) else {
                        return Err(Error::Other(format!("memory {mem_addr} not found for data segment {i}")));
                    };

                    let offset = usize::try_from(offset).unwrap_or(usize::MAX);
                    match mem.inner.write_all(offset, &data.data)? {
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
            data_addrs.push((i + data_count) as DataAddr);
        }

        // this should be optimized out by the compiler
        Ok((data_addrs.into_boxed_slice(), None))
    }

    /// Adds a function and returns its store address.
    pub(crate) fn add_func(&mut self, func: FunctionInstance) -> FuncAddr {
        let addr = self.state.funcs.len() as FuncAddr;
        self.state.funcs.push(func);
        addr
    }
}

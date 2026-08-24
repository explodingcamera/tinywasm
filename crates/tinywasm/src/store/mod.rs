use alloc::{boxed::Box, format, vec::Vec};
use core::hint::cold_path;
use tinywasm_types::*;

use crate::func::FromWasmValues;
use crate::interpreter::stack::{CallStack, StackBase, ValueStack};
use crate::interpreter::{RuntimeValue, ValueRef};
use crate::reference::{ReferentKind, RootedItem, StoreId, StoredRef};
use crate::{Engine, Error, ExnRef, FuncRef, ModuleInstance, RefValue, Result, Trap, WasmValue};

mod const_expr;
mod data;
mod element;
mod function;
mod gc;
mod global;
mod memory;
mod state;
mod table;
mod tag;
mod types;

use const_expr::eval_const;
pub(crate) use gc::{GcObjectKind, data_range, decode_data, default_value, pop_value, push_value};
pub(crate) use memory::{MemValue, MemoryInstance};
pub(crate) use state::State;
pub(crate) use types::{canonicalize_ref_type, canonicalize_value_type};
pub(crate) use {data::*, element::*, function::*, global::*, table::*, tag::*};

/// Controls selected allocation requests from WebAssembly instances.
///
/// Configure a limiter with
/// [`Config::with_resource_limiter`](crate::engine::Config::with_resource_limiter). The callbacks
/// cover linear memory, tables, and the logical GC heap. They do not account for stacks, runtime
/// metadata, temporary buffers, backing-capacity overhead, or other host allocations.
///
/// `Ok(true)` allows an allocation attempt, `Ok(false)` rejects it, and `Err` returns the supplied
/// trap. Allowing a request does not guarantee that the backing allocation will succeed. Rejected
/// growth uses the operation's normal failed-growth result. Rejected initial allocation and GC
/// allocation produce [`Trap::OutOfMemory`].
///
/// # Example
/// ```rust
/// use std::sync::Arc;
/// use tinywasm::engine::Config;
/// use tinywasm::types::{MemoryArch, MemoryType};
/// use tinywasm::{Engine, Memory, ResourceLimiter, Store};
///
/// struct MemoryLimit(usize);
///
/// impl ResourceLimiter for MemoryLimit {
///     fn memory_growing(
///         &self,
///         _current: usize,
///         desired: usize,
///         _maximum: Option<usize>,
///     ) -> Result<bool, tinywasm::Trap> {
///         Ok(desired <= self.0)
///     }
/// }
///
/// let config = Config::new().with_resource_limiter(Arc::new(MemoryLimit(64 * 1024)));
/// let mut store = Store::new(Engine::new(config));
/// let memory = Memory::try_new(&mut store, MemoryType::new(MemoryArch::I32, 1, None, None))?;
/// assert_eq!(memory.grow(&mut store, 1)?, None);
/// # Ok::<(), tinywasm::Error>(())
/// ```
pub trait ResourceLimiter: Send + Sync {
    /// Checks a nonzero memory allocation or growth request.
    ///
    /// Sizes are in bytes. `current` is zero for initial allocation. `maximum` is the declared
    /// maximum in bytes, saturated to `usize::MAX` if needed, or `None` if no maximum was declared.
    /// Growth limits are checked before this callback. The default implementation allows the
    /// request.
    fn memory_growing(
        &self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> core::result::Result<bool, Trap> {
        Ok(true)
    }

    /// Checks a nonzero table allocation or growth request.
    ///
    /// Sizes are in elements. `current` is zero for initial allocation. `maximum` is the declared
    /// maximum element count when it fits `usize`, or `None` otherwise. Table limits are checked
    /// before this callback. The default implementation allows the request.
    fn table_growing(
        &self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> core::result::Result<bool, Trap> {
        Ok(true)
    }

    /// Checks a GC object allocation request.
    ///
    /// Sizes are the current TinyWasm-accounted heap bytes and that count plus the requested
    /// allocation. They include unreachable objects until collection but not all allocator
    /// overhead. `maximum` is currently always `None`. This callback runs before threshold-triggered
    /// collection.
    fn gc_growing(
        &self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> core::result::Result<bool, Trap> {
        Ok(true)
    }
}

/// Runtime state used by WebAssembly instances and host functions.
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
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#store>
pub struct Store {
    id: StoreId,
    pub(crate) module_instances: Vec<ModuleInstance>,

    pub(crate) engine: Engine,
    pub(crate) execution_fuel: u32,
    pub(crate) execution_active: bool,
    pub(crate) state: State,
    pub(crate) call_stack: CallStack,
    pub(crate) value_stack: ValueStack,
    value_scratch: ValueScratch,
}

#[derive(Default)]
struct ValueScratch(Vec<WasmValue>);

impl ValueScratch {
    fn take_resized(&mut self, len: usize) -> Result<Vec<WasmValue>> {
        self.0.clear();
        self.0.try_reserve(len).map_err(|_| Trap::OutOfMemory)?;
        self.0.resize(len, WasmValue::I32(0));
        Ok(core::mem::take(&mut self.0))
    }

    fn restore(&mut self, mut values: Vec<WasmValue>) {
        values.clear();
        if values.capacity() > self.0.capacity() {
            self.0 = values;
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum FuncValueTypes {
    Params,
    Results,
}

pub(crate) struct StackValueIter<'a> {
    store: &'a mut Store,
    type_addr: TypeAddr,
    types: FuncValueTypes,
    index: StackBase,
    position: usize,
    len: usize,
}

impl Iterator for StackValueIter<'_> {
    type Item = WasmValue;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position == self.len {
            return None;
        }
        let ty = {
            let func_ty = self.store.state.get_canonical_func_type(self.type_addr);
            match self.types {
                FuncValueTypes::Params => func_ty.params()[self.position],
                FuncValueTypes::Results => func_ty.results()[self.position],
            }
        };
        self.position += 1;
        let value = self.store.stack_value(ty, &mut self.index);
        Some(
            value
                .into_wasm(self.store, ty)
                .unwrap_or_else(|_| unreachable!("invalid internal value at a typed host boundary")),
        )
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len - self.position;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for StackValueIter<'_> {}

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
        let id = StoreId::fresh();
        let state = State::new(engine.config());
        Self {
            id,
            module_instances: Vec::new(),
            state,
            call_stack: CallStack::new(engine.config()),
            value_stack: ValueStack::new(engine.config()),
            value_scratch: ValueScratch::default(),
            engine,
            execution_fuel: 0,
            execution_active: false,
        }
    }

    /// Get the store's ID (unique per process)
    pub fn id(&self) -> u32 {
        self.id.get()
    }

    pub(crate) const fn store_id(&self) -> StoreId {
        self.id
    }

    pub(crate) fn with_scratch_values<T>(
        &mut self,
        len: usize,
        use_values: impl FnOnce(&mut Self, &mut Vec<WasmValue>) -> Result<T>,
    ) -> Result<T> {
        let mut values = self.value_scratch.take_resized(len)?;
        let result = use_values(self, &mut values);
        self.value_scratch.restore(values);
        result
    }

    pub(crate) fn root_reference<T: StoredRef>(&mut self, value: ValueRef, kind: ReferentKind) -> Result<T> {
        let token = match kind {
            ReferentKind::Struct | ReferentKind::Array | ReferentKind::Exception => {
                Some(self.state.roots.insert(value)?)
            }
            ReferentKind::I31 | ReferentKind::HostExtern => None,
        };
        Ok(T::from_rooted_item(RootedItem { store: self.id, value, kind, _token: token }))
    }

    pub(crate) fn root_exception(&mut self, value: ValueRef) -> Result<ExnRef> {
        if !matches!(self.state.gc.get(value).map(|object| object.kind), Some(gc::GcObjectKind::Exception(_))) {
            return Err(Trap::InvalidReference.into());
        }
        self.root_reference(value, ReferentKind::Exception)
    }

    pub(crate) fn resolve_ref<T: StoredRef>(&self, reference: &T) -> Result<ValueRef, Trap> {
        let item = reference.rooted_item();
        if item.store != self.id {
            return Err(Trap::InvalidStore);
        }
        Ok(item.value)
    }

    pub(crate) fn encode_ref(&self, value: &RefValue) -> Result<ValueRef> {
        Ok(match value {
            RefValue::Null => ValueRef::NULL,
            RefValue::Func(value) => ValueRef::from_category_addr(value.addr(self.id).ok_or(Trap::InvalidStore)?),
            RefValue::Any(value) => self.resolve_ref(value)?,
            RefValue::Extern(value) => self.resolve_ref(value)?,
            RefValue::Exn(value) => self.resolve_ref(value)?,
        })
    }

    pub(crate) fn decode_ref(&mut self, value: ValueRef, ty: RefType) -> Result<RefValue> {
        if value.is_null() {
            return Ok(RefValue::Null);
        }
        if ty.type_index().is_some_and(|addr| self.state.get_type(addr).as_func().is_some()) || ty.is_func() {
            return Ok(RefValue::Func(FuncRef::new(self.id, value.addr().ok_or(Trap::InvalidReference)?)));
        }
        if ty.is_exn() {
            return self.root_exception(value).map(RefValue::Exn);
        }
        let kind = if value.is_i31() {
            ReferentKind::I31
        } else if value.is_host_any() {
            ReferentKind::HostExtern
        } else {
            match self.state.gc.get(value).map(|object| object.kind) {
                Some(gc::GcObjectKind::Composite(type_addr)) => match &self.state.get_type(type_addr).composite {
                    CompositeType::Struct(_) => ReferentKind::Struct,
                    CompositeType::Array(_) => ReferentKind::Array,
                    CompositeType::Func(_) => return Err(Trap::InvalidReference.into()),
                },
                _ => return Err(Trap::InvalidReference.into()),
            }
        };
        if ty.is_extern() {
            Ok(RefValue::Extern(self.root_reference(value, kind)?))
        } else {
            Ok(RefValue::Any(self.root_reference(value, kind)?))
        }
    }

    fn stack_value(&self, ty: WasmType, index: &mut StackBase) -> RuntimeValue {
        match ty {
            WasmType::I32 | WasmType::F32 | WasmType::Ref(_) => {
                let value = *self.value_stack.stack_32.get(index.s32 as usize);
                index.s32 += 1;
                if matches!(ty, WasmType::Ref(_)) {
                    RuntimeValue::ValueRef(ValueRef::from_raw(value))
                } else {
                    RuntimeValue::Value32(value)
                }
            }
            WasmType::I64 | WasmType::F64 => {
                let value = *self.value_stack.stack_64.get(index.s64 as usize);
                index.s64 += 1;
                RuntimeValue::Value64(value)
            }
            WasmType::V128 => {
                let value = *self.value_stack.stack_128.get(index.s128 as usize);
                index.s128 += 1;
                RuntimeValue::Value128(value)
            }
        }
    }

    pub(crate) fn stack_value_iter(
        &mut self,
        type_addr: TypeAddr,
        types: FuncValueTypes,
        index: StackBase,
    ) -> Result<StackValueIter<'_>, Trap> {
        let canonical = self.state.get_canonical_func_type(type_addr);
        let canonical = match types {
            FuncValueTypes::Params => canonical.params(),
            FuncValueTypes::Results => canonical.results(),
        };
        let len = canonical.len();
        let reference_count = canonical.iter().filter(|ty| matches!(ty, WasmType::Ref(_))).count();
        self.state.roots.reserve(reference_count)?;
        Ok(StackValueIter { store: self, type_addr, types, index, position: 0, len })
    }

    pub(crate) fn push_wasm_values(&mut self, values: impl IntoIterator<Item = WasmValue>) -> Result<()> {
        for value in values {
            let value = value.to_runtime(self)?;
            self.value_stack.push_dyn(value)?;
        }
        Ok(())
    }

    pub(crate) fn pop_stack_values(&mut self, types: &[WasmType], results: &mut [WasmValue]) -> Result<()> {
        if types.len() != results.len() {
            return Err(Error::other("result buffer has the wrong length"));
        }
        let base = self.value_stack.base_before(types.iter().collect());
        let result = (|| {
            let mut index = base;
            for (&ty, result) in types.iter().zip(results) {
                let value = self.stack_value(ty, &mut index);
                *result = value.into_wasm(self, ty)?;
            }
            Ok(())
        })();
        self.value_stack.truncate_to_base(base);
        result
    }

    /// Reclaims unreachable managed objects.
    ///
    /// Globals, tables, operand stacks, and owned host references are traced as
    /// roots. Objects also become eligible for automatic threshold collection
    /// after the last owned handle is dropped.
    pub fn gc(&mut self) -> Result<()> {
        let stack_roots = self.value_stack.stack_32.into_iter().copied().map(ValueRef::from_raw).collect::<Vec<_>>();
        self.state.collect_gc(stack_roots).map_err(|_| Trap::OutOfMemory.into())
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

    pub(crate) fn value_matches_type(&self, value: &WasmValue, ty: WasmType) -> bool {
        let (WasmValue::Ref(value), WasmType::Ref(expected)) = (value, ty) else {
            return value.matches_type(ty);
        };
        let category_matches = match value {
            RefValue::Null => return expected.is_nullable(),
            RefValue::Func(_) => {
                expected.is_func()
                    || expected.type_index().is_some_and(|addr| self.state.get_type(addr).as_func().is_some())
            }
            RefValue::Extern(_) => expected.is_extern(),
            RefValue::Exn(_) => expected.is_exn(),
            RefValue::Any(_) => !expected.is_func() && !expected.is_extern() && !expected.is_exn(),
        };
        if !category_matches {
            return false;
        }
        match self.encode_ref(value) {
            Ok(value) => self.state.value_ref_matches(value, expected),
            _ => false,
        }
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
                let internal = value.to_runtime(self)?;
                if !self.value_matches_type(&value, ty) {
                    return Err(Error::other("typed function value does not match its signature"));
                }
                self.value_stack.push_dyn(internal)?;
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
    ) -> Result<R> {
        let result = {
            let mut values = self.stack_value_iter(type_addr, FuncValueTypes::Results, stack_base)?;
            R::from_wasm_values_exact(&mut values)
        };
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
            self.state
                .funcs
                .push(FunctionInstance { type_addr, kind: FunctionKind::Wasm(WasmFunctionInstance { func, owner }) });
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
        let limiter = self.engine.config().resource_limiter.clone();
        self.state.tables.reserve_exact(tables.len());
        for table in tables {
            let init = match &table.init {
                Some(expr) => match eval_const(&mut self.state, expr, global_addrs, func_addrs, type_addrs)? {
                    RuntimeValue::ValueRef(value) => value,
                    _ => return Err(Error::other("table initializer is not a reference value")),
                },
                None => ValueRef::NULL,
            };
            let element_type = canonicalize_ref_type(table.ty.element_type, type_addrs);
            let ty = match table.ty.arch() {
                MemoryArch::I32 => TableType::new(element_type, table.ty.size_initial, table.ty.size_max),
                MemoryArch::I64 => TableType::new64(element_type, table.ty.size_initial, table.ty.size_max),
            };
            self.state.tables.push(TableInstance::new(ty, init, limiter.as_deref())?);
        }
        Ok(start..start + tables.len() as TableAddr)
    }

    /// Add memories to the store, returning their addresses in the store
    pub(crate) fn init_memories(
        &mut self,
        memories: &[MemoryType],
        init: impl Fn(MemoryType) -> Result<MemoryInstance>,
    ) -> Result<impl ExactSizeIterator<Item = MemAddr>> {
        let start = self.state.memories.len() as MemAddr;
        self.state.memories.reserve_exact(memories.len());
        for mem in memories {
            self.state.memories.push(cold_err!(init(*mem))?);
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
                RuntimeValue::ValueRef(value) => Ok(value),
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
                        RuntimeValue::Value32(value) => u64::from(value),
                        RuntimeValue::Value64(value) => value,
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
                        RuntimeValue::Value32(value) => u64::from(value),
                        RuntimeValue::Value64(value) => value,
                        other => return Err(Error::Other(format!("expected i32 or i64, got {other:?}"))),
                    };
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

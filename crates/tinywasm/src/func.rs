use crate::interpreter::stack::{CallFrame, ValueStack};
use crate::reference::StoreItem;
use crate::{Error, FunctionInstance, InterpreterRuntime, Result, Store, Trap};
use alloc::{borrow::Cow, boxed::Box, format, rc::Rc, vec, vec::Vec};
use core::hint::cold_path;
use tinywasm_types::{ExternRef, FuncAddr, FuncRef, FuncType, ModuleInstanceId, TypeAddr, WasmType, WasmValue};

impl Function {
    #[inline]
    pub(crate) const fn addr(&self) -> FuncAddr {
        self.item.addr
    }

    /// Get this function's canonical type from its store.
    ///
    /// Concrete reference types are only meaningful in this store.
    pub fn ty<'a>(&self, store: &'a Store) -> Result<&'a FuncType> {
        self.item.validate_store(store)?;
        Ok(store.state.get_func_type(self.addr()))
    }

    /// Call a function (Invocation)
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#invocation>
    #[inline]
    pub fn call(&self, store: &mut Store, params: &[WasmValue]) -> Result<Vec<WasmValue>> {
        #[inline]
        fn call_inner(func: &Function, store: &mut Store, params: &[WasmValue]) -> Result<Vec<WasmValue>> {
            let func_instance = store.state.get_func(func.addr()).clone();
            let wasm_func = match &func_instance.kind {
                crate::store::FunctionKind::Host(host_func) => {
                    let result = host_func.clone().call(FuncContext { store, module_id: func.module_id }, params)?;
                    return validate_host_results(store, func_instance.type_addr, result);
                }
                crate::store::FunctionKind::Wasm(wasm_func) => wasm_func,
            };

            // Reset stack, push args, allocate locals, create entry frame.
            store.call_stack.clear();
            store.value_stack.clear();
            store.value_stack.extend_from_wasmvalues(params)?;
            let locals_base = store.value_stack.enter_locals(&wasm_func.func.params, &wasm_func.func.locals)?;
            let callframe = CallFrame::new(func.addr(), locals_base, wasm_func.func.locals);

            // Execute until completion and then collect result values from the stack.
            InterpreterRuntime::exec(store, callframe, 0)?;
            collect_call_results(&mut store.value_stack, store.state.get_type(func_instance.type_addr))
        }

        self.item.validate_store(store)?;
        validate_call_params(&store.state, store.state.get_func_type(self.addr()), params)?;

        store.enter_execution()?;
        let result = call_inner(self, store, params);
        store.exit_execution();

        result
    }

    /// Call a function and return a resumable execution handle.
    ///
    /// The returned handle keeps a mutable borrow of the [`Store`] until it
    /// completes. Use [`FuncExecution::resume_with_fuel`] (or
    /// `resume_with_time_budget` with `std`) to continue.
    pub fn call_resumable<'store>(
        &self,
        store: &'store mut Store,
        params: &[WasmValue],
    ) -> Result<FuncExecution<'store>> {
        #[inline]
        fn call_resumable_inner(
            func: &Function,
            store: &mut Store,
            params: &[WasmValue],
        ) -> Result<FuncExecutionState> {
            let func_instance = store.state.get_func(func.addr()).clone();
            match &func_instance.kind {
                crate::store::FunctionKind::Host(host_func) => {
                    let result = host_func.clone().call(FuncContext { store, module_id: func.module_id }, params)?;
                    let result = validate_host_results(store, func_instance.type_addr, result)?;
                    Ok(FuncExecutionState::Completed { result: Some(result) })
                }
                crate::store::FunctionKind::Wasm(wasm_func) => {
                    store.call_stack.clear();
                    store.value_stack.clear();
                    store.value_stack.extend_from_wasmvalues(params)?;
                    let locals_base = store.value_stack.enter_locals(&wasm_func.func.params, &wasm_func.func.locals)?;
                    let callframe = CallFrame::new(func.addr(), locals_base, wasm_func.func.locals);

                    Ok(FuncExecutionState::Running {
                        exec_state: ExecutionState { callframe },
                        root_func_addr: func.addr(),
                    })
                }
            }
        }

        self.item.validate_store(store)?;
        validate_call_params(&store.state, store.state.get_func_type(self.addr()), params)?;

        store.enter_execution()?;
        let result = call_resumable_inner(self, store, params);
        store.exit_execution();

        Ok(FuncExecution { store, state: result? })
    }
}

#[derive(Clone, PartialEq, Eq)]
/// Progress for fuel-limited function execution.
pub enum ExecProgress<T> {
    /// Execution completed and produced a result.
    Completed(T),
    /// Execution suspended after exhausting fuel or time budget.
    Suspended,
}

#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
pub(crate) struct ExecutionState {
    pub(crate) callframe: CallFrame,
}

/// A function handle
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
pub struct Function {
    pub(crate) item: StoreItem,
    pub(crate) module_id: ModuleInstanceId,
}

/// A typed function handle
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
pub struct FunctionTyped<P, R> {
    /// The underlying function handle
    pub func: Function,
    pub(crate) marker: core::marker::PhantomData<(P, R)>,
}

/// A host function
pub struct HostFunction {
    pub(crate) func: HostFuncInner,
}

impl HostFunction {
    /// Call the function
    pub fn call(&self, ctx: FuncContext<'_>, args: &[WasmValue]) -> Result<Vec<WasmValue>> {
        (self.func)(ctx, args)
    }

    /// Create a new untyped host function import.
    ///
    /// ## Example
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// # use tinywasm::{FuncContext, HostFunction, Imports, ModuleInstance, Store};
    /// # use tinywasm::types::{FuncType, WasmType, WasmValue};
    /// # let wasm = wat::parse_str(r#"
    /// #     (module
    /// #       (import "host" "add_one" (func $add_one (param i32) (result i32)))
    /// #       (func (export "call") (param i32) (result i32)
    /// #         local.get 0
    /// #         call $add_one))
    /// # "#).expect("valid wat");
    /// # let module = tinywasm::parse_bytes(&wasm)?;
    /// let mut store = Store::default();
    /// let ty = FuncType::new(&[WasmType::I32], &[WasmType::I32]);
    /// let add_one = HostFunction::from_untyped(&mut store, &ty, |_ctx: FuncContext<'_>, args| {
    ///     let WasmValue::I32(value) = args[0] else {
    ///         return Err(tinywasm::Error::Other("expected i32".into()));
    ///     };
    ///     Ok(vec![WasmValue::I32(value + 1)])
    /// });
    ///
    /// let mut imports = Imports::new();
    /// imports.define("host", "add_one", add_one);
    /// # let instance = ModuleInstance::instantiate(&mut store, &module, Some(imports))?;
    /// # let call = instance.func::<i32, i32>(&store, "call")?;
    /// # assert_eq!(call.call(&mut store, 41)?, 42);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_untyped(
        store: &mut Store,
        ty: &FuncType,
        func: impl Fn(FuncContext<'_>, &[WasmValue]) -> Result<Vec<WasmValue>> + 'static,
    ) -> Function {
        let type_addr = store.register_host_type(ty);
        let addr = store.add_func(FunctionInstance {
            type_addr,
            kind: crate::store::FunctionKind::Host(Rc::new(Self { func: Box::new(func) })),
        });
        Function { item: crate::StoreItem::new(store.id(), addr), module_id: 0 }
    }

    /// Create a new typed host function import.
    ///
    /// ## Example
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// # use tinywasm::{HostFunction, Imports, ModuleInstance, Store};
    /// # let wasm = wat::parse_str(r#"
    /// #     (module
    /// #       (import "host" "add_one" (func $add_one (param i32) (result i32)))
    /// #       (func (export "call") (param i32) (result i32)
    /// #         local.get 0
    /// #         call $add_one))
    /// # "#).expect("valid wat");
    /// # let module = tinywasm::parse_bytes(&wasm)?;
    /// let mut store = Store::default();
    /// let add_one = HostFunction::from(&mut store, |_ctx, value: i32| Ok(value + 1));
    ///
    /// let mut imports = Imports::new();
    /// imports.define("host", "add_one", add_one);
    /// # let instance = ModuleInstance::instantiate(&mut store, &module, Some(imports))?;
    /// # let call = instance.func::<i32, i32>(&store, "call")?;
    /// # assert_eq!(call.call(&mut store, 41)?, 42);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from<P, R>(store: &mut Store, func: impl Fn(FuncContext<'_>, P) -> Result<R> + 'static) -> Function
    where
        P: FromWasmValues + ToWasmTypes,
        R: IntoWasmValues + ToWasmTypes,
    {
        let inner_func = move |ctx: FuncContext<'_>, args: &[WasmValue]| -> Result<Vec<WasmValue>> {
            Ok(func(ctx, P::from_wasm_values(args)?)?.into_wasm_values())
        };

        let ty = tinywasm_types::FuncType::new(&P::wasm_types(), &R::wasm_types());
        let type_addr = store.register_host_type(&ty);
        let addr = store.add_func(FunctionInstance {
            type_addr,
            kind: crate::store::FunctionKind::Host(Rc::new(Self { func: Box::new(inner_func) })),
        });
        Function { item: crate::StoreItem::new(store.id(), addr), module_id: 0 }
    }
}

pub(crate) type HostFuncInner = Box<dyn Fn(FuncContext<'_>, &[WasmValue]) -> Result<Vec<WasmValue>>>;

/// The context of a host-function call
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
pub struct FuncContext<'a> {
    pub(crate) store: &'a mut crate::Store,
    pub(crate) module_id: ModuleInstanceId,
}

impl FuncContext<'_> {
    /// Get the store.
    pub fn store(&self) -> &crate::Store {
        self.store
    }

    /// Get mutable access to the store.
    pub fn store_mut(&mut self) -> &mut crate::Store {
        self.store
    }

    /// Get the module instance.
    pub fn module(&self) -> crate::ModuleInstance {
        self.store
            .get_module_instance(self.module_id)
            .unwrap_or_else(|| unreachable!("invalid module instance id in host function context: {}", self.module_id))
    }

    /// Get a memory export.
    pub fn memory(&self, name: &str) -> Result<crate::Memory> {
        self.module().memory(name)
    }

    /// Get any exported extern value by name.
    pub fn extern_item(&self, name: &str) -> Result<crate::ExternItem> {
        self.module().extern_item(name)
    }

    /// Get a table export.
    pub fn table(&self, name: &str) -> Result<crate::Table> {
        self.module().table(name)
    }

    /// Get the value of a global export.
    pub fn global_get(&self, name: &str) -> Result<WasmValue> {
        self.module().global_get(self.store, name)
    }

    /// Get a global export.
    pub fn global(&self, name: &str) -> Result<crate::Global> {
        self.module().global(name)
    }

    /// Set the value of a mutable global export.
    pub fn global_set(&mut self, name: &str, value: WasmValue) -> Result<()> {
        self.module().global_set(self.store, name, value)
    }

    /// Charge additional fuel from the currently running resumable invocation.
    ///
    /// This is a no-op when the current invocation is not using fuel-based
    /// resumption.
    pub fn charge_fuel(&mut self, fuel: u32) {
        self.store.execution_fuel = self.store.execution_fuel.saturating_sub(fuel);
    }

    /// Get remaining fuel for the current invocation.
    ///
    /// Returns `0` when fuel-based resumption is not active.
    pub fn remaining_fuel(&self) -> u32 {
        self.store.execution_fuel
    }

    /// Call a function from within the current host-function invocation.
    ///
    /// This is the safe way for host functions to perform blocking reentrant
    /// calls into Wasm. Unlike [`Function::call`], it preserves the active
    /// invocation's stacks and resumes the host caller after the nested call
    /// completes.
    ///
    /// Nested calls are currently blocking only. If the surrounding invocation
    /// is resumed with fuel or a time budget, this method does not suspend and
    /// later continue the host function in the middle of the nested call.
    pub fn call_untyped(&mut self, func: &Function, args: &[WasmValue]) -> Result<Vec<WasmValue>> {
        if !self.store.execution_active {
            return Err(Error::other("FuncContext::call requires an active host-function invocation"));
        }

        func.item.validate_store(self.store)?;
        validate_call_params(&self.store.state, self.store.state.get_func_type(func.addr()), args)?;

        let func_instance = self.store.state.get_func(func.addr()).clone();
        match func_instance.kind {
            crate::store::FunctionKind::Host(host_func) => {
                let result =
                    host_func.call(FuncContext { store: &mut *self.store, module_id: func.module_id }, args)?;
                validate_host_results(self.store, func_instance.type_addr, result)
            }
            crate::store::FunctionKind::Wasm(wasm_func) => {
                let call_stack_base = self.store.call_stack.len();
                let value_stack_base = self.store.value_stack.base();

                self.store.value_stack.extend_from_wasmvalues(args).inspect_err(|_| {
                    self.store.value_stack.truncate_to_base(value_stack_base);
                })?;

                let locals_base = self
                    .store
                    .value_stack
                    .enter_locals(&wasm_func.func.params, &wasm_func.func.locals)
                    .inspect_err(|_| self.store.value_stack.truncate_to_base(value_stack_base))?;

                let callframe = CallFrame::new(func.addr(), locals_base, wasm_func.func.locals);
                InterpreterRuntime::exec(self.store, callframe, call_stack_base).inspect_err(|_| {
                    self.store.call_stack.truncate_to(call_stack_base);
                    self.store.value_stack.truncate_to_base(value_stack_base);
                })?;

                collect_call_results(&mut self.store.value_stack, self.store.state.get_type(func_instance.type_addr))
            }
        }
    }

    /// Call a typed function from within the current host-function invocation.
    ///
    /// See [`Self::call_untyped`] for reentrancy and resumable-execution
    /// limitations.
    pub fn call<P, R>(&mut self, func: &FunctionTyped<P, R>, params: P) -> Result<R>
    where
        P: IntoWasmValues,
        R: FromWasmValues,
    {
        R::from_wasm_values(&self.call_untyped(&func.func, &params.into_wasm_values())?)
    }
}

impl core::ops::Deref for FuncContext<'_> {
    type Target = crate::Store;

    fn deref(&self) -> &Self::Target {
        self.store
    }
}

impl core::ops::DerefMut for FuncContext<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.store
    }
}

impl<'a> FuncContext<'a> {
    /// Create a new host function context.
    pub const fn new(store: &'a mut crate::Store, module_id: ModuleInstanceId) -> Self {
        Self { store, module_id }
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for HostFunction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HostFunction").field("func", &"...").finish()
    }
}

/// Resumable execution for an untyped function call.
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
pub struct FuncExecution<'store> {
    store: &'store mut Store,
    state: FuncExecutionState,
}

#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
enum FuncExecutionState {
    Running { exec_state: ExecutionState, root_func_addr: u32 },
    Completed { result: Option<Vec<WasmValue>> },
}

/// Resumable execution for a typed function call.
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
pub struct FuncExecutionTyped<'store, R> {
    execution: FuncExecution<'store>,
    marker: core::marker::PhantomData<R>,
}

impl<'store> FuncExecution<'store> {
    fn resume(
        &mut self,
        run: impl FnOnce(&mut Store, CallFrame) -> Result<crate::interpreter::ExecState, Trap>,
    ) -> Result<ExecProgress<Vec<WasmValue>>> {
        let (callframe, root_func_addr) = match &mut self.state {
            FuncExecutionState::Running { exec_state, root_func_addr } => (exec_state.callframe, *root_func_addr),
            FuncExecutionState::Completed { result } => {
                return match result.take() {
                    Some(res) => Ok(ExecProgress::Completed(res)),
                    None => Err(Error::other("execution already completed")),
                };
            }
        };

        self.store.enter_execution()?;
        let result = run(self.store, callframe);
        self.store.exit_execution();

        match result? {
            crate::interpreter::ExecState::Completed => {
                let result_ty = self.store.state.get_func(root_func_addr).type_addr;
                self.state = FuncExecutionState::Completed { result: None };
                Ok(ExecProgress::Completed(collect_call_results(
                    &mut self.store.value_stack,
                    self.store.state.get_type(result_ty),
                )?))
            }
            crate::interpreter::ExecState::Suspended(callframe) => {
                let FuncExecutionState::Running { exec_state, .. } = &mut self.state else {
                    unreachable!("invalid function execution state")
                };
                exec_state.callframe = callframe;
                Ok(ExecProgress::Suspended)
            }
        }
    }

    /// Resume execution with up to `fuel` units of fuel.
    ///
    /// Fuel is accounted in chunks, so execution may overshoot the requested
    /// fuel before returning [`ExecProgress::Suspended`] (currently the chunk size is 128 instructions between fuel checks, but this may change in the future).
    ///
    /// Returns [`ExecProgress::Suspended`] when fuel is exhausted, or
    /// [`ExecProgress::Completed`] with the final values once the invocation
    /// returns.
    ///
    /// Reentrant calls made by host functions through [`FuncContext::call`] are
    /// currently blocking. They do not suspend and later resume the host
    /// function in the middle of the nested call.
    pub fn resume_with_fuel(&mut self, fuel: u32) -> Result<ExecProgress<Vec<WasmValue>>> {
        self.resume(|store, callframe| InterpreterRuntime::exec_with_fuel(store, callframe, fuel))
    }

    #[cfg(feature = "std")]
    /// Resume execution for at most `time_budget` wall-clock time.
    ///
    /// Time is checked periodically, so execution may overshoot the requested
    /// time budget before returning [`ExecProgress::Suspended`] (currently time is checked every 128 instructions, but this may change in the future).
    ///
    /// Returns [`ExecProgress::Suspended`] when the budget is exhausted, or
    /// [`ExecProgress::Completed`] with the final values once the invocation
    /// returns.
    ///
    /// Reentrant calls made by host functions through [`FuncContext::call`] are
    /// currently blocking. They do not suspend and later resume the host
    /// function in the middle of the nested call.
    pub fn resume_with_time_budget(
        &mut self,
        time_budget: crate::std::time::Duration,
    ) -> Result<ExecProgress<Vec<WasmValue>>> {
        self.resume(|store, callframe| InterpreterRuntime::exec_with_time_budget(store, callframe, time_budget))
    }
}

fn validate_call_params(state: &crate::store::State, func_ty: &FuncType, params: &[WasmValue]) -> Result<()> {
    if func_ty.params().len() != params.len() {
        cold_path();
        return Err(Error::Other(format!(
            "param count mismatch: expected {}, got {}",
            func_ty.params().len(),
            params.len()
        )));
    }

    if !func_ty.params().iter().zip(params).all(|(ty, param)| state.value_matches_type(*param, *ty)) {
        return Err(Error::other("Type mismatch"));
    }

    Ok(())
}

pub(crate) fn validate_host_results(
    store: &Store,
    type_addr: TypeAddr,
    result: Vec<WasmValue>,
) -> Result<Vec<WasmValue>> {
    let expected = store.state.get_type(type_addr);
    if result.len() == expected.results().len()
        && result.iter().zip(expected.results()).all(|(&value, &ty)| store.state.value_matches_type(value, ty))
    {
        return Ok(result);
    }

    Err(Error::InvalidHostFnReturn { expected: Box::new(expected.clone()), actual: result })
}

fn collect_call_results(value_stack: &mut ValueStack, func_ty: &FuncType) -> Result<Vec<WasmValue>> {
    debug_assert!(value_stack.len() >= func_ty.results().len()); // m values are on the top of the stack (Ensured by validation)
    let mut res: Vec<_> = value_stack.pop_types(func_ty.results().iter().rev()).collect(); // pop in reverse order since the stack is LIFO
    res.reverse(); // reverse to get the original order
    Ok(res)
}

pub trait IntoWasmValues {
    fn into_wasm_values(self) -> Vec<WasmValue>;
}

pub trait FromWasmValues: Sized {
    fn from_wasm_values(values: &[WasmValue]) -> Result<Self>;
}

impl<P: IntoWasmValues, R: FromWasmValues> FunctionTyped<P, R> {
    /// Call a typed function
    pub fn call(&self, store: &mut Store, params: P) -> Result<R> {
        // Convert params into Vec<WasmValue>
        let wasm_values = params.into_wasm_values();

        // Call the underlying WASM function
        let result = self.func.call(store, &wasm_values)?;

        // Convert the Vec<WasmValue> back to R
        R::from_wasm_values(&result)
    }

    /// Call a typed function and return a resumable execution handle.
    ///
    /// The handle keeps a mutable borrow of the [`Store`] until completion.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// use tinywasm::{ExecProgress, ModuleInstance, Store};
    ///
    /// let wasm = include_bytes!("../../../examples/wasm/add.wasm");
    /// let module = tinywasm::parse_bytes(wasm)?;
    /// let mut store = Store::default();
    /// let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    /// let add = instance.func::<(i32, i32), i32>(&store, "add")?;
    ///
    /// let mut execution = add.call_resumable(&mut store, (20, 22))?;
    /// assert!(matches!(execution.resume_with_fuel(0)?, ExecProgress::Suspended));
    /// assert!(matches!(execution.resume_with_fuel(16)?, ExecProgress::Completed(42)));
    /// # Ok(())
    /// # }
    /// ```
    pub fn call_resumable<'store>(&self, store: &'store mut Store, params: P) -> Result<FuncExecutionTyped<'store, R>> {
        let wasm_values = params.into_wasm_values();
        let execution = self.func.call_resumable(store, &wasm_values)?;
        Ok(FuncExecutionTyped { execution, marker: core::marker::PhantomData })
    }
}

impl<'store, R: FromWasmValues> FuncExecutionTyped<'store, R> {
    /// Resume typed execution with up to `fuel` units of fuel.
    ///
    /// Fuel is accounted in chunks, so execution may overshoot the requested
    /// fuel before returning [`ExecProgress::Suspended`].
    pub fn resume_with_fuel(&mut self, fuel: u32) -> Result<ExecProgress<R>> {
        match self.execution.resume_with_fuel(fuel)? {
            ExecProgress::Completed(values) => Ok(ExecProgress::Completed(R::from_wasm_values(&values)?)),
            ExecProgress::Suspended => Ok(ExecProgress::Suspended),
        }
    }

    #[cfg(feature = "std")]
    /// Resume typed execution for at most `time_budget` wall-clock time.
    ///
    /// Time is checked periodically, so execution may overshoot the requested
    /// time budget before returning [`ExecProgress::Suspended`].
    pub fn resume_with_time_budget(&mut self, time_budget: crate::std::time::Duration) -> Result<ExecProgress<R>> {
        match self.execution.resume_with_time_budget(time_budget)? {
            ExecProgress::Completed(values) => Ok(ExecProgress::Completed(R::from_wasm_values(&values)?)),
            ExecProgress::Suspended => Ok(ExecProgress::Suspended),
        }
    }
}

/// Describes the WebAssembly value types produced by a Rust value or tuple shape.
pub trait ToWasmTypes {
    /// Static WebAssembly types for this shape.
    ///
    /// Implementations that require runtime construction may set this to `None`,
    /// but must then override [`Self::wasm_types`].
    const WASM_TYPES: Option<&'static [WasmType]>;

    /// Return the flattened WebAssembly value types for this tuple shape.
    fn wasm_types() -> Cow<'static, [WasmType]> {
        Cow::Borrowed(Self::WASM_TYPES.expect("dynamic ToWasmTypes implementation must override wasm_types"))
    }
}

/// Describes the WebAssembly value types produced by a scalar Rust type.
pub trait ToWasmType {
    /// The single WebAssembly value type for this scalar type.
    const WASM_TYPE: WasmType;
}

macro_rules! impl_scalar_wasm_traits {
    ($($T:ty => $val_ty:expr),+ $(,)?) => {
        $(
            impl ToWasmType for $T {
                const WASM_TYPE: WasmType = $val_ty;
            }

            impl ToWasmTypes for $T {
                const WASM_TYPES: Option<&'static [WasmType]> = Some(&[$val_ty]);
            }

            impl IntoWasmValues for $T {
                #[inline]
                fn into_wasm_values(self) -> Vec<WasmValue> {
                    vec![self.into()]
                }
            }

            impl FromWasmValues for $T {
                #[inline]
                fn from_wasm_values(values: &[WasmValue]) -> Result<Self> {
                    let value = *values.first().ok_or_else(|| {
                        core::hint::cold_path();
                        Error::other("Not enough elements in &[WasmValue]")
                    })?;

                    <$T>::try_from(value).map_err(|e| {
                        core::hint::cold_path();
                        Error::Other(format!(
                            "FromWasmValues: Could not convert WasmValue to expected type: {e:?}"
                        ))
                    })
                }
            }
        )+
    };
}

macro_rules! impl_tuple_traits {
    ($($T:ident),+) => {
        impl<$($T),+> ToWasmTypes for ($($T,)+)
        where
            $($T: ToWasmType,)+
        {
            const WASM_TYPES: Option<&'static [WasmType]> = Some(&[$($T::WASM_TYPE,)+]);
        }

        impl<$($T),+> IntoWasmValues for ($($T,)+)
        where
            $($T: Into<WasmValue>,)+
        {
            #[allow(non_snake_case)]
            #[inline]
            fn into_wasm_values(self) -> Vec<WasmValue> {
                let ($($T,)+) = self;
                vec![$($T.into(),)+]
            }
        }

        impl<$($T),+> FromWasmValues for ($($T,)+)
        where
            $($T: TryFrom<WasmValue, Error = ()>,)+
        {
            #[inline]
            fn from_wasm_values(values: &[WasmValue]) -> Result<Self> {
                let mut iter = values.iter();

                Ok(($(
                    $T::try_from(*iter.next().ok_or(Error::other("Not enough values in WasmValue vector"))?)
                        .map_err(|e| Error::Other(format!("FromWasmValues: Could not convert WasmValue to expected type: {e:?}")))?,
                )+))
            }
        }
    }
}

macro_rules! impl_tuple {
    ($macro:ident) => {
        $macro!(T1);
        $macro!(T1, T2);
        $macro!(T1, T2, T3);
        $macro!(T1, T2, T3, T4);
        $macro!(T1, T2, T3, T4, T5);
        $macro!(T1, T2, T3, T4, T5, T6);
        $macro!(T1, T2, T3, T4, T5, T6, T7);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
    };
}

impl_scalar_wasm_traits!(
    i32 => WasmType::I32,
    i64 => WasmType::I64,
    f32 => WasmType::F32,
    f64 => WasmType::F64,
    FuncRef => WasmType::Ref(tinywasm_types::RefType::FUNCREF),
    ExternRef => WasmType::Ref(tinywasm_types::RefType::EXTERNREF),
);
impl_tuple!(impl_tuple_traits);

/// A helper type for using tuples of arbitrary number of elements as function parameters or results,
/// by concatenating the Wasm types of each element.
///
/// This is useful when a function signature exceeds tuple arity 12. `tinywasm` only implements
/// direct tuple conversions up to arity 12, but `WasmTupleChain` lets you describe longer
/// signatures by combining smaller tuples at the type level.
///
/// ## Example
/// ```rust
/// # fn main() -> tinywasm::Result<()> {
/// # use tinywasm::{ModuleInstance, Store, WasmTupleChain};
/// # let wasm = wat::parse_str(r#"
/// #     (module
/// #       (func (export "echo13")
/// #         (param i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32)
/// #         (result i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32)
/// #         local.get 0
/// #         local.get 1
/// #         local.get 2
/// #         local.get 3
/// #         local.get 4
/// #         local.get 5
/// #         local.get 6
/// #         local.get 7
/// #         local.get 8
/// #         local.get 9
/// #         local.get 10
/// #         local.get 11
/// #         local.get 12)
/// #     )
/// # "#).expect("valid wat");
/// # let module = tinywasm::parse_bytes(&wasm)?;
/// # let mut store = Store::default();
/// # let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
///
/// type Params =
///     WasmTupleChain<(i32, i32, i32, i32, i32, i32), (i32, i32, i32, i32, i32, i32, i32)>;
/// type Results =
///     WasmTupleChain<(i32, i32, i32, i32, i32, i32), (i32, i32, i32, i32, i32, i32, i32)>;
///
/// let echo13 = instance.func::<Params, Results>(&store, "echo13")?;
/// let result = echo13.call(&mut store, ((1, 2, 3, 4, 5, 6), (7, 8, 9, 10, 11, 12, 13)).into())?;
/// assert_eq!(result.into_inner(), ((1, 2, 3, 4, 5, 6), (7, 8, 9, 10, 11, 12, 13)));
/// # Ok(())
/// # }
/// ```
#[derive(Default)]
pub struct WasmTupleChain<T1, T2>(T1, T2);

impl<T1, T2> WasmTupleChain<T1, T2> {
    /// Create a new concatenated tuple wrapper.
    pub const fn new(left: T1, right: T2) -> Self {
        Self(left, right)
    }

    /// Split the wrapper back into its two component values.
    pub fn into_inner(self) -> (T1, T2) {
        (self.0, self.1)
    }
}

impl<T1, T2> From<(T1, T2)> for WasmTupleChain<T1, T2> {
    fn from((left, right): (T1, T2)) -> Self {
        Self::new(left, right)
    }
}

impl<T1: ToWasmTypes, T2: ToWasmTypes> ToWasmTypes for WasmTupleChain<T1, T2> {
    const WASM_TYPES: Option<&'static [WasmType]> = None;

    #[inline]
    fn wasm_types() -> Cow<'static, [WasmType]> {
        let mut types = Vec::new();
        types.extend_from_slice(&T1::wasm_types());
        types.extend_from_slice(&T2::wasm_types());
        Cow::Owned(types)
    }
}

impl<T1: IntoWasmValues, T2: IntoWasmValues> IntoWasmValues for WasmTupleChain<T1, T2> {
    #[inline]
    fn into_wasm_values(self) -> Vec<WasmValue> {
        let (left, right) = self.into_inner();
        let mut values = Vec::new();
        values.extend(left.into_wasm_values());
        values.extend(right.into_wasm_values());
        values
    }
}

impl<T1: FromWasmValues + ToWasmTypes, T2: FromWasmValues> FromWasmValues for WasmTupleChain<T1, T2> {
    #[inline]
    fn from_wasm_values(values: &[WasmValue]) -> Result<Self> {
        let left_len = T1::wasm_types().len();
        let left = T1::from_wasm_values(&values[..values.len().min(left_len)])?;
        let right = T2::from_wasm_values(values.get(left_len..).unwrap_or(&[]))?;
        Ok(Self::new(left, right))
    }
}

impl ToWasmTypes for () {
    const WASM_TYPES: Option<&'static [WasmType]> = Some(&[]);
}

impl IntoWasmValues for () {
    #[inline]
    fn into_wasm_values(self) -> Vec<WasmValue> {
        vec![]
    }
}

impl FromWasmValues for () {
    #[inline]
    fn from_wasm_values(_values: &[WasmValue]) -> Result<Self> {
        Ok(())
    }
}

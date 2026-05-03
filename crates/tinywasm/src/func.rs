use crate::interpreter::stack::{CallFrame, ValueStack};
use crate::reference::StoreItem;
use crate::{Error, FunctionInstance, InterpreterRuntime, Result, Store, unlikely};
use alloc::{boxed::Box, format, rc::Rc, string::ToString, sync::Arc, vec, vec::Vec};
use tinywasm_types::{ExternRef, FuncRef, FuncType, ModuleInstanceAddr, WasmType, WasmValue};

impl Function {
    /// Call a function (Invocation)
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#invocation>
    #[inline]
    pub fn call(&self, store: &mut Store, params: &[WasmValue]) -> Result<Vec<WasmValue>> {
        self.item.validate_store(store)?;
        validate_call_params(&self.ty, params)?;

        let wasm_func = match store.state.get_func(self.addr) {
            FunctionInstance::Host(host_func) => {
                return host_func.clone().call(FuncContext { store, module_addr: self.module_addr }, params);
            }
            FunctionInstance::Wasm(wasm_func) => wasm_func,
        };

        // Reset stack, push args, allocate locals, create entry frame.
        store.call_stack.clear();
        store.value_stack.clear();
        store.value_stack.extend_from_wasmvalues(params)?;
        let locals_base = store.value_stack.enter_locals(&wasm_func.func.params, &wasm_func.func.locals)?;
        let callframe = CallFrame::new(self.addr, locals_base, wasm_func.func.locals);

        // Execute until completion and then collect result values from the stack.
        InterpreterRuntime::exec(store, callframe)?;
        collect_call_results(&mut store.value_stack, &self.ty)
    }

    /// Call a function and return a resumable execution handle.
    ///
    /// The returned handle keeps a mutable borrow of the [`Store`] until it
    /// completes. Use [`FuncExecution::resume_with_fuel`] (or
    /// [`FuncExecution::resume_with_time_budget`] with `std`) to continue.
    pub fn call_resumable<'store>(
        &self,
        store: &'store mut Store,
        params: &[WasmValue],
    ) -> Result<FuncExecution<'store>> {
        self.item.validate_store(store)?;
        validate_call_params(&self.ty, params)?;

        match store.state.get_func(self.addr) {
            FunctionInstance::Host(host_func) => {
                let result = host_func.clone().call(FuncContext { store, module_addr: self.module_addr }, params)?;
                Ok(FuncExecution { store, state: FuncExecutionState::Completed { result: Some(result) } })
            }
            FunctionInstance::Wasm(wasm_func) => {
                store.call_stack.clear();
                store.value_stack.clear();
                store.value_stack.extend_from_wasmvalues(params)?;
                let locals_base = store.value_stack.enter_locals(&wasm_func.func.params, &wasm_func.func.locals)?;
                let callframe = CallFrame::new(self.addr, locals_base, wasm_func.func.locals);

                Ok(FuncExecution {
                    store,
                    state: FuncExecutionState::Running {
                        exec_state: ExecutionState { callframe },
                        root_func_addr: self.addr,
                    },
                })
            }
        }
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
    pub(crate) module_addr: ModuleInstanceAddr,
    pub(crate) addr: u32,
    pub(crate) ty: Arc<FuncType>,
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
    pub(crate) ty: Arc<tinywasm_types::FuncType>,
    pub(crate) func: HostFuncInner,
}

impl HostFunction {
    /// Get the function's type
    pub fn ty(&self) -> &Arc<tinywasm_types::FuncType> {
        &self.ty
    }

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
        let ty = Arc::new(ty.clone());
        let ty_inner = ty.clone();
        let inner_func = move |ctx: FuncContext<'_>, args: &[WasmValue]| -> Result<Vec<WasmValue>> {
            let ty = ty_inner.clone();
            let result = func(ctx, args)?;

            if result.len() != ty.results().len() {
                return Err(crate::Error::InvalidHostFnReturn { expected: ty.clone(), actual: result });
            };

            result.iter().zip(ty.results().iter()).try_for_each(|(val, res_ty)| {
                if WasmType::from(val) != *res_ty {
                    return Err(crate::Error::InvalidHostFnReturn { expected: ty.clone(), actual: result.clone() });
                }
                Ok(())
            })?;

            Ok(result)
        };

        let addr = store.add_func(FunctionInstance::Host(Rc::new(Self { func: Box::new(inner_func), ty: ty.clone() })));
        Function { item: crate::StoreItem::new(store.id(), addr), module_addr: 0, addr, ty: ty.clone() }
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
            let args = P::from_wasm_values(args)?;
            let result = func(ctx, args)?;
            Ok(result.into_wasm_values())
        };

        let ty = Arc::new(tinywasm_types::FuncType::new(&P::wasm_types(), &R::wasm_types()));
        let addr = store.add_func(FunctionInstance::Host(Rc::new(Self { func: Box::new(inner_func), ty: ty.clone() })));
        Function { item: crate::StoreItem::new(store.id(), addr), module_addr: 0, addr, ty }
    }
}

pub(crate) type HostFuncInner = Box<dyn Fn(FuncContext<'_>, &[WasmValue]) -> Result<Vec<WasmValue>>>;

/// The context of a host-function call
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
pub struct FuncContext<'a> {
    pub(crate) store: &'a mut crate::Store,
    pub(crate) module_addr: ModuleInstanceAddr,
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
        self.store.get_module_instance(self.module_addr).unwrap_or_else(|| {
            unreachable!("invalid module instance address in host function context: {}", self.module_addr)
        })
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
    pub const fn new(store: &'a mut crate::Store, module_addr: ModuleInstanceAddr) -> Self {
        Self { store, module_addr }
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for HostFunction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HostFunction").field("ty", &self.ty).field("func", &"...").finish()
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
    /// Resume execution with up to `fuel` units of fuel.
    ///
    /// Fuel is accounted in chunks, so execution may overshoot the requested
    /// fuel before returning [`ExecProgress::Suspended`].
    ///
    /// Returns [`ExecProgress::Suspended`] when fuel is exhausted, or
    /// [`ExecProgress::Completed`] with the final values once the invocation
    /// returns.
    pub fn resume_with_fuel(&mut self, fuel: u32) -> Result<ExecProgress<Vec<WasmValue>>> {
        let FuncExecutionState::Running { exec_state, root_func_addr } = &mut self.state else {
            let FuncExecutionState::Completed { result } = &mut self.state else {
                unreachable!("invalid function execution state")
            };
            return result
                .take()
                .map(ExecProgress::Completed)
                .ok_or_else(|| Error::Other("execution already completed".to_string()));
        };

        match InterpreterRuntime::exec_with_fuel(self.store, exec_state.callframe, fuel)? {
            crate::interpreter::ExecState::Completed => {
                let result_ty = self.store.state.get_func(*root_func_addr).ty().clone();
                let result = collect_call_results(&mut self.store.value_stack, &result_ty)?;
                self.state = FuncExecutionState::Completed { result: None };
                Ok(ExecProgress::Completed(result))
            }
            crate::interpreter::ExecState::Suspended(callframe) => {
                exec_state.callframe = callframe;
                Ok(ExecProgress::Suspended)
            }
        }
    }

    #[cfg(feature = "std")]
    /// Resume execution for at most `time_budget` wall-clock time.
    ///
    /// Time is checked periodically, so execution may overshoot the requested
    /// time budget before returning [`ExecProgress::Suspended`].
    ///
    /// Returns [`ExecProgress::Suspended`] when the budget is exhausted, or
    /// [`ExecProgress::Completed`] with the final values once the invocation
    /// returns.
    pub fn resume_with_time_budget(
        &mut self,
        time_budget: crate::std::time::Duration,
    ) -> Result<ExecProgress<Vec<WasmValue>>> {
        let FuncExecutionState::Running { exec_state, root_func_addr } = &mut self.state else {
            let FuncExecutionState::Completed { result } = &mut self.state else {
                unreachable!("invalid function execution state")
            };
            return result
                .take()
                .map(ExecProgress::Completed)
                .ok_or_else(|| Error::Other("execution already completed".to_string()));
        };

        match InterpreterRuntime::exec_with_time_budget(self.store, exec_state.callframe, time_budget)? {
            crate::interpreter::ExecState::Completed => {
                let result_ty = self.store.state.get_func(*root_func_addr).ty().clone();
                let result = collect_call_results(&mut self.store.value_stack, &result_ty)?;
                self.state = FuncExecutionState::Completed { result: None };
                Ok(ExecProgress::Completed(result))
            }
            crate::interpreter::ExecState::Suspended(callframe) => {
                exec_state.callframe = callframe;
                Ok(ExecProgress::Suspended)
            }
        }
    }
}

fn validate_call_params(func_ty: &FuncType, params: &[WasmValue]) -> Result<()> {
    if unlikely(func_ty.params().len() != params.len()) {
        return Err(Error::Other(format!(
            "param count mismatch: expected {}, got {}",
            func_ty.params().len(),
            params.len()
        )));
    }

    if !(func_ty.params().iter().zip(params).all(|(ty, param)| ty == &param.into())) {
        return Err(Error::Other("Type mismatch".into()));
    }

    Ok(())
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
    /// Return the flattened WebAssembly value types for this tuple shape.
    fn wasm_types() -> Box<[WasmType]>;
}

/// Describes the WebAssembly value types produced by a scalar Rust type.
pub trait ToWasmType {
    /// Return the single WebAssembly value type for this scalar type.
    fn wasm_type() -> WasmType;
}

macro_rules! impl_scalar_wasm_traits {
    ($($T:ty => $val_ty:ident),+ $(,)?) => {
        $(
            impl ToWasmType for $T {
                #[inline]
                fn wasm_type() -> WasmType {
                    WasmType::$val_ty
                }
            }

            impl ToWasmTypes for $T {
                #[inline]
                fn wasm_types() -> Box<[WasmType]> {
                    Box::new([WasmType::$val_ty])
                }
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
                    let value = *values
                        .first()
                        .ok_or(Error::Other("Not enough values in WasmValue vector".to_string()))?;
                    <$T>::try_from(value).map_err(|e| {
                        Error::Other(format!(
                            "FromWasmValues: Could not convert WasmValue to expected type: {:?}",
                            e
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
            #[inline]
            fn wasm_types() -> Box<[WasmType]> {
                Box::new([$($T::wasm_type(),)+])
            }
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

                Ok((
                    $(
                        $T::try_from(
                            *iter.next()
                            .ok_or(Error::Other("Not enough values in WasmValue vector".to_string()))?
                        )
                        .map_err(|e| Error::Other(format!(
                            "FromWasmValues: Could not convert WasmValue to expected type: {:?}",
                            e,
                        )))?,
                    )+
                ))
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
    i32 => I32,
    i64 => I64,
    f32 => F32,
    f64 => F64,
    FuncRef => RefFunc,
    ExternRef => RefExtern,
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
    #[inline]
    fn wasm_types() -> Box<[WasmType]> {
        let mut types = Vec::new();
        types.extend_from_slice(&T1::wasm_types());
        types.extend_from_slice(&T2::wasm_types());
        types.into_boxed_slice()
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
    #[inline]
    fn wasm_types() -> Box<[WasmType]> {
        Box::new([])
    }
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

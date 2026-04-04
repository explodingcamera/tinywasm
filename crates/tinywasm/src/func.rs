use crate::interpreter::stack::CallFrame;
use crate::{Error, FuncContext, InterpreterRuntime, Result, Store};
use crate::{Function, unlikely};
use alloc::{boxed::Box, format, string::ToString, vec, vec::Vec};
use tinywasm_types::{ExternRef, FuncRef, FuncType, ModuleInstanceAddr, ValType, WasmValue};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Progress for fuel-limited function execution.
pub enum ExecProgress<T> {
    /// Execution completed and produced a result.
    Completed(T),
    /// Execution suspended after exhausting fuel or time budget.
    Suspended,
}

#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct ExecutionState {
    pub(crate) callframe: CallFrame,
}

/// A function handle
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct FuncHandle {
    pub(crate) module_addr: ModuleInstanceAddr,
    pub(crate) addr: u32,
    pub(crate) ty: FuncType,
}

/// Resumable execution for an untyped function call.
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct FuncExecution<'store> {
    store: &'store mut Store,
    state: FuncExecutionState,
}

#[cfg_attr(feature = "debug", derive(Debug))]
enum FuncExecutionState {
    Running { exec_state: ExecutionState, root_func_addr: u32 },
    Completed { result: Option<Vec<WasmValue>> },
}

/// Resumable execution for a typed function call.
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct FuncExecutionTyped<'store, R> {
    execution: FuncExecution<'store>,
    marker: core::marker::PhantomData<R>,
}

impl FuncHandle {
    /// Call a function (Invocation)
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#invocation>
    #[inline]
    pub fn call(&self, store: &mut Store, params: &[WasmValue]) -> Result<Vec<WasmValue>> {
        validate_call_params(&self.ty, params)?;

        let func_inst = store.state.get_func(self.addr);
        let wasm_func = match &func_inst.func {
            Function::Host(host_func) => {
                return host_func.clone().call(FuncContext { store, module_addr: self.module_addr }, params);
            }
            Function::Wasm(wasm_func) => wasm_func.clone(),
        };

        // Reset stack, push args, allocate locals, create entry frame.
        store.stack.clear();
        store.stack.values.extend_from_wasmvalues(params)?;
        let locals_base = store.stack.values.enter_locals(&wasm_func.params, &wasm_func.locals)?;
        let stack_offset = wasm_func.locals;
        let callframe = CallFrame::new(self.addr, func_inst.owner, locals_base, stack_offset);

        // Execute until completion and then collect result values from the stack.
        InterpreterRuntime::exec(store, callframe)?;

        collect_call_results(store, &self.ty)
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
        validate_call_params(&self.ty, params)?;

        let func_inst = store.state.get_func(self.addr);
        let func_inst_owner = func_inst.owner;
        let func = func_inst.func.clone();

        match func {
            Function::Host(host_func) => {
                let result = host_func.call(FuncContext { store, module_addr: self.module_addr }, params)?;
                Ok(FuncExecution { store, state: FuncExecutionState::Completed { result: Some(result) } })
            }
            Function::Wasm(wasm_func) => {
                store.stack.clear();
                store.stack.values.extend_from_wasmvalues(params)?;
                let locals_base = store.stack.values.enter_locals(&wasm_func.params, &wasm_func.locals)?;
                let stack_offset = wasm_func.locals;
                let callframe = CallFrame::new(self.addr, func_inst_owner, locals_base, stack_offset);

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
                let result_ty = self.store.state.get_func(*root_func_addr).func.ty().clone();
                let result = collect_call_results(self.store, &result_ty)?;
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
                let result_ty = self.store.state.get_func(*root_func_addr).func.ty().clone();
                let result = collect_call_results(self.store, &result_ty)?;
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
    if unlikely(func_ty.params.len() != params.len()) {
        return Err(Error::Other(format!(
            "param count mismatch: expected {}, got {}",
            func_ty.params.len(),
            params.len()
        )));
    }

    if !(func_ty.params.iter().zip(params).all(|(ty, param)| ty == &param.val_type())) {
        return Err(Error::Other("Type mismatch".into()));
    }

    Ok(())
}

fn collect_call_results(store: &mut Store, func_ty: &FuncType) -> Result<Vec<WasmValue>> {
    // m values are on the top of the stack (Ensured by validation)
    debug_assert!(store.stack.values.len() >= func_ty.results.len());
    let mut res: Vec<_> = store.stack.values.pop_types(func_ty.results.iter().rev()).collect(); // pop in reverse order since the stack is LIFO
    res.reverse(); // reverse to get the original order
    Ok(res)
}

/// A typed function handle
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct FuncHandleTyped<P, R> {
    /// The underlying function handle
    pub func: FuncHandle,
    pub(crate) marker: core::marker::PhantomData<(P, R)>,
}

pub trait IntoWasmValueTuple {
    fn into_wasm_value_tuple(self) -> Vec<WasmValue>;
}

pub trait FromWasmValueTuple {
    fn from_wasm_value_tuple(values: &[WasmValue]) -> Result<Self>
    where
        Self: Sized;
}

impl<P: IntoWasmValueTuple, R: FromWasmValueTuple> FuncHandleTyped<P, R> {
    /// Call a typed function
    pub fn call(&self, store: &mut Store, params: P) -> Result<R> {
        // Convert params into Vec<WasmValue>
        let wasm_values = params.into_wasm_value_tuple();

        // Call the underlying WASM function
        let result = self.func.call(store, &wasm_values)?;

        // Convert the Vec<WasmValue> back to R
        R::from_wasm_value_tuple(&result)
    }

    /// Call a typed function and return a resumable execution handle.
    ///
    /// The handle keeps a mutable borrow of the [`Store`] until completion.
    pub fn call_resumable<'store>(&self, store: &'store mut Store, params: P) -> Result<FuncExecutionTyped<'store, R>> {
        let wasm_values = params.into_wasm_value_tuple();
        let execution = self.func.call_resumable(store, &wasm_values)?;
        Ok(FuncExecutionTyped { execution, marker: core::marker::PhantomData })
    }
}

impl<'store, R: FromWasmValueTuple> FuncExecutionTyped<'store, R> {
    /// Resume typed execution with up to `fuel` units of fuel.
    ///
    /// Fuel is accounted in chunks, so execution may overshoot the requested
    /// fuel before returning [`ExecProgress::Suspended`].
    pub fn resume_with_fuel(&mut self, fuel: u32) -> Result<ExecProgress<R>> {
        match self.execution.resume_with_fuel(fuel)? {
            ExecProgress::Completed(values) => Ok(ExecProgress::Completed(R::from_wasm_value_tuple(&values)?)),
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
            ExecProgress::Completed(values) => Ok(ExecProgress::Completed(R::from_wasm_value_tuple(&values)?)),
            ExecProgress::Suspended => Ok(ExecProgress::Suspended),
        }
    }
}

pub trait ValTypesFromTuple {
    fn val_types() -> Box<[ValType]>;
}

pub trait ToValType {
    fn to_val_type() -> ValType;
}

macro_rules! impl_scalar_wasm_traits {
    ($($T:ty => $val_ty:ident),+ $(,)?) => {
        $(
            impl ToValType for $T {
                #[inline]
                fn to_val_type() -> ValType {
                    ValType::$val_ty
                }
            }

            impl ValTypesFromTuple for $T {
                #[inline]
                fn val_types() -> Box<[ValType]> {
                    Box::new([ValType::$val_ty])
                }
            }

            impl IntoWasmValueTuple for $T {
                #[inline]
                fn into_wasm_value_tuple(self) -> Vec<WasmValue> {
                    vec![self.into()]
                }
            }

            impl FromWasmValueTuple for $T {
                #[inline]
                fn from_wasm_value_tuple(values: &[WasmValue]) -> Result<Self> {
                    let value = *values
                        .first()
                        .ok_or(Error::Other("Not enough values in WasmValue vector".to_string()))?;
                    <$T>::try_from(value).map_err(|e| {
                        Error::Other(format!(
                            "FromWasmValueTuple: Could not convert WasmValue to expected type: {:?}",
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
        impl<$($T),+> ValTypesFromTuple for ($($T,)+)
        where
            $($T: ToValType,)+
        {
            #[inline]
            fn val_types() -> Box<[ValType]> {
                Box::new([$($T::to_val_type(),)+])
            }
        }

        impl<$($T),+> IntoWasmValueTuple for ($($T,)+)
        where
            $($T: Into<WasmValue>,)+
        {
            #[allow(non_snake_case)]
            #[inline]
            fn into_wasm_value_tuple(self) -> Vec<WasmValue> {
                let ($($T,)+) = self;
                vec![$($T.into(),)+]
            }
        }

        impl<$($T),+> FromWasmValueTuple for ($($T,)+)
        where
            $($T: TryFrom<WasmValue, Error = ()>,)+
        {
            #[inline]
            fn from_wasm_value_tuple(values: &[WasmValue]) -> Result<Self> {
                let mut iter = values.iter();

                Ok((
                    $(
                        $T::try_from(
                            *iter.next()
                            .ok_or(Error::Other("Not enough values in WasmValue vector".to_string()))?
                        )
                        .map_err(|e| Error::Other(format!(
                            "FromWasmValueTuple: Could not convert WasmValue to expected type: {:?}",
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

impl ValTypesFromTuple for () {
    #[inline]
    fn val_types() -> Box<[ValType]> {
        Box::new([])
    }
}

impl IntoWasmValueTuple for () {
    #[inline]
    fn into_wasm_value_tuple(self) -> Vec<WasmValue> {
        vec![]
    }
}

impl FromWasmValueTuple for () {
    #[inline]
    fn from_wasm_value_tuple(_values: &[WasmValue]) -> Result<Self> {
        Ok(())
    }
}

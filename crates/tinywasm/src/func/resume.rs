use tinywasm_types::{FuncAddr, TypeAddr};

use super::{FromWasmValues, Function, FunctionTyped, IntoWasmValues};
use crate::interpreter::stack::{CallFrame, StackBase};
use crate::{Error, InterpreterRuntime, Result, Store, WasmValue};

#[derive(Clone, PartialEq, Eq)]
/// Progress for fuel-limited function execution.
pub enum ExecProgress<T> {
    /// Execution completed and produced a result.
    Completed(T),
    /// Execution suspended after exhausting fuel or time budget.
    Suspended,
}

/// Resumable execution for an untyped function call.
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
pub struct FuncExecution<'store> {
    execution: ExecutionInner<'store>,
    results: &'store mut [WasmValue],
}

/// Resumable execution for a typed function call.
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
pub struct FuncExecutionTyped<'store, R> {
    execution: ExecutionInner<'store>,
    result: Option<R>,
}

#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
struct ExecutionInner<'store> {
    store: &'store mut Store,
    state: ExecState,
}

#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
enum ExecState {
    Running { callframe: CallFrame, root_func_addr: FuncAddr },
    Completed(Option<CallResult>),
}

#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
enum CallResult {
    Stack { type_addr: TypeAddr },
    Written,
}

impl Function {
    /// Call a function and return a resumable execution handle.
    ///
    /// The returned handle keeps a mutable borrow of the [`Store`] until it
    /// completes. Use [`FuncExecution::resume_with_fuel`] (or
    /// `resume_with_time_budget` with `std`) to continue.
    pub fn call_resumable<'store>(
        &self,
        store: &'store mut Store,
        params: &[WasmValue],
        results: &'store mut [WasmValue],
    ) -> Result<FuncExecution<'store>> {
        let type_addr = self.validate_call(store, params, results.len())?;

        store.enter_execution()?;
        let result: Result<ExecState> = (|| {
            if store.state.funcs.is_host(self.addr()) {
                let func = {
                    let func = store.state.funcs.host(self.addr());
                    func.func.clone()
                };
                func.call_values(store, self.module_id, type_addr, params, results)?;
                return Ok(ExecState::Completed(Some(CallResult::Written)));
            }

            let (wasm_params, wasm_locals) = {
                let wasm = store.state.funcs.wasm(self.addr());
                (wasm.func.params, wasm.func.locals)
            };

            store.call_stack.clear();
            store.value_stack.clear();
            store.push_wasm_values(params)?;
            let locals_base = store.value_stack.enter_locals(&wasm_params, &wasm_locals)?;
            let callframe = CallFrame::new(self.addr(), locals_base, wasm_locals);

            Ok(ExecState::Running { callframe, root_func_addr: self.addr() })
        })();
        if result.is_err() {
            store.call_stack.clear();
            store.value_stack.clear();
        }
        store.exit_execution();

        let state = result?;
        Ok(FuncExecution { execution: ExecutionInner { store, state }, results })
    }
}

impl ExecutionInner<'_> {
    fn resume_raw(
        &mut self,
        run: impl FnOnce(&mut Store, CallFrame) -> Result<crate::interpreter::ExecState>,
    ) -> Result<ExecProgress<CallResult>> {
        let (callframe, root_func_addr) = match &mut self.state {
            ExecState::Running { callframe, root_func_addr } => (*callframe, *root_func_addr),
            ExecState::Completed(result) => {
                return result
                    .take()
                    .map(ExecProgress::Completed)
                    .ok_or_else(|| Error::other("execution already completed"));
            }
        };

        self.store.enter_execution()?;
        let result = run(self.store, callframe);
        self.store.exit_execution();

        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.store.call_stack.clear();
                self.store.value_stack.clear();
                self.state = ExecState::Completed(None);
                return Err(error);
            }
        };

        match result {
            crate::interpreter::ExecState::Completed => {
                let result_ty = self.store.state.funcs.type_addr(root_func_addr);
                self.state = ExecState::Completed(None);
                Ok(ExecProgress::Completed(CallResult::Stack { type_addr: result_ty }))
            }
            crate::interpreter::ExecState::Suspended(callframe) => {
                let ExecState::Running { callframe: current, .. } = &mut self.state else {
                    unreachable!("invalid function execution state")
                };
                *current = callframe;
                Ok(ExecProgress::Suspended)
            }
        }
    }
}

impl FuncExecution<'_> {
    fn resume(
        &mut self,
        run: impl FnOnce(&mut Store, CallFrame) -> Result<crate::interpreter::ExecState>,
    ) -> Result<ExecProgress<()>> {
        match self.execution.resume_raw(run)? {
            ExecProgress::Completed(CallResult::Stack { type_addr }) => {
                self.execution.store.pop_stack_values(type_addr, self.results)?;
                Ok(ExecProgress::Completed(()))
            }
            ExecProgress::Completed(CallResult::Written) => Ok(ExecProgress::Completed(())),
            ExecProgress::Suspended => Ok(ExecProgress::Suspended),
        }
    }

    /// Resume execution with up to `fuel` units of fuel.
    ///
    /// Fuel is accounted in chunks, so execution may overshoot the requested
    /// fuel before returning [`ExecProgress::Suspended`] (currently the chunk size is 128 instructions between fuel checks, but this may change in the future).
    ///
    /// Returns [`ExecProgress::Suspended`] when fuel is exhausted, or
    /// [`ExecProgress::Completed`] after writing the final values to the result
    /// slice supplied to [`Function::call_resumable`].
    ///
    /// Reentrant calls made by host functions through [`crate::FuncContext::call`] are
    /// currently blocking. They do not suspend and later resume the host
    /// function in the middle of the nested call.
    pub fn resume_with_fuel(&mut self, fuel: u32) -> Result<ExecProgress<()>> {
        self.resume(|store, callframe| InterpreterRuntime::exec_with_fuel(store, callframe, fuel))
    }

    #[cfg(feature = "std")]
    /// Resume execution for at most `time_budget` wall-clock time.
    ///
    /// Time is checked periodically, so execution may overshoot the requested
    /// time budget before returning [`ExecProgress::Suspended`] (currently time
    /// is checked every 128 instructions, but this may change in the future).
    ///
    /// Reentrant calls made by host functions through [`crate::FuncContext::call`]
    /// are blocking and do not suspend in the middle of the host callback.
    pub fn resume_with_time_budget(&mut self, time_budget: crate::std::time::Duration) -> Result<ExecProgress<()>> {
        self.resume(|store, callframe| InterpreterRuntime::exec_with_time_budget(store, callframe, time_budget))
    }
}

impl<P: IntoWasmValues, R: FromWasmValues> FunctionTyped<P, R> {
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
    /// let wasm = include_bytes!("../../../../examples/wasm/add.wasm");
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
        self.func.item.validate_store(store)?;
        if store.state.funcs.is_host(self.func.addr()) {
            let result = self.call(store, params)?;
            let execution = ExecutionInner { store, state: ExecState::Completed(None) };
            return Ok(FuncExecutionTyped { execution, result: Some(result) });
        }
        let (type_addr, wasm_params, wasm_locals) = {
            let wasm = store.state.funcs.wasm(self.func.addr());
            (wasm.type_addr, wasm.func.params, wasm.func.locals)
        };

        store.enter_execution()?;
        let result: Result<ExecState> = (|| {
            store.call_stack.clear();
            store.value_stack.clear();
            store.push_typed_values::<false>(type_addr, params.into_wasm_values(), StackBase::default())?;
            let locals_base = store
                .value_stack
                .enter_locals(&wasm_params, &wasm_locals)
                .inspect_err(|_| store.value_stack.clear())?;
            let callframe = CallFrame::new(self.func.addr(), locals_base, wasm_locals);
            Ok(ExecState::Running { callframe, root_func_addr: self.func.addr() })
        })();
        store.exit_execution();
        let execution = ExecutionInner { store, state: result? };
        Ok(FuncExecutionTyped { execution, result: None })
    }
}

impl<'store, R: FromWasmValues> FuncExecutionTyped<'store, R> {
    fn resume(
        &mut self,
        run: impl FnOnce(&mut Store, CallFrame) -> Result<crate::interpreter::ExecState>,
    ) -> Result<ExecProgress<R>> {
        if let Some(result) = self.result.take() {
            return Ok(ExecProgress::Completed(result));
        }
        match self.execution.resume_raw(run)? {
            ExecProgress::Completed(CallResult::Stack { type_addr }) => {
                Ok(ExecProgress::Completed(self.execution.store.take_typed_results(type_addr, StackBase::default())?))
            }
            ExecProgress::Completed(CallResult::Written) => unreachable!("untyped result in typed execution"),
            ExecProgress::Suspended => Ok(ExecProgress::Suspended),
        }
    }

    /// Resume typed execution with up to `fuel` units of fuel.
    pub fn resume_with_fuel(&mut self, fuel: u32) -> Result<ExecProgress<R>> {
        self.resume(|store, callframe| InterpreterRuntime::exec_with_fuel(store, callframe, fuel))
    }

    #[cfg(feature = "std")]
    /// Resume typed execution for at most `time_budget` wall-clock time.
    ///
    /// Time is checked periodically, so execution may overshoot the requested
    /// time budget before returning [`ExecProgress::Suspended`].
    pub fn resume_with_time_budget(&mut self, time_budget: crate::std::time::Duration) -> Result<ExecProgress<R>> {
        self.resume(|store, callframe| InterpreterRuntime::exec_with_time_budget(store, callframe, time_budget))
    }
}

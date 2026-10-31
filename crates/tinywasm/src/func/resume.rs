use tinywasm_types::{FuncAddr, TypeAddr};

use super::{FromWasmValues, Function, FunctionTyped, IntoWasmValues};
use crate::interpreter::stack::{CallFrame, StackBase};
use crate::{Error, InterpreterRuntime, Result, Store, WasmValue};

#[derive(Clone, PartialEq, Eq)]
/// Progress for fuel-limited function execution.
pub enum ExecProgress<T> {
    /// Execution completed and produced a result.
    Completed(T),
    /// Execution suspended after exhausting its fuel budget.
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

impl Drop for ExecutionInner<'_> {
    fn drop(&mut self) {
        // Release frames and GC roots left by a suspended or unwound execution.
        self.store.call_stack.clear();
        self.store.value_stack.clear();
    }
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
    /// completes. Use [`FuncExecution::resume_with_fuel`] to continue.
    pub fn call_resumable<'store>(
        &self,
        store: &'store mut Store,
        params: &[WasmValue],
        results: &'store mut [WasmValue],
    ) -> Result<FuncExecution<'store>> {
        let type_addr = self.validate_call(store, params, results.len())?;

        let result: Result<ExecState> = {
            let mut store = store.enter_execution()?;
            let result = (|| {
                if store.code.funcs.is_host(self.addr()) {
                    let func = store.code.funcs.host(self.addr()).func.clone();
                    func.call_values(&mut store, self.module_id, type_addr, params, results)?;
                    return Ok(ExecState::Completed(Some(CallResult::Written)));
                }

                let (wasm_params, wasm_locals, wasm_max_stack) = {
                    let wasm = store.code.funcs.wasm(self.addr());
                    (wasm.func.params, wasm.func.locals, wasm.func.max_stack)
                };

                store.call_stack.clear();
                store.value_stack.clear();
                store.push_wasm_values(params)?;
                let locals_base = store.value_stack.enter_locals(&wasm_params, &wasm_locals, &wasm_max_stack)?;
                let callframe = CallFrame::new(self.addr(), locals_base, wasm_locals);

                Ok(ExecState::Running { callframe, root_func_addr: self.addr() })
            })();
            if result.is_err() {
                store.call_stack.clear();
                store.value_stack.clear();
            }
            result
        };

        let state = result?;
        Ok(FuncExecution { execution: ExecutionInner { store, state }, results })
    }
}

impl ExecutionInner<'_> {
    fn resume(&mut self, fuel: u32) -> Result<ExecProgress<CallResult>> {
        let (callframe, root_func_addr) = match &mut self.state {
            ExecState::Running { callframe, root_func_addr } => (*callframe, *root_func_addr),
            ExecState::Completed(result) => {
                return result
                    .take()
                    .map(ExecProgress::Completed)
                    .ok_or_else(|| Error::other("execution already completed"));
            }
        };

        let result = {
            let mut store = self.store.enter_execution()?;
            // A panic leaves partially executed stacks that the saved frame no longer describes.
            self.state = ExecState::Completed(None);
            InterpreterRuntime::exec_with_fuel(&mut store, callframe, fuel)
        };

        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.store.call_stack.clear();
                self.store.value_stack.clear();
                return Err(error);
            }
        };

        match result {
            crate::interpreter::ExecState::Completed => {
                let result_ty = self.store.code.funcs.type_addr(root_func_addr);
                Ok(ExecProgress::Completed(CallResult::Stack { type_addr: result_ty }))
            }
            crate::interpreter::ExecState::Suspended(callframe) => {
                self.state = ExecState::Running { callframe, root_func_addr };
                Ok(ExecProgress::Suspended)
            }
        }
    }
}

impl FuncExecution<'_> {
    /// Resume execution with the given fuel budget.
    ///
    /// Fuel is checked at taken branches, calls, returns, throws, and synthetic checkpoints, so
    /// execution may overshoot the requested fuel. By default, checkpoints target 1,024 emitted
    /// interpreter instructions, with up to three extra instructions for pending fusion.
    /// Synthetic checkpoints cost no fuel. Taken loop backedges are checked even without them.
    /// Completion and traps take precedence over suspension after their instruction executes.
    /// A synchronous host callback finishes before suspension at its call boundary.
    /// Set checkpoint spacing with `ParserOptions::with_checkpoint_interval` when parsing.
    ///
    /// Returns [`ExecProgress::Suspended`] when fuel is exhausted, or
    /// [`ExecProgress::Completed`] after writing the final values to the result
    /// slice supplied to [`Function::call_resumable`].
    ///
    /// Reentrant calls made by host functions through [`crate::FuncContext::call`] are
    /// currently blocking. They do not suspend and later resume the host
    /// function in the middle of the nested call.
    pub fn resume_with_fuel(&mut self, fuel: u32) -> Result<ExecProgress<()>> {
        match self.execution.resume(fuel)? {
            ExecProgress::Completed(CallResult::Stack { type_addr }) => {
                self.execution.store.pop_stack_values(type_addr, self.results)?;
                Ok(ExecProgress::Completed(()))
            }
            ExecProgress::Completed(CallResult::Written) => Ok(ExecProgress::Completed(())),
            ExecProgress::Suspended => Ok(ExecProgress::Suspended),
        }
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
        if store.code.funcs.is_host(self.func.addr()) {
            let result = self.call(store, params)?;
            let execution = ExecutionInner { store, state: ExecState::Completed(None) };
            return Ok(FuncExecutionTyped { execution, result: Some(result) });
        }
        let (type_addr, wasm_params, wasm_locals, wasm_max_stack) = {
            let wasm = store.code.funcs.wasm(self.func.addr());
            (wasm.type_addr, wasm.func.params, wasm.func.locals, wasm.func.max_stack)
        };

        let state = {
            let mut store = store.enter_execution()?;
            store.call_stack.clear();
            store.value_stack.clear();
            store.push_typed_values::<false>(type_addr, params.into_wasm_values(), StackBase::default())?;
            let locals_base = store
                .value_stack
                .enter_locals(&wasm_params, &wasm_locals, &wasm_max_stack)
                .inspect_err(|_| store.value_stack.clear())?;
            let callframe = CallFrame::new(self.func.addr(), locals_base, wasm_locals);
            ExecState::Running { callframe, root_func_addr: self.func.addr() }
        };
        let execution = ExecutionInner { store, state };
        Ok(FuncExecutionTyped { execution, result: None })
    }
}

impl<'store, R: FromWasmValues> FuncExecutionTyped<'store, R> {
    /// Resume typed execution with the given fuel budget.
    ///
    /// Uses the same boundary checks and suspension rules as [`FuncExecution::resume_with_fuel`].
    pub fn resume_with_fuel(&mut self, fuel: u32) -> Result<ExecProgress<R>> {
        if let Some(result) = self.result.take() {
            return Ok(ExecProgress::Completed(result));
        }
        match self.execution.resume(fuel)? {
            ExecProgress::Completed(CallResult::Stack { type_addr }) => {
                Ok(ExecProgress::Completed(self.execution.store.take_typed_results(type_addr, StackBase::default())?))
            }
            ExecProgress::Completed(CallResult::Written) => unreachable!("untyped result in typed execution"),
            ExecProgress::Suspended => Ok(ExecProgress::Suspended),
        }
    }
}

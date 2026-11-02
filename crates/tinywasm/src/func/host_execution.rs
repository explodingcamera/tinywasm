use alloc::{boxed::Box, vec, vec::Vec};

use super::{FuncContext, Function};
use crate::interpreter::executor::{Budget, ExecOutcome, Executor, HostCall};
use crate::interpreter::stack::CallFrame;
use crate::store::FuncValueTypes;
use crate::{Error, Result, Store, Trap, WasmValue, types::TypeAddr};

/// Progress of an invocation whose host calls are handled by the embedder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostExecProgress {
    /// The invocation wrote its final results to the supplied result slice.
    Completed,
    /// Execution exhausted its fuel at an interpreter boundary.
    Suspended,
    /// A host call is waiting for [`HostExecution::host_call`].
    HostCall,
}

/// An invocation that stops before invoking each host callback.
///
/// The embedder can invoke a synchronous callback or supply results from an external future.
/// Futures, wakeups, and cancellation of external work belong to the embedder. Dropping this
/// handle cancels the Wasm continuation and releases its stack and pending argument roots.
/// Only one invocation can borrow a store at a time; this is not a component task scheduler.
pub struct HostExecution<'store> {
    store: &'store mut Store,
    results: &'store mut [WasmValue],
    root_type: TypeAddr,
    state: State,
}

enum State {
    Ready(CallFrame),
    ReturnFromHost(HostCall),
    Pending { function: Function, params: Vec<WasmValue>, continuation: Option<HostCall> },
    Completed,
    Finished,
}

/// Access to the current host call, with its arguments rooted in the owning store.
///
/// Dropping this view leaves the call pending. Clone arguments needed by external work, drop
/// the view before waiting, and obtain it again when the results are ready. The callback is
/// never replayed by [`HostExecution::resume_with_fuel`].
pub struct PendingHostCall<'execution, 'store> {
    execution: &'execution mut HostExecution<'store>,
}

impl Function {
    /// Starts an invocation that yields to the embedder before every host callback.
    ///
    /// Unlike [`Self::call_resumable`], this also defers a host function used as the root call.
    /// Use [`PendingHostCall::invoke`] for synchronous callbacks and
    /// [`PendingHostCall::complete`] for results produced outside the interpreter.
    /// This API uses the same fuel boundaries as [`super::FuncExecution::resume_with_fuel`].
    pub fn call_resumable_with_host_calls<'store>(
        &self,
        store: &'store mut Store,
        params: &[WasmValue],
        results: &'store mut [WasmValue],
    ) -> Result<HostExecution<'store>> {
        let root_type = self.validate_call(store, params, results.len())?;
        let state = {
            let mut store = store.enter_execution()?;
            store.call_stack.clear();
            store.value_stack.clear();
            (|| -> Result<State> {
                if store.code.funcs.is_host(self.addr()) {
                    return Ok(State::Pending { function: self.clone(), params: params.to_vec(), continuation: None });
                }
                let wasm = store.code.funcs.wasm(self.addr());
                let (params_count, locals, max_stack) = (wasm.func.params, wasm.func.locals, wasm.func.max_stack);
                store.push_wasm_values(params)?;
                let base = store.value_stack.enter_locals(&params_count, &locals, &max_stack)?;
                Ok(State::Ready(CallFrame::new(self.addr(), base, locals)))
            })()
            .inspect_err(|_| store.value_stack.clear())?
        };
        Ok(HostExecution { store, results, root_type, state })
    }
}

impl<'store> HostExecution<'store> {
    /// Runs until completion, fuel exhaustion, or the next host call.
    ///
    /// A pending host call is returned again without executing guest code or invoking a callback.
    /// Completing the host call commits its results; a later resume continues after that call.
    /// A trap or panic makes the invocation terminal, as with ordinary resumable execution.
    pub fn resume_with_fuel(&mut self, fuel: u32) -> Result<HostExecProgress> {
        match self.state {
            State::Pending { .. } => return Ok(HostExecProgress::HostCall),
            State::Completed => {
                self.state = State::Finished;
                return Ok(HostExecProgress::Completed);
            }
            State::Finished => return Err(Error::other("execution already completed")),
            _ => {}
        }
        let state = core::mem::replace(&mut self.state, State::Finished);
        let outcome = {
            let mut store = self.store.enter_execution()?;
            store.execution_fuel = fuel;
            match state {
                State::Ready(frame) => Executor::new(&mut store, frame, 0, Budget::Fuel).run(),
                State::ReturnFromHost(call) => {
                    Executor::new(&mut store, call.caller, 0, Budget::Fuel).return_from_host()
                }
                _ => unreachable!(),
            }
        };
        self.accept_outcome(outcome).inspect_err(|_| self.finish())
    }

    fn accept_outcome(&mut self, outcome: Result<ExecOutcome>) -> Result<HostExecProgress> {
        match outcome? {
            ExecOutcome::Complete => {
                self.store.pop_stack_values(self.root_type, self.results)?;
                Ok(HostExecProgress::Completed)
            }
            ExecOutcome::Suspended(frame) => {
                self.state = State::Ready(frame);
                Ok(HostExecProgress::Suspended)
            }
            ExecOutcome::Host(call) => {
                let type_addr = self.store.code.funcs.type_addr(call.func);
                let base =
                    self.store.value_stack.base_before(self.store.get_func_type(call.func).params().iter().collect());
                let params = self.store.stack_value_iter(type_addr, FuncValueTypes::Params, base)?.collect();
                self.store.value_stack.truncate_to_base(base);
                let function =
                    Function { item: crate::StoreItem::new(self.store.id(), call.func), module_id: call.module };
                self.state = State::Pending { function, params, continuation: Some(call) };
                Ok(HostExecProgress::HostCall)
            }
            ExecOutcome::Exception(reference) => Err(Error::Exception(self.store.root_exception(reference)?)),
        }
    }

    /// Returns the pending host call, if execution has reached one.
    pub fn host_call(&mut self) -> Option<PendingHostCall<'_, 'store>> {
        matches!(self.state, State::Pending { .. }).then_some(PendingHostCall { execution: self })
    }

    fn finish(&mut self) {
        self.state = State::Finished;
        self.store.call_stack.clear();
        self.store.value_stack.clear();
    }
}

impl Drop for HostExecution<'_> {
    fn drop(&mut self) {
        self.finish();
    }
}

impl PendingHostCall<'_, '_> {
    /// The resolved host function, including the importing module's context.
    pub fn function(&self) -> &Function {
        let State::Pending { function, .. } = &self.execution.state else { unreachable!() };
        function
    }

    /// Arguments in WebAssembly signature order. Managed references remain rooted while pending.
    pub fn params(&self) -> &[WasmValue] {
        let State::Pending { params, .. } = &self.execution.state else { unreachable!() };
        params
    }

    /// Read-only access to the owning store, for inspecting types and store-checked handles.
    pub fn store(&self) -> &Store {
        self.execution.store
    }

    /// Accesses host state, guest memory, and blocking nested calls while this call is pending.
    ///
    /// The context cannot outlive this callback. Use [`FuncContext::call`] for nested calls,
    /// including canonical ABI realloc and post-return functions. Nested calls remain synchronous.
    /// A panic in the callback terminates the invocation.
    pub fn with_context<R>(&mut self, callback: impl FnOnce(FuncContext<'_>) -> Result<R>) -> Result<R> {
        let module_id = self.function().module_id;
        let pending = core::mem::replace(&mut self.execution.state, State::Finished);
        let result = {
            let mut store = self.execution.store.enter_execution()?;
            callback(FuncContext { store: &mut store, module_id })
        };
        self.execution.state = pending;
        result
    }

    /// Commits externally produced results without replaying the host callback.
    ///
    /// Invalid result counts, types, and foreign-store references leave the call pending so
    /// the embedder can correct them. A failure to grow the result stack terminates execution.
    pub fn complete(self, results: &[WasmValue]) -> Result<()> {
        let function = self.function();
        let store = &*self.execution.store;
        for value in results {
            if matches!(value, WasmValue::Ref(_)) {
                value.to_runtime(store)?;
            }
        }
        let expected = function.ty(store)?;
        if results.len() != expected.results().len()
            || !results.iter().zip(expected.results()).all(|(value, &ty)| store.value_matches_type(value, ty))
        {
            return Err(Error::InvalidHostFnReturn { expected: Box::new(expected.clone()), actual: results.to_vec() });
        }
        let State::Pending { continuation, .. } = &self.execution.state else { unreachable!() };
        if let Some(call) = *continuation {
            if let Err(error) = self.execution.store.push_wasm_values(results) {
                self.execution.finish();
                return Err(error);
            }
            self.execution.state = if call.tail { State::ReturnFromHost(call) } else { State::Ready(call.caller) };
        } else {
            self.execution.results.clone_from_slice(results);
            self.execution.state = State::Completed;
        }
        Ok(())
    }

    /// Invokes the registered synchronous callback once, then commits its results.
    ///
    /// An error or panic terminates this invocation. Errors use the same host-trap wrapping as
    /// synchronous calls from Wasm; root host calls return the callback's error directly.
    pub fn invoke(self) -> Result<()> {
        let State::Pending { function, continuation, .. } = &self.execution.state else { unreachable!() };
        let (addr, module_id, continuation) = (function.addr(), function.module_id, *continuation);
        let type_addr = self.execution.store.code.funcs.type_addr(addr);
        let callback = self.execution.store.code.funcs.host(addr).func.clone();
        let mut results = vec![WasmValue::I32(0); self.execution.store.get_func_type(addr).results().len()];
        // Keep owned arguments alive while a callback runs, even if it panics or collects.
        let State::Pending { params, .. } = core::mem::replace(&mut self.execution.state, State::Finished) else {
            unreachable!()
        };
        let result = {
            let mut store = self.execution.store.enter_execution()?;
            callback.call_values(&mut store, module_id, type_addr, &params, &mut results)
        };
        if let Err(error) = result {
            self.execution.finish();
            return Err(if continuation.is_some() { Error::Trap(Trap::HostFunction(Box::new(error))) } else { error });
        }
        // Complete through the same checked result path used by external callbacks.
        let function = Function { item: crate::StoreItem::new(self.execution.store.id(), addr), module_id };
        self.execution.state = State::Pending { function, params, continuation };
        self.complete(&results)
    }
}

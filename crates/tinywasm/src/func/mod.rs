use crate::interpreter::stack::{CallFrame, StackBase};
use crate::reference::StoreItem;
use crate::{Error, InterpreterRuntime, Result, Store};
use alloc::format;
use core::hint::cold_path;
use tinywasm_types::{FuncAddr, FuncType, ModuleInstanceId};

use crate::{FuncRef, WasmValue};

mod context;
mod host;
mod resume;
mod values;
pub use context::FuncContext;
pub use host::HostFunction;
#[doc(hidden)]
pub use host::HostFunctionCallback;
pub use resume::{ExecProgress, FuncExecution, FuncExecutionTyped};
pub use values::{FromWasmValues, IntoWasmValues, WasmTypes, WasmValueType};

fn write_typed_params(params: &mut [WasmValue], mut values: impl Iterator<Item = WasmValue>) -> Result<()> {
    for param in params {
        *param = values.next().ok_or_else(|| Error::other("not enough typed function parameters"))?;
    }
    if values.next().is_some() {
        return Err(Error::other("too many typed function parameters"));
    }
    Ok(())
}

/// A handle to a function instance in a store.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#function-instances>
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
pub struct Function {
    pub(crate) item: StoreItem,
    pub(crate) module_id: ModuleInstanceId,
}

impl Function {
    /// Returns this function as a Store-aware WebAssembly function reference.
    pub fn as_func_ref(&self, store: &Store) -> Result<FuncRef> {
        self.item.validate_store(store)?;
        Ok(FuncRef::new(store.id(), self.addr()))
    }

    #[inline]
    /// Returns the function's address in its store.
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

    /// Calls the function and writes its results to `results`.
    ///
    /// The initial result values are ignored. The result slice length must
    /// match the function signature.
    #[inline]
    pub fn call(&self, store: &mut Store, params: &[WasmValue], results: &mut [WasmValue]) -> Result<()> {
        let type_addr = self.validate_call(store, params, results.len())?;

        store.enter_execution()?;
        store.call_stack.clear();
        store.value_stack.clear();
        let result = self.call_untyped(store, type_addr, params, results, 0, StackBase::default());
        store.exit_execution();
        result
    }

    fn validate_call(
        &self,
        store: &Store,
        params: &[WasmValue],
        result_count: usize,
    ) -> Result<tinywasm_types::TypeAddr> {
        self.item.validate_store(store)?;
        let type_addr = store.state.funcs.type_addr(self.addr());
        self.validate_call_type(store, type_addr, params, result_count)?;
        Ok(type_addr)
    }

    fn validate_call_type(
        &self,
        store: &Store,
        type_addr: tinywasm_types::TypeAddr,
        params: &[WasmValue],
        result_count: usize,
    ) -> Result<()> {
        let func_ty = store.state.get_canonical_func_type(type_addr);
        if func_ty.params().len() != params.len() {
            cold_path();
            return Err(Error::Other(format!(
                "param count mismatch: expected {}, got {}",
                func_ty.params().len(),
                params.len()
            )));
        }

        for param in params {
            if matches!(param, WasmValue::Ref(_)) {
                param.to_runtime(store)?;
            }
        }

        if !func_ty.params().iter().zip(params).all(|(ty, param)| store.value_matches_type(param, *ty)) {
            cold_path();
            #[cfg(feature = "debug")]
            return Err(Error::Other(format!(
                "param type mismatch: expected {:?}, got {:?}",
                func_ty.params(),
                params.iter().map(|v| v.ty()).collect::<alloc::vec::Vec<_>>()
            )));
            #[cfg(not(feature = "debug"))]
            return Err(Error::Other("param type mismatch".into()));
        }
        if func_ty.results().len() != result_count {
            return Err(Error::Other(format!(
                "result count mismatch: expected {}, got {result_count}",
                func_ty.results().len()
            )));
        }
        Ok(())
    }

    #[inline]
    fn call_untyped(
        &self,
        store: &mut Store,
        type_addr: tinywasm_types::TypeAddr,
        params: &[WasmValue],
        results: &mut [WasmValue],
        call_stack_base: u32,
        value_stack_base: StackBase,
    ) -> Result<()> {
        if store.state.funcs.is_host(self.addr()) {
            let host = store.state.funcs.host(self.addr()).func.clone();
            host.call_values(store, self.module_id, type_addr, params, results)
        } else {
            let (wasm_params, wasm_locals) = {
                let wasm = store.state.funcs.wasm(self.addr());
                let wasm_params = wasm.func.params;
                let wasm_locals = wasm.func.locals;
                (wasm_params, wasm_locals)
            };
            store.push_wasm_values(params).inspect_err(|_| store.value_stack.truncate_to_base(value_stack_base))?;
            let locals_base = store
                .value_stack
                .enter_locals(&wasm_params, &wasm_locals)
                .inspect_err(|_| store.value_stack.truncate_to_base(value_stack_base))?;
            let callframe = CallFrame::new(self.addr(), locals_base, wasm_locals);
            InterpreterRuntime::exec(store, callframe, call_stack_base).inspect_err(|_| {
                store.call_stack.truncate_to(call_stack_base);
                store.value_stack.truncate_to_base(value_stack_base);
            })?;
            store.pop_stack_values(type_addr, results)
        }
    }

    #[inline]
    fn call_typed<R: FromWasmValues>(
        &self,
        store: &mut Store,
        params: impl Iterator<Item = WasmValue>,
        call_stack_base: u32,
        value_stack_base: StackBase,
    ) -> Result<R> {
        if store.state.funcs.is_host(self.addr()) {
            let (type_addr, host) = {
                let host = store.state.funcs.host(self.addr());
                (host.type_addr, host.func.clone())
            };
            return match host.typed_callback() {
                Some(callback) => {
                    store.push_typed_values::<false>(type_addr, params, value_stack_base)?;
                    callback
                        .call_stack(store, self.module_id, type_addr)
                        .inspect_err(|_| store.value_stack.truncate_to_base(value_stack_base))?;
                    store.take_typed_results(type_addr, value_stack_base)
                }
                None => {
                    let ty = store.state.get_canonical_func_type(type_addr);
                    let (param_count, result_count) = (ty.params().len(), ty.results().len());
                    store.with_scratch_values(param_count + result_count, |store, values| {
                        write_typed_params(&mut values[..param_count], params)?;
                        let (params, results) = values.split_at_mut(param_count);
                        self.validate_call_type(store, type_addr, params, results.len())?;
                        host.call_values(store, self.module_id, type_addr, params, results)?;
                        values.drain(..param_count);
                        R::from_wasm_values(&mut values.drain(..))
                    })
                }
            };
        }

        let (type_addr, wasm_params, wasm_locals) = {
            let wasm = store.state.funcs.wasm(self.addr());
            (wasm.type_addr, wasm.func.params, wasm.func.locals)
        };
        store.push_typed_values::<false>(type_addr, params, value_stack_base)?;
        let locals_base = store
            .value_stack
            .enter_locals(&wasm_params, &wasm_locals)
            .inspect_err(|_| store.value_stack.truncate_to_base(value_stack_base))?;
        let callframe = CallFrame::new(self.addr(), locals_base, wasm_locals);
        InterpreterRuntime::exec(store, callframe, call_stack_base).inspect_err(|_| {
            store.call_stack.truncate_to(call_stack_base);
            store.value_stack.truncate_to_base(value_stack_base);
        })?;
        store.take_typed_results(type_addr, value_stack_base)
    }
}

/// A typed function handle.
///
/// Parameter and result tuples are supported up to arity 20. Use
/// [`crate::ModuleInstance::func_untyped`] for larger signatures.
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
pub struct FunctionTyped<P, R> {
    /// The underlying function handle
    pub func: Function,
    pub(crate) marker: core::marker::PhantomData<(P, R)>,
}

impl<P: IntoWasmValues, R: FromWasmValues> FunctionTyped<P, R> {
    /// Call a typed function
    pub fn call(&self, store: &mut Store, params: P) -> Result<R> {
        self.func.item.validate_store(store)?;
        store.enter_execution()?;
        store.call_stack.clear();
        store.value_stack.clear();
        let result = self.func.call_typed(store, params.into_wasm_values(), 0, StackBase::default());
        store.exit_execution();
        result
    }
}

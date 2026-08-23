use crate::interpreter::stack::{CallFrame, StackBase};
use crate::reference::StoreItem;
use crate::{Error, FunctionInstance, InterpreterRuntime, Result, Store};
use alloc::{format, vec::Vec};
use core::hint::cold_path;
use tinywasm_types::{FuncAddr, FuncType, ModuleInstanceId};

use crate::{FuncRef, WasmValue};

mod context;
mod host;
mod resume;
mod values;
pub use context::FuncContext;
pub use host::HostFunction;
pub use resume::{ExecProgress, FuncExecution, FuncExecutionTyped};
pub use values::{FromWasmValues, IntoWasmValues, ToWasmType, ToWasmTypes};

/// A function handle
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
        Ok(FuncRef::new(store.store_id(), self.addr()))
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

    /// Call a function (Invocation)
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#invocation>
    #[inline]
    pub fn call(&self, store: &mut Store, params: &[WasmValue]) -> Result<Vec<WasmValue>> {
        self.item.validate_store(store)?;
        self.validate_params(store, params)?;

        store.enter_execution()?;
        store.call_stack.clear();
        store.value_stack.clear();
        let result = self.call_untyped(store, params, 0, StackBase::default());
        store.exit_execution();
        result
    }

    fn validate_params(&self, store: &Store, params: &[WasmValue]) -> Result<()> {
        let func_ty = store.state.get_func_type(self.addr());
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
                params.iter().map(|v| v.ty()).collect::<Vec<_>>()
            )));
            #[cfg(not(feature = "debug"))]
            return Err(Error::Other("param type mismatch".into()));
        }
        Ok(())
    }

    #[inline]
    fn call_untyped(
        &self,
        store: &mut Store,
        params: &[WasmValue],
        call_stack_base: u32,
        value_stack_base: StackBase,
    ) -> Result<Vec<WasmValue>> {
        let instance = store.state.get_func(self.addr());
        let type_addr = instance.type_addr;
        match &instance.kind {
            crate::store::FunctionKind::Host(host) => {
                let host = host.clone();
                host.call_values(store, self.module_id, type_addr, params)
            }
            crate::store::FunctionKind::Wasm(wasm) => {
                let wasm_params = wasm.func.params;
                let wasm_locals = wasm.func.locals;
                store
                    .push_wasm_values(params.iter().cloned())
                    .inspect_err(|_| store.value_stack.truncate_to_base(value_stack_base))?;
                let locals_base = store
                    .value_stack
                    .enter_locals(&wasm_params, &wasm_locals)
                    .inspect_err(|_| store.value_stack.truncate_to_base(value_stack_base))?;
                let callframe = CallFrame::new(self.addr(), locals_base, wasm_locals);
                InterpreterRuntime::exec(store, callframe, call_stack_base).inspect_err(|_| {
                    store.call_stack.truncate_to(call_stack_base);
                    store.value_stack.truncate_to_base(value_stack_base);
                })?;
                let result_type = store.state.get_canonical_func_type(type_addr).clone();
                store.pop_stack_values(result_type.results())
            }
        }
    }

    fn prepare_typed(
        &self,
        store: &mut Store,
        instance: &FunctionInstance,
        params: impl Iterator<Item = WasmValue>,
        stack_base: StackBase,
    ) -> Result<Option<CallFrame>> {
        store.push_typed_values::<false>(instance.type_addr, params, stack_base)?;
        match &instance.kind {
            crate::store::FunctionKind::Wasm(wasm) => {
                let locals_base = store
                    .value_stack
                    .enter_locals(&wasm.func.params, &wasm.func.locals)
                    .inspect_err(|_| store.value_stack.truncate_to_base(stack_base))?;
                Ok(Some(CallFrame::new(self.addr(), locals_base, wasm.func.locals)))
            }
            crate::store::FunctionKind::Host(host) => {
                host.typed_callback()
                    .expect("typed host function")
                    .call_stack(store, self.module_id, instance.type_addr)
                    .inspect_err(|_| store.value_stack.truncate_to_base(stack_base))?;
                Ok(None)
            }
        }
    }

    #[inline]
    fn call_typed<R: FromWasmValues>(
        &self,
        store: &mut Store,
        instance: &FunctionInstance,
        params: impl Iterator<Item = WasmValue>,
        call_stack_base: u32,
        value_stack_base: StackBase,
    ) -> Result<R> {
        if let Some(callframe) = self.prepare_typed(store, instance, params, value_stack_base)? {
            InterpreterRuntime::exec(store, callframe, call_stack_base).inspect_err(|_| {
                store.call_stack.truncate_to(call_stack_base);
                store.value_stack.truncate_to_base(value_stack_base);
            })?;
        }
        store.take_typed_results(instance.type_addr, value_stack_base)
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
        let func = store.state.get_func(self.func.addr()).clone();
        if matches!(&func.kind, crate::store::FunctionKind::Host(host) if host.typed_callback().is_none()) {
            let params = params.into_wasm_values().collect::<Vec<_>>();
            let result = self.func.call(store, &params)?;
            let mut values = result.into_iter();
            return R::from_wasm_values_exact(&mut values);
        }

        store.enter_execution()?;
        let result: Result<R> = {
            store.call_stack.clear();
            store.value_stack.clear();
            self.func.call_typed(store, &func, params.into_wasm_values(), 0, StackBase::default())
        };
        store.exit_execution();
        result
    }
}

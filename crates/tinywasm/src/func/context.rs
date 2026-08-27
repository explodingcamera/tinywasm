use tinywasm_types::ModuleInstanceId;

use crate::{Error, FromWasmValues, FuncRef, Function, FunctionTyped, IntoWasmValues, Result, WasmValue};

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
    pub fn module(&self) -> &crate::ModuleInstance {
        unwrap_or_unreachable!(
            self.store.get_module_instance(self.module_id),
            "invalid module instance id in host function context"
        )
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
    pub fn global_get(&mut self, name: &str) -> Result<WasmValue> {
        self.module().clone().global_get(self.store, name)
    }

    /// Get a global export.
    pub fn global(&self, name: &str) -> Result<crate::Global> {
        self.module().global(name)
    }

    /// Set the value of a mutable global export.
    pub fn global_set(&mut self, name: &str, value: WasmValue) -> Result<()> {
        self.module().clone().global_set(self.store, name, value)
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
    pub fn call_untyped(&mut self, func: &Function, args: &[WasmValue], results: &mut [WasmValue]) -> Result<()> {
        if !self.store.execution_active {
            return Err(Error::other("FuncContext::call requires an active host-function invocation"));
        }

        func.validate_call(self.store, args, results.len())?;

        let call_stack_base = self.store.call_stack.len();
        let value_stack_base = self.store.value_stack.base();
        func.call_untyped(self.store, args, results, call_stack_base, value_stack_base)
    }

    /// Calls a Store-aware function reference in the current module context.
    pub fn call_ref(&mut self, func: FuncRef, args: &[WasmValue], results: &mut [WasmValue]) -> Result<()> {
        let addr = func.addr(self.store.id()).ok_or(crate::Trap::InvalidStore)?;
        if self.store.state.funcs.get(addr as usize).is_none() {
            return Err(crate::Trap::InvalidReference.into());
        }
        let function = Function { item: crate::StoreItem::new(self.store.id(), addr), module_id: self.module_id };
        self.call_untyped(&function, args, results)
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
        if !self.store.execution_active {
            return Err(Error::other("FuncContext::call requires an active host-function invocation"));
        }
        func.func.item.validate_store(self.store)?;
        let func_instance = self.store.state.get_func(func.func.addr()).clone();
        if matches!(&func_instance.inner, crate::store::FunctionInstanceInner::Host(host) if host.typed_callback().is_none())
        {
            let ty = self.store.state.get_canonical_func_type(func_instance.type_addr);
            let (param_count, result_count) = (ty.params().len(), ty.results().len());
            self.store.with_scratch_values(param_count + result_count, |store, values| {
                super::write_typed_params(&mut values[..param_count], params.into_wasm_values())?;
                let (params, results) = values.split_at_mut(param_count);
                func.func.validate_call(store, params, results.len())?;
                let call_stack_base = store.call_stack.len();
                let value_stack_base = store.value_stack.base();
                func.func.call_untyped(store, params, results, call_stack_base, value_stack_base)?;
                values.drain(..param_count);
                R::from_wasm_values(&mut values.drain(..))
            })
        } else {
            let call_stack_base = self.store.call_stack.len();
            let value_stack_base = self.store.value_stack.base();
            func.func.call_typed(
                self.store,
                &func_instance,
                params.into_wasm_values(),
                call_stack_base,
                value_stack_base,
            )
        }
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

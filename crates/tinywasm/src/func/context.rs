use alloc::vec::Vec;
use tinywasm_types::{ModuleInstanceId, WasmValue};

use crate::{Error, FromWasmValues, Function, FunctionTyped, IntoWasmValues, Result};

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
            .clone()
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
        func.validate_params(self.store, args)?;

        let call_stack_base = self.store.call_stack.len();
        let value_stack_base = self.store.value_stack.base();
        func.call_untyped(self.store, args, call_stack_base, value_stack_base)
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
        if matches!(&func_instance.kind, crate::store::FunctionKind::Host(host) if host.typed_callback().is_none()) {
            let params = params.into_wasm_values().collect::<Vec<_>>();
            let results = self.call_untyped(&func.func, &params)?;
            let mut values = results.into_iter();
            let result = R::from_wasm_values(&mut values)?;
            return if values.next().is_none() {
                Ok(result)
            } else {
                Err(Error::other("typed conversion did not consume all WebAssembly values"))
            };
        }

        let call_stack_base = self.store.call_stack.len();
        let value_stack_base = self.store.value_stack.base();
        func.func.call_typed(self.store, &func_instance, params.into_wasm_values(), call_stack_base, value_stack_base)
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

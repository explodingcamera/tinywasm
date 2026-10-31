use alloc::{boxed::Box, vec::Vec};
use tinywasm_types::{FuncType, ModuleInstanceId, TypeAddr, WasmType};

use super::{FromWasmValues, FuncContext, IntoWasmValues};
use crate::shared::StoreShared;
use crate::store::FuncValueTypes;
use crate::{Function, Result, Store, Trap, WasmValue};

/// Trait bounds required for a host callback in the current build mode.
#[doc(hidden)]
#[cfg(not(feature = "send"))]
pub trait HostFunctionCallback {}
#[cfg(not(feature = "send"))]
impl<T: ?Sized> HostFunctionCallback for T {}

/// Trait bounds required for a host callback in the current build mode.
#[doc(hidden)]
#[cfg(feature = "send")]
pub trait HostFunctionCallback: Send + Sync {}
#[cfg(feature = "send")]
impl<T: Send + Sync + ?Sized> HostFunctionCallback for T {}

/// A reusable host function definition.
///
/// Host functions accept thread-local callback state by default. With the
/// `send` feature, callbacks must implement `Send + Sync` and host function
/// definitions can be moved and shared across threads.
#[derive(Clone)]
pub struct HostFunction(StoreShared<HostFunctionInner>);

impl HostFunction {
    /// Instantiates the function with an already registered canonical type.
    pub(crate) fn instantiate_registered(&self, store: &mut Store, type_addr: TypeAddr) -> Function {
        let addr = store.add_host_func(type_addr, self.clone());
        Function { item: crate::StoreItem::new(store.id(), addr), module_id: 0 }
    }

    /// Resolves the importing module's types without allocating a function instance.
    pub(crate) fn resolve_import_type(&self, type_addrs: &[TypeAddr]) -> Result<FuncType> {
        let mut types = self.0.ty.params().iter().chain(self.0.ty.results());
        if types.all(|ty| !matches!(ty, WasmType::Ref(ty) if ty.is_concrete())) {
            return Ok(self.0.ty.clone());
        }
        let resolve = |ty: WasmType| -> Result<WasmType> {
            let WasmType::Ref(ref_ty) = ty else { return Ok(ty) };
            let Some(module_addr) = ref_ty.type_index() else { return Ok(ty) };
            let canonical = *type_addrs
                .get(module_addr as usize)
                .ok_or_else(|| crate::Error::other("host function signature contains an invalid concrete type"))?;
            Ok(WasmType::Ref(tinywasm_types::RefType::new_concrete(ref_ty.is_nullable(), canonical)))
        };
        let params = self.0.ty.params().iter().copied().map(resolve).collect::<Result<Vec<_>>>()?;
        let results = self.0.ty.results().iter().copied().map(resolve).collect::<Result<Vec<_>>>()?;
        Ok(FuncType::new(&params, &results))
    }

    /// Calls the host function through its untyped value interface.
    pub(crate) fn call_values(
        &self,
        store: &mut Store,
        module_id: ModuleInstanceId,
        type_addr: TypeAddr,
        args: &[WasmValue],
        results: &mut [WasmValue],
    ) -> Result<()> {
        {
            let expected = store.state.get_canonical_func_type(type_addr);
            if results.len() != expected.results().len() {
                return Err(crate::Error::other("host result buffer has the wrong length"));
            }
            for (result, ty) in results.iter_mut().zip(expected.results()) {
                *result = match ty {
                    WasmType::I32 => WasmValue::I64(0),
                    _ => WasmValue::I32(0),
                };
            }
        }
        match &self.0.callback {
            HostCallback::Untyped(func) => func(FuncContext { store, module_id }, args, results)?,
            HostCallback::Typed(func) => func.call(FuncContext { store, module_id }, args, results)?,
        }
        let expected = store.state.get_canonical_func_type(type_addr);
        if !results.iter().zip(expected.results()).all(|(value, &ty)| store.value_matches_type(value, ty)) {
            return Err(crate::Error::InvalidHostFnReturn {
                expected: Box::new(expected.clone()),
                actual: results.to_vec(),
            });
        }
        Ok(())
    }

    /// Returns the allocation-free typed callback when one is available.
    pub(crate) fn typed_callback(&self) -> Option<&dyn TypedHostCallback> {
        match &self.0.callback {
            HostCallback::Untyped(_) => None,
            HostCallback::Typed(func) => Some(&**func),
        }
    }

    /// Calls from Wasm using arguments on the store's restored value stack.
    pub(crate) fn call_stack(
        &self,
        store: &mut Store,
        module_id: ModuleInstanceId,
        type_addr: TypeAddr,
    ) -> Result<(), Trap> {
        if let Some(callback) = self.typed_callback() {
            return cold_err!(callback.call_stack(store, module_id, type_addr))
                .map_err(|error| Trap::HostFunction(Box::new(error)));
        }

        let ty = store.state.get_canonical_func_type(type_addr);
        let (param_count, result_count) = (ty.params().len(), ty.results().len());
        let base = store.value_stack.base_before(ty.params().iter().collect());
        store
            .with_scratch_values(param_count + result_count, |store, values| {
                let host_values = store.stack_value_iter(type_addr, FuncValueTypes::Params, base)?;
                for (slot, value) in values[..param_count].iter_mut().zip(host_values) {
                    *slot = value;
                }
                store.value_stack.truncate_to_base(base);
                let (params, results) = values.split_at_mut(param_count);
                self.call_values(store, module_id, type_addr, params, results)
                    .map_err(|error| crate::Error::Trap(Trap::HostFunction(Box::new(error))))?;
                store.push_wasm_values(results)
            })
            .map_err(|error| match error {
                crate::Error::Trap(trap) => trap,
                other => Trap::HostFunction(Box::new(other)),
            })
    }

    /// Create a directly callable, store-owned function from this definition.
    ///
    /// The returned [`Function`] can only be used with `store`.
    ///
    /// Host functions intended as module imports usually do not need to be
    /// instantiated manually. Pass the reusable definition to
    /// [`crate::Imports::define`] instead. TinyWasm will then match any GC
    /// reference types to the module that imports the function.
    ///
    /// # Errors
    ///
    /// Returns an error if the signature contains a concrete reference type.
    /// Define the function through [`crate::Imports`] when concrete types must
    /// be resolved against an importing module.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// use tinywasm::types::WasmValue;
    /// use tinywasm::{HostFunction, Store};
    ///
    /// let mut store = Store::default();
    /// let add_one = HostFunction::from(|_ctx, value: i32| Ok(value + 1));
    /// let function = add_one.instantiate(&mut store)?;
    ///
    /// let mut results = [WasmValue::I32(0)];
    /// function.call(&mut store, &[WasmValue::I32(41)], &mut results)?;
    /// assert_eq!(results, [WasmValue::I32(42)]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn instantiate(&self, store: &mut Store) -> Result<Function> {
        if self
            .0
            .ty
            .params()
            .iter()
            .chain(self.0.ty.results())
            .any(|ty| matches!(ty, WasmType::Ref(ty) if ty.is_concrete()))
        {
            return Err(crate::Error::other("standalone host functions cannot use concrete reference types"));
        }
        let type_addr = store.register_host_type(&self.0.ty);
        Ok(self.instantiate_registered(store, type_addr))
    }

    /// Create a new untyped host function.
    ///
    /// To call Wasm from inside the callback, use [`FuncContext::call`] or
    /// [`FuncContext::call_untyped`].
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
    /// let add_one = HostFunction::from_untyped(&ty, |_ctx: FuncContext<'_>, args, results| {
    ///     let WasmValue::I32(value) = args[0] else {
    ///         return Err(tinywasm::Error::Other("expected i32".into()));
    ///     };
    ///     results[0] = WasmValue::I32(value + 1);
    ///     Ok(())
    /// });
    ///
    /// let mut imports = Imports::new();
    /// imports.define("host", "add_one", add_one);
    /// # let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    /// # let call = instance.func::<i32, i32>(&store, "call")?;
    /// # assert_eq!(call.call(&mut store, 41)?, 42);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_untyped<F>(ty: &FuncType, func: F) -> Self
    where
        F: Fn(FuncContext<'_>, &[WasmValue], &mut [WasmValue]) -> Result<()> + HostFunctionCallback + 'static,
    {
        Self(StoreShared::new(HostFunctionInner { ty: ty.clone(), callback: HostCallback::Untyped(Box::new(func)) }))
    }

    /// Create a new typed host function.
    ///
    /// To call Wasm from inside the callback, use [`FuncContext::call`] or
    /// [`FuncContext::call_untyped`].
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
    /// let add_one = HostFunction::from(|_ctx, value: i32| Ok(value + 1));
    ///
    /// let mut imports = Imports::new();
    /// imports.define("host", "add_one", add_one);
    /// # let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    /// # let call = instance.func::<i32, i32>(&store, "call")?;
    /// # assert_eq!(call.call(&mut store, 41)?, 42);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from<F, P, R>(func: F) -> Self
    where
        F: Fn(FuncContext<'_>, P) -> Result<R> + HostFunctionCallback + 'static,
        P: FromWasmValues + 'static,
        R: IntoWasmValues + 'static,
    {
        let ty = FuncType::new(P::WASM_TYPES, R::WASM_TYPES);
        let func = TypedHostCallbackImpl { func, marker: core::marker::PhantomData };
        Self(StoreShared::new(HostFunctionInner { ty, callback: HostCallback::Typed(Box::new(func)) }))
    }
}

struct HostFunctionInner {
    ty: FuncType,
    callback: HostCallback,
}

enum HostCallback {
    Untyped(Box<UntypedHostCallback>),
    Typed(Box<dyn TypedHostCallback>),
}

#[cfg(not(feature = "send"))]
type UntypedHostCallback = dyn Fn(FuncContext<'_>, &[WasmValue], &mut [WasmValue]) -> Result<()>;
#[cfg(feature = "send")]
type UntypedHostCallback = dyn Fn(FuncContext<'_>, &[WasmValue], &mut [WasmValue]) -> Result<()> + Send + Sync;

/// Typed callback entry points with the build mode's thread-safety bounds.
pub(crate) trait TypedHostCallback: HostFunctionCallback {
    fn call(&self, ctx: FuncContext<'_>, args: &[WasmValue], results: &mut [WasmValue]) -> Result<()>;
    fn call_stack(&self, store: &mut Store, module_id: ModuleInstanceId, type_addr: TypeAddr) -> Result<()>;
}

struct TypedHostCallbackImpl<F, P, R> {
    func: F,
    marker: core::marker::PhantomData<fn(P) -> R>,
}

impl<F, P, R> TypedHostCallback for TypedHostCallbackImpl<F, P, R>
where
    F: Fn(FuncContext<'_>, P) -> Result<R> + HostFunctionCallback,
    P: FromWasmValues,
    R: IntoWasmValues,
{
    fn call(&self, ctx: FuncContext<'_>, args: &[WasmValue], results: &mut [WasmValue]) -> Result<()> {
        let mut values = args.iter().cloned();
        let params = cold_err!(P::from_wasm_values(&mut values))?;
        let mut values = (self.func)(ctx, params)?.into_wasm_values();
        for result in results {
            *result = cold_err!(values.next().ok_or_else(|| crate::Error::other("not enough typed function results")))?;
        }
        if values.next().is_some() {
            return cold!(Err(crate::Error::other("too many typed function results")));
        }
        Ok(())
    }

    fn call_stack(&self, store: &mut Store, module_id: ModuleInstanceId, type_addr: TypeAddr) -> Result<()> {
        // Canonicalizing reference types does not change their physical parameter lanes.
        let base = store.value_stack.base_before(P::WASM_TYPES.iter().collect());
        let params = {
            let mut values = store.stack_value_iter(type_addr, FuncValueTypes::Params, base)?;
            cold_err!(P::from_wasm_values(&mut values))
        };
        store.value_stack.truncate_to_base(base);
        let result = cold_err!((self.func)(FuncContext { store, module_id }, params?))?;
        store.push_typed_values::<true>(type_addr, result.into_wasm_values(), base)
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for HostFunction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HostFunction").field("ty", &self.0.ty).finish_non_exhaustive()
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::{Error, Imports, ModuleInstance};

    #[test]
    fn wasm_callback_errors_are_wrapped() {
        for instruction in ["call", "return_call"] {
            let source = alloc::format!(
                r#"(module
                    (import "env" "fail" (func $fail))
                    (func (export "run") {instruction} $fail))"#
            );
            let module = crate::parse_bytes(&wat::parse_str(source).unwrap()).unwrap();
            for typed in [false, true] {
                let host = if typed {
                    HostFunction::from(|_, (): ()| Err::<(), _>(Trap::Unreachable.into()))
                } else {
                    HostFunction::from_untyped(&FuncType::new(&[], &[]), |_, _, _| Err(Trap::Unreachable.into()))
                };
                let mut store = Store::default();
                let mut imports = Imports::new();
                imports.define("env", "fail", host);
                let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports)).unwrap();
                let function = instance.func::<(), ()>(&store, "run").unwrap();
                let Error::Trap(Trap::HostFunction(cause)) = function.call(&mut store, ()).unwrap_err() else {
                    panic!("callback errors must remain host-function traps");
                };
                assert!(matches!(cause.downcast_ref::<Error>(), Some(Error::Trap(Trap::Unreachable))));
            }
        }
    }
}

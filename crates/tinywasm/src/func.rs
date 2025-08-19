use crate::coro::CoroState;
use crate::interpreter;
use crate::interpreter::executor::SuspendedHostCoroState;
use crate::interpreter::stack::{CallFrame, Stack};
use crate::{log, unlikely, Function};
use crate::{Error, FuncContext, Result, Store};
use alloc::{boxed::Box, format, string::String, string::ToString, vec, vec::Vec};
use tinywasm_types::{ExternRef, FuncRef, FuncType, ModuleInstanceAddr, ResumeArgument, ValType, WasmValue};

#[derive(Debug)]
/// A function handle
pub struct FuncHandle {
    pub(crate) module_addr: ModuleInstanceAddr,
    pub(crate) addr: u32,
    pub(crate) ty: FuncType,

    /// The name of the function, if it has one
    pub name: Option<String>,
}

impl FuncHandle {
    /// Call a function (Invocation)
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#invocation>
    ///
    #[inline]
    pub fn call(&self, store: &mut Store, params: &[WasmValue]) -> Result<Vec<WasmValue>> {
        self.call_coro(store, params)?.suspend_to_err()
    }

    /// Call a function (Invocation) and anticipate possible yield instead as well as return
    #[inline]
    pub fn call_coro(&self, store: &mut Store, params: &[WasmValue]) -> Result<FuncHandleCallOutcome> {
        // Comments are ordered by the steps in the spec
        // In this implementation, some steps are combined and ordered differently for performance reasons

        // 3. Let func_ty be the function type
        let func_ty = &self.ty;

        // 4. If the length of the provided argument values is different from the number of expected arguments, then fail
        if unlikely(func_ty.params.len() != params.len()) {
            return Err(Error::Other(format!(
                "param count mismatch: expected {}, got {}",
                func_ty.params.len(),
                params.len()
            )));
        }

        // 5. For each value type and the corresponding value, check if types match
        if !(func_ty.params.iter().zip(params).enumerate().all(|(_i, (ty, param))| {
            if ty != &param.val_type() {
                log::error!("param type mismatch at index {}: expected {:?}, got {:?}", _i, ty, param);
                false
            } else {
                true
            }
        })) {
            return Err(Error::Other("Type mismatch".into()));
        }

        let func_inst = store.get_func(self.addr);
        let wasm_func = match &func_inst.func {
            Function::Host(host_func) => {
                let host_func = host_func.clone();
                let ctx = FuncContext { store, module_addr: self.module_addr };
                return Ok(host_func.call(ctx, params)?.map_state(|state| SuspendedFunc {
                    func: SuspendedFuncInner::Host(SuspendedHostCoroState {
                        coro_state: state,
                        coro_orig_function: self.addr,
                    }),
                    module_addr: self.module_addr,
                    store_id: store.id(),
                }));
            }
            Function::Wasm(wasm_func) => wasm_func,
        };

        // 6. Let f be the dummy frame
        let call_frame = CallFrame::new(wasm_func.clone(), func_inst.owner, params, 0);

        // 7. Push the frame f to the call stack
        // & 8. Push the values to the stack (Not needed since the call frame owns the values)
        let stack = Stack::new(call_frame);

        // 9. Invoke the function instance
        let runtime = store.runtime();
        let exec_outcome = runtime.exec(store, stack)?;
        Ok(exec_outcome
            .map_result(|mut stack| -> Vec<WasmValue> {
                // Once the function returns:
                // let result_m = func_ty.results.len();

                // 1. Assert: m values are on the top of the stack (Ensured by validation)
                // assert!(stack.values.len() >= result_m);

                // 2. Pop m values from the stack
                stack.values.pop_results(&func_ty.results)
                // The values are returned as the results of the invocation.
            })
            .map_state(|coro_state| -> SuspendedFunc {
                SuspendedFunc {
                    func: SuspendedFuncInner::Wasm(SuspendedWasmFunc {
                        runtime: coro_state,
                        result_types: func_ty.results.clone(),
                    }),
                    module_addr: self.module_addr,
                    store_id: store.id(),
                }
            }))
    }
}

#[derive(Debug)]
/// A typed function handle
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

    /// call a typed function, anticipating possible suspension of execution
    pub fn call_coro(&self, store: &mut Store, params: P) -> Result<TypedFuncHandleCallOutcome<R>> {
        // Convert params into Vec<WasmValue>
        let wasm_values = params.into_wasm_value_tuple();

        // Call the underlying WASM function
        let result = self.func.call_coro(store, &wasm_values)?;

        // Convert the Vec<WasmValue> back to R
        result
            .map_result(|vals| R::from_wasm_value_tuple(&vals))
            .map_state(|state| SuspendedFuncTyped::<R> { func: state, _marker: core::marker::PhantomData {} })
            .propagate_err_result()
    }
}

pub(crate) type FuncHandleCallOutcome = crate::coro::PotentialCoroCallResult<Vec<WasmValue>, SuspendedFunc>;
pub(crate) type TypedFuncHandleCallOutcome<R> = crate::coro::PotentialCoroCallResult<R, SuspendedFuncTyped<R>>;

#[derive(Debug)]
struct SuspendedWasmFunc {
    runtime: interpreter::SuspendedRuntime,
    result_types: Box<[ValType]>,
}
impl SuspendedWasmFunc {
    fn resume(
        &mut self,
        ctx: FuncContext<'_>,
        arg: ResumeArgument,
    ) -> Result<crate::CoroStateResumeResult<Vec<WasmValue>>> {
        Ok(self.runtime.resume(ctx, arg)?.map_result(|mut stack| stack.values.pop_results(&self.result_types)))
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // Wasm is bigger, but also much more common variant
enum SuspendedFuncInner {
    Wasm(SuspendedWasmFunc),
    Host(SuspendedHostCoroState),
}

/// handle to function that was suspended and can be resumed
#[derive(Debug)]
pub struct SuspendedFunc {
    func: SuspendedFuncInner,
    module_addr: ModuleInstanceAddr,
    store_id: usize,
}

impl crate::coro::CoroState<Vec<WasmValue>, &mut Store> for SuspendedFunc {
    fn resume(
        &mut self,
        store: &mut Store,
        arg: ResumeArgument,
    ) -> Result<crate::CoroStateResumeResult<Vec<WasmValue>>> {
        if store.id() != self.store_id {
            return Err(Error::InvalidStore);
        }

        let ctx = FuncContext { store, module_addr: self.module_addr };
        match &mut self.func {
            SuspendedFuncInner::Wasm(wasm) => wasm.resume(ctx, arg),
            SuspendedFuncInner::Host(host) => Ok(host.coro_state.resume(ctx, arg)?),
        }
    }
}

pub struct SuspendedFuncTyped<R> {
    pub func: SuspendedFunc,
    pub(crate) _marker: core::marker::PhantomData<R>,
}

impl<R> core::fmt::Debug for SuspendedFuncTyped<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SuspendedFuncTyped").field("func", &self.func).finish()
    }
}

impl<R> crate::coro::CoroState<R, &mut Store> for SuspendedFuncTyped<R>
where
    R: FromWasmValueTuple,
{
    fn resume(&mut self, ctx: &mut Store, arg: ResumeArgument) -> Result<crate::CoroStateResumeResult<R>> {
        self.func.resume(ctx, arg)?.map_result(|vals| R::from_wasm_value_tuple(&vals)).propagate_err()
    }
}

macro_rules! impl_into_wasm_value_tuple {
    ($($T:ident),*) => {
        impl<$($T),*> IntoWasmValueTuple for ($($T,)*)
        where
            $($T: Into<WasmValue>),*
        {
            #[allow(non_snake_case)]
            #[inline]
            fn into_wasm_value_tuple(self) -> Vec<WasmValue> {
                let ($($T,)*) = self;
                vec![$($T.into(),)*]
            }
        }
    }
}

macro_rules! impl_into_wasm_value_tuple_single {
    ($T:ident) => {
        impl IntoWasmValueTuple for $T {
            #[inline]
            fn into_wasm_value_tuple(self) -> Vec<WasmValue> {
                vec![self.into()]
            }
        }
    };
}

macro_rules! impl_from_wasm_value_tuple {
    ($($T:ident),*) => {
        impl<$($T),*> FromWasmValueTuple for ($($T,)*)
        where
            $($T: TryFrom<WasmValue, Error = ()>),*
        {
            #[inline]
            fn from_wasm_value_tuple(values: &[WasmValue]) -> Result<Self> {
                #[allow(unused_variables, unused_mut)]
                let mut iter = values.iter();

                Ok((
                    $(
                        $T::try_from(
                            *iter.next()
                            .ok_or(Error::Other("Not enough values in WasmValue vector".to_string()))?
                        )
                        .map_err(|e| Error::Other(format!("FromWasmValueTuple: Could not convert WasmValue to expected type: {:?}", e,
                    )))?,
                    )*
                ))
            }
        }
    }
}

macro_rules! impl_from_wasm_value_tuple_single {
    ($T:ident) => {
        impl FromWasmValueTuple for $T {
            #[inline]
            fn from_wasm_value_tuple(values: &[WasmValue]) -> Result<Self> {
                #[allow(unused_variables, unused_mut)]
                let mut iter = values.iter();
                $T::try_from(*iter.next().ok_or(Error::Other("Not enough values in WasmValue vector".to_string()))?)
                    .map_err(|e| {
                        Error::Other(format!(
                            "FromWasmValueTupleSingle: Could not convert WasmValue to expected type: {:?}",
                            e
                        ))
                    })
            }
        }
    };
}

pub trait ValTypesFromTuple {
    fn val_types() -> Box<[ValType]>;
}

pub trait ToValType {
    fn to_val_type() -> ValType;
}

impl ToValType for i32 {
    fn to_val_type() -> ValType {
        ValType::I32
    }
}

impl ToValType for i64 {
    fn to_val_type() -> ValType {
        ValType::I64
    }
}

impl ToValType for f32 {
    fn to_val_type() -> ValType {
        ValType::F32
    }
}

impl ToValType for f64 {
    fn to_val_type() -> ValType {
        ValType::F64
    }
}

impl ToValType for FuncRef {
    fn to_val_type() -> ValType {
        ValType::RefFunc
    }
}

impl ToValType for ExternRef {
    fn to_val_type() -> ValType {
        ValType::RefExtern
    }
}

macro_rules! impl_val_types_from_tuple {
    ($($t:ident),+) => {
        impl<$($t),+> ValTypesFromTuple for ($($t,)+)
        where
            $($t: ToValType,)+
        {
            #[inline]
            fn val_types() -> Box<[ValType]> {
                Box::new([$($t::to_val_type(),)+])
            }
        }
    };
}

impl ValTypesFromTuple for () {
    #[inline]
    fn val_types() -> Box<[ValType]> {
        Box::new([])
    }
}

impl<T: ToValType> ValTypesFromTuple for T {
    #[inline]
    fn val_types() -> Box<[ValType]> {
        Box::new([T::to_val_type()])
    }
}

impl_from_wasm_value_tuple_single!(i32);
impl_from_wasm_value_tuple_single!(i64);
impl_from_wasm_value_tuple_single!(f32);
impl_from_wasm_value_tuple_single!(f64);
impl_from_wasm_value_tuple_single!(FuncRef);
impl_from_wasm_value_tuple_single!(ExternRef);

impl_into_wasm_value_tuple_single!(i32);
impl_into_wasm_value_tuple_single!(i64);
impl_into_wasm_value_tuple_single!(f32);
impl_into_wasm_value_tuple_single!(f64);
impl_into_wasm_value_tuple_single!(FuncRef);
impl_into_wasm_value_tuple_single!(ExternRef);

impl_val_types_from_tuple!(T1);
impl_val_types_from_tuple!(T1, T2);
impl_val_types_from_tuple!(T1, T2, T3);
impl_val_types_from_tuple!(T1, T2, T3, T4);
impl_val_types_from_tuple!(T1, T2, T3, T4, T5);
impl_val_types_from_tuple!(T1, T2, T3, T4, T5, T6);

impl_from_wasm_value_tuple!();
impl_from_wasm_value_tuple!(T1);
impl_from_wasm_value_tuple!(T1, T2);
impl_from_wasm_value_tuple!(T1, T2, T3);
impl_from_wasm_value_tuple!(T1, T2, T3, T4);
impl_from_wasm_value_tuple!(T1, T2, T3, T4, T5);
impl_from_wasm_value_tuple!(T1, T2, T3, T4, T5, T6);

impl_into_wasm_value_tuple!();
impl_into_wasm_value_tuple!(T1);
impl_into_wasm_value_tuple!(T1, T2);
impl_into_wasm_value_tuple!(T1, T2, T3);
impl_into_wasm_value_tuple!(T1, T2, T3, T4);
impl_into_wasm_value_tuple!(T1, T2, T3, T4, T5);
impl_into_wasm_value_tuple!(T1, T2, T3, T4, T5, T6);

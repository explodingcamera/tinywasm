use alloc::{boxed::Box, format, string::String, string::ToString, vec, vec::Vec};
use log::{debug, info};
use tinywasm_types::{FuncAddr, FuncType, ValType, WasmValue};

use crate::{
    runtime::{CallFrame, Stack},
    Error, FuncContext, ModuleInstance, Result, Store,
};

#[derive(Debug)]
/// A function handle
pub struct FuncHandle {
    pub(crate) module: ModuleInstance,
    pub(crate) addr: FuncAddr,
    pub(crate) ty: FuncType,

    /// The name of the function, if it has one
    pub name: Option<String>,
}

impl FuncHandle {
    /// Call a function (Invocation)
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#invocation>
    pub fn call(&self, store: &mut Store, params: &[WasmValue]) -> Result<Vec<WasmValue>> {
        let mut stack = Stack::default();

        // 1. Assert: funcs[func_addr] exists
        // 2. let func_inst be the functiuon instance funcs[func_addr]
        let func_inst = store.get_func(self.addr as usize)?;

        // 3. Let func_ty be the function type
        let func_ty = &self.ty;

        // 4. If the length of the provided argument values is different from the number of expected arguments, then fail
        if func_ty.params.len() != params.len() {
            info!("func_ty.params: {:?}", func_ty.params);
            return Err(Error::Other(format!(
                "param count mismatch: expected {}, got {}",
                func_ty.params.len(),
                params.len()
            )));
        }

        // 5. For each value type and the corresponding value, check if types match
        for (i, (ty, param)) in func_ty.params.iter().zip(params).enumerate() {
            if ty != &param.val_type() {
                return Err(Error::Other(format!(
                    "param type mismatch at index {}: expected {:?}, got {:?}",
                    i, ty, param
                )));
            }
        }

        let wasm_func = match &func_inst.func {
            crate::Function::Host(h) => {
                let func = h.func.clone();
                let ctx = FuncContext { store, module: &self.module };
                return (func)(ctx, params);
            }
            crate::Function::Wasm(ref f) => f,
        };

        // 6. Let f be the dummy frame
        debug!("locals: {:?}", wasm_func.locals);
        let call_frame = CallFrame::new(self.addr as usize, params, wasm_func.locals.to_vec());

        // 7. Push the frame f to the call stack
        // & 8. Push the values to the stack (Not needed since the call frame owns the values)
        stack.call_stack.push(call_frame)?;

        // 9. Invoke the function instance
        let runtime = store.runtime();
        runtime.exec(store, &mut stack, self.module.clone())?;

        // Once the function returns:
        let result_m = func_ty.results.len();

        // 1. Assert: m values are on the top of the stack (Ensured by validation)
        assert!(stack.values.len() >= result_m);

        // 2. Pop m values from the stack
        let res = stack.values.last_n(result_m)?;

        // The values are returned as the results of the invocation.
        Ok(res.iter().zip(func_ty.results.iter()).map(|(v, ty)| v.attach_type(*ty)).collect())
    }
}

#[derive(Debug)]
/// A typed function handle
pub struct TypedFuncHandle<P, R> {
    /// The underlying function handle
    pub func: FuncHandle,
    pub(crate) marker: core::marker::PhantomData<(P, R)>,
}

pub trait IntoWasmValueTuple {
    fn into_wasm_value_tuple(self) -> Vec<WasmValue>;
}

pub trait FromWasmValueTuple {
    fn from_wasm_value_tuple(values: Vec<WasmValue>) -> Result<Self>
    where
        Self: Sized;
}

impl<P: IntoWasmValueTuple, R: FromWasmValueTuple> TypedFuncHandle<P, R> {
    /// Call a typed function
    pub fn call(&self, store: &mut Store, params: P) -> Result<R> {
        // Convert params into Vec<WasmValue>
        let wasm_values = params.into_wasm_value_tuple();

        // Call the underlying WASM function
        let result = self.func.call(store, &wasm_values)?;

        // Convert the Vec<WasmValue> back to R
        R::from_wasm_value_tuple(result)
    }
}
macro_rules! impl_into_wasm_value_tuple {
    ($($T:ident),*) => {
        impl<$($T),*> IntoWasmValueTuple for ($($T,)*)
        where
            $($T: Into<WasmValue>),*
        {
            #[allow(non_snake_case)]
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
            fn into_wasm_value_tuple(self) -> Vec<WasmValue> {
                vec![self.into()]
            }
        }
    };
}

impl_into_wasm_value_tuple_single!(i32);
impl_into_wasm_value_tuple_single!(i64);
impl_into_wasm_value_tuple_single!(f32);
impl_into_wasm_value_tuple_single!(f64);

impl_into_wasm_value_tuple!();
impl_into_wasm_value_tuple!(T1);
impl_into_wasm_value_tuple!(T1, T2);
impl_into_wasm_value_tuple!(T1, T2, T3);
impl_into_wasm_value_tuple!(T1, T2, T3, T4);
impl_into_wasm_value_tuple!(T1, T2, T3, T4, T5);
impl_into_wasm_value_tuple!(T1, T2, T3, T4, T5, T6);
impl_into_wasm_value_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_into_wasm_value_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);

macro_rules! impl_from_wasm_value_tuple {
    ($($T:ident),*) => {
        impl<$($T),*> FromWasmValueTuple for ($($T,)*)
        where
            $($T: TryFrom<WasmValue, Error = ()>),*
        {
            fn from_wasm_value_tuple(values: Vec<WasmValue>) -> Result<Self> {
                #[allow(unused_variables, unused_mut)]
                let mut iter = values.into_iter();
                Ok((
                    $(
                        $T::try_from(
                            iter.next()
                            .ok_or(Error::Other("Not enough values in WasmValue vector".to_string()))?
                        )
                        .map_err(|_| Error::Other("Could not convert WasmValue to expected type".to_string()))?,
                    )*
                ))
            }
        }
    }
}

macro_rules! impl_from_wasm_value_tuple_single {
    ($T:ident) => {
        impl FromWasmValueTuple for $T {
            fn from_wasm_value_tuple(values: Vec<WasmValue>) -> Result<Self> {
                #[allow(unused_variables, unused_mut)]
                let mut iter = values.into_iter();
                $T::try_from(iter.next().ok_or(Error::Other("Not enough values in WasmValue vector".to_string()))?)
                    .map_err(|_| Error::Other("Could not convert WasmValue to expected type".to_string()))
            }
        }
    };
}

impl_from_wasm_value_tuple_single!(i32);
impl_from_wasm_value_tuple_single!(i64);
impl_from_wasm_value_tuple_single!(f32);
impl_from_wasm_value_tuple_single!(f64);

impl_from_wasm_value_tuple!();
impl_from_wasm_value_tuple!(T1);
impl_from_wasm_value_tuple!(T1, T2);
impl_from_wasm_value_tuple!(T1, T2, T3);
impl_from_wasm_value_tuple!(T1, T2, T3, T4);
impl_from_wasm_value_tuple!(T1, T2, T3, T4, T5);
impl_from_wasm_value_tuple!(T1, T2, T3, T4, T5, T6);
impl_from_wasm_value_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_from_wasm_value_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);

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

macro_rules! impl_val_types_from_tuple {
    ($($t:ident),+) => {
        impl<$($t),+> ValTypesFromTuple for ($($t,)+)
        where
            $($t: ToValType,)+
        {
            fn val_types() -> Box<[ValType]> {
                Box::new([$($t::to_val_type(),)+])
            }
        }
    };
}

impl ValTypesFromTuple for () {
    fn val_types() -> Box<[ValType]> {
        Box::new([])
    }
}

impl<T1> ValTypesFromTuple for T1
where
    T1: ToValType,
{
    fn val_types() -> Box<[ValType]> {
        Box::new([T1::to_val_type()])
    }
}

impl_val_types_from_tuple!(T1);
impl_val_types_from_tuple!(T1, T2);
impl_val_types_from_tuple!(T1, T2, T3);
impl_val_types_from_tuple!(T1, T2, T3, T4);
impl_val_types_from_tuple!(T1, T2, T3, T4, T5);
impl_val_types_from_tuple!(T1, T2, T3, T4, T5, T6);

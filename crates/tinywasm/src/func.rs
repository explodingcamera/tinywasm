use alloc::{format, string::String, string::ToString, vec, vec::Vec};
use tinywasm_types::{FuncAddr, FuncType, ValType, WasmValue};

use crate::{runtime::Stack, Error, ModuleInstance, Result, Store};

#[derive(Debug)]
pub struct FuncHandle {
    pub(crate) _module: ModuleInstance,
    pub(crate) addr: FuncAddr,
    pub(crate) ty: FuncType,
    pub name: Option<String>,
}
impl FuncHandle {
    /// Call a function
    pub fn call(&self, store: &mut Store, params: &[WasmValue]) -> Result<Vec<WasmValue>> {
        let func = store
            .data
            .funcs
            .get(self.addr as usize)
            .ok_or(Error::Other(format!("function {} not found", self.addr)))?;

        let func_ty = &self.ty;

        // check that params match func_ty params
        for (ty, param) in func_ty.params.iter().zip(params) {
            if ty != &param.val_type() {
                return Err(Error::Other(format!(
                    "param type mismatch: expected {:?}, got {:?}",
                    ty, param
                )));
            }
        }

        let mut local_types: Vec<ValType> = Vec::new();
        local_types.extend(func_ty.params.iter());
        local_types.extend(func.locals().iter());

        // let runtime = &mut store.runtime;

        let mut stack = Stack::default();
        stack.locals.extend(params.iter().cloned());

        let instrs = func.instructions().iter();
        store.runtime.exec(&mut stack, instrs)?;

        let res = func_ty
            .results
            .iter()
            .map(|_| stack.value_stack.pop())
            .collect::<Option<Vec<_>>>()
            .ok_or(Error::Other(
                "function did not return the correct number of values".into(),
            ))?;

        Ok(res)
    }
}

pub struct TypedFuncHandle<P, R> {
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

impl_into_wasm_value_tuple!(T1);
impl_into_wasm_value_tuple!(T1, T2);
impl_into_wasm_value_tuple!(T1, T2, T3);
impl_into_wasm_value_tuple!(T1, T2, T3, T4);
impl_into_wasm_value_tuple!(T1, T2, T3, T4, T5);

macro_rules! impl_from_wasm_value_tuple {
    ($($T:ident),*) => {
        impl<$($T),*> FromWasmValueTuple for ($($T,)*)
        where
            $($T: TryFrom<WasmValue, Error = ()>),*
        {
            fn from_wasm_value_tuple(values: Vec<WasmValue>) -> Result<Self> {
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

impl_from_wasm_value_tuple!(T1);
impl_from_wasm_value_tuple!(T1, T2);
impl_from_wasm_value_tuple!(T1, T2, T3);
impl_from_wasm_value_tuple!(T1, T2, T3, T4);
impl_from_wasm_value_tuple!(T1, T2, T3, T4, T5);

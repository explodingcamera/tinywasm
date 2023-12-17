use alloc::{format, string::String, string::ToString, vec, vec::Vec};
use log::{debug, info};
use tinywasm_types::{FuncAddr, FuncType, WasmValue};

use crate::{
    runtime::{CallFrame, Stack},
    Error, ModuleInstance, Result, Store,
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
    /// Call a function
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

        // 6. Let f be the dummy frame
        debug!("locals: {:?}", func_inst.locals());
        let call_frame = CallFrame::new(self.addr as usize, params, func_inst.locals().to_vec());

        // 7. Push the frame f to the call stack
        stack.call_stack.push(call_frame);

        // 8. Push the values to the stack (Not needed since the call frame owns the values)

        // 9. Invoke the function instance
        let runtime = store.runtime();
        runtime.exec(store, &mut stack, self.module.clone())?;

        // Once the function returns:
        let result_m = func_ty.results.len();
        let res = stack.values.pop_n(result_m)?;

        Ok(res
            .iter()
            .zip(func_ty.results.iter())
            .map(|(v, ty)| v.attach_type(*ty))
            .collect())
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

impl_from_wasm_value_tuple!();
impl_from_wasm_value_tuple!(T1);
impl_from_wasm_value_tuple!(T1, T2);
impl_from_wasm_value_tuple!(T1, T2, T3);
impl_from_wasm_value_tuple!(T1, T2, T3, T4);
impl_from_wasm_value_tuple!(T1, T2, T3, T4, T5);
impl_from_wasm_value_tuple!(T1, T2, T3, T4, T5, T6);
impl_from_wasm_value_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_from_wasm_value_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);

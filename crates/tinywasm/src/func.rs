use crate::interpreter::stack::CallFrame;
use crate::{Error, FuncContext, InterpreterRuntime, Result, Store};
use crate::{Function, log, unlikely};
use alloc::{boxed::Box, format, string::ToString, vec, vec::Vec};
use tinywasm_types::{ExternRef, FuncRef, FuncType, ModuleInstanceAddr, ValType, WasmValue};

#[derive(Debug)]
/// A function handle
pub struct FuncHandle {
    pub(crate) module_addr: ModuleInstanceAddr,
    pub(crate) addr: u32,
    pub(crate) ty: FuncType,
}

impl FuncHandle {
    /// Call a function (Invocation)
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#invocation>
    #[inline]
    pub fn call(&self, store: &mut Store, params: &[WasmValue]) -> Result<Vec<WasmValue>> {
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
            if ty == &param.val_type() {
                true
            } else {
                log::error!("param type mismatch at index {_i}: expected {ty:?}, got {param:?}");
                false
            }
        })) {
            return Err(Error::Other("Type mismatch".into()));
        }

        let func_inst = store.state.get_func(self.addr);
        let wasm_func = match &func_inst.func {
            Function::Host(host_func) => {
                let host_func = host_func.clone();
                let ctx = FuncContext { store, module_addr: self.module_addr };
                return host_func.call(ctx, params);
            }
            Function::Wasm(wasm_func) => wasm_func.clone(),
        };

        // 6. Let f be the dummy frame
        let callframe = CallFrame::new(wasm_func, func_inst.owner, params, 0);

        // 7. Push the frame f to the call stack
        // & 8. Push the values to the stack (Not needed since the call frame owns the values)
        store.stack.initialize(callframe);

        // 9. Invoke the function instance
        InterpreterRuntime::exec(store)?;

        // Once the function returns:
        // 1. Assert: m values are on the top of the stack (Ensured by validation)
        debug_assert!(store.stack.values.len() >= func_ty.results.len());

        // 2. Pop m values from the stack
        let mut res: Vec<_> = store.stack.values.pop_types(func_ty.results.iter().rev()).collect(); // pop in reverse order since the stack is LIFO
        res.reverse(); // reverse to get the original order

        // The values are returned as the results of the invocation.
        Ok(res)
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
}

pub trait ValTypesFromTuple {
    fn val_types() -> Box<[ValType]>;
}

pub trait ToValType {
    fn to_val_type() -> ValType;
}

macro_rules! impl_scalar_wasm_traits {
    ($($T:ty => $val_ty:ident),+ $(,)?) => {
        $(
            impl ToValType for $T {
                #[inline]
                fn to_val_type() -> ValType {
                    ValType::$val_ty
                }
            }

            impl ValTypesFromTuple for $T {
                #[inline]
                fn val_types() -> Box<[ValType]> {
                    Box::new([ValType::$val_ty])
                }
            }

            impl IntoWasmValueTuple for $T {
                #[inline]
                fn into_wasm_value_tuple(self) -> Vec<WasmValue> {
                    vec![self.into()]
                }
            }

            impl FromWasmValueTuple for $T {
                #[inline]
                fn from_wasm_value_tuple(values: &[WasmValue]) -> Result<Self> {
                    let value = *values
                        .first()
                        .ok_or(Error::Other("Not enough values in WasmValue vector".to_string()))?;
                    <$T>::try_from(value).map_err(|e| {
                        Error::Other(format!(
                            "FromWasmValueTuple: Could not convert WasmValue to expected type: {:?}",
                            e
                        ))
                    })
                }
            }
        )+
    };
}

macro_rules! impl_tuple_traits {
    ($($T:ident),+) => {
        impl<$($T),+> ValTypesFromTuple for ($($T,)+)
        where
            $($T: ToValType,)+
        {
            #[inline]
            fn val_types() -> Box<[ValType]> {
                Box::new([$($T::to_val_type(),)+])
            }
        }

        impl<$($T),+> IntoWasmValueTuple for ($($T,)+)
        where
            $($T: Into<WasmValue>,)+
        {
            #[allow(non_snake_case)]
            #[inline]
            fn into_wasm_value_tuple(self) -> Vec<WasmValue> {
                let ($($T,)+) = self;
                vec![$($T.into(),)+]
            }
        }

        impl<$($T),+> FromWasmValueTuple for ($($T,)+)
        where
            $($T: TryFrom<WasmValue, Error = ()>,)+
        {
            #[inline]
            fn from_wasm_value_tuple(values: &[WasmValue]) -> Result<Self> {
                let mut iter = values.iter();

                Ok((
                    $(
                        $T::try_from(
                            *iter.next()
                            .ok_or(Error::Other("Not enough values in WasmValue vector".to_string()))?
                        )
                        .map_err(|e| Error::Other(format!(
                            "FromWasmValueTuple: Could not convert WasmValue to expected type: {:?}",
                            e,
                        )))?,
                    )+
                ))
            }
        }
    }
}

macro_rules! impl_tuple {
    ($macro:ident) => {
        $macro!(T1);
        $macro!(T1, T2);
        $macro!(T1, T2, T3);
        $macro!(T1, T2, T3, T4);
        $macro!(T1, T2, T3, T4, T5);
        $macro!(T1, T2, T3, T4, T5, T6);
        $macro!(T1, T2, T3, T4, T5, T6, T7);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
    };
}

impl_scalar_wasm_traits!(
    i32 => I32,
    i64 => I64,
    f32 => F32,
    f64 => F64,
    FuncRef => RefFunc,
    ExternRef => RefExtern,
);
impl_tuple!(impl_tuple_traits);

impl ValTypesFromTuple for () {
    #[inline]
    fn val_types() -> Box<[ValType]> {
        Box::new([])
    }
}

impl IntoWasmValueTuple for () {
    #[inline]
    fn into_wasm_value_tuple(self) -> Vec<WasmValue> {
        vec![]
    }
}

impl FromWasmValueTuple for () {
    #[inline]
    fn from_wasm_value_tuple(_values: &[WasmValue]) -> Result<Self> {
        Ok(())
    }
}

use crate::{Error, Result};
use alloc::borrow::Cow;
use tinywasm_types::WasmType;

use crate::{AnyRef, ArrayRef, EqRef, ExnRef, ExternRef, FuncRef, I31Ref, StructRef, WasmValue};

/// Convert a Rust value or tuple into WebAssembly values.
pub trait IntoWasmValues {
    /// Return the flattened WebAssembly values.
    fn into_wasm_values(self) -> impl Iterator<Item = WasmValue>;
}

/// Convert WebAssembly values into a Rust value or tuple.
pub trait FromWasmValues: Sized {
    /// Read this value from a flattened WebAssembly value iterator.
    fn from_wasm_values(values: &mut impl Iterator<Item = WasmValue>) -> Result<Self>;

    /// Read one value and reject unconsumed iterator items.
    fn from_wasm_values_exact(values: &mut impl Iterator<Item = WasmValue>) -> Result<Self> {
        let result = Self::from_wasm_values(values)?;
        if values.next().is_some() {
            return Err(Error::other("typed conversion did not consume all WebAssembly values"));
        }
        Ok(result)
    }
}

/// Describes the WebAssembly value types produced by a Rust value or tuple shape.
pub trait ToWasmTypes {
    /// Static WebAssembly types for this shape.
    ///
    /// Implementations that require runtime construction may set this to `None`,
    /// but must then override [`Self::wasm_types`].
    const WASM_TYPES: Option<&'static [WasmType]>;

    /// Return the flattened WebAssembly value types for this tuple shape.
    fn wasm_types() -> Cow<'static, [WasmType]> {
        Cow::Borrowed(Self::WASM_TYPES.expect("dynamic ToWasmTypes implementation must override wasm_types"))
    }
}

/// Describes the WebAssembly value types produced by a scalar Rust type.
pub trait ToWasmType {
    /// The single WebAssembly value type for this scalar type.
    const WASM_TYPE: WasmType;
}

fn next_value<T: TryFrom<WasmValue, Error = ()>>(values: &mut impl Iterator<Item = WasmValue>) -> Result<T> {
    let value = values.next().ok_or_else(|| {
        core::hint::cold_path();
        Error::other("not enough WebAssembly values")
    })?;
    T::try_from(value).map_err(|_| {
        core::hint::cold_path();
        Error::other("WebAssembly value does not match the expected type")
    })
}

macro_rules! impl_scalar_wasm_traits {
    ($($T:ty => $val_ty:expr),+ $(,)?) => {
        $(
            impl ToWasmType for $T {
                const WASM_TYPE: WasmType = $val_ty;
            }

            impl ToWasmTypes for $T {
                const WASM_TYPES: Option<&'static [WasmType]> = Some(&[$val_ty]);
            }

            impl IntoWasmValues for $T {
                #[inline]
                fn into_wasm_values(self) -> impl Iterator<Item = WasmValue> {
                    core::iter::once(self.into())
                }
            }

            impl FromWasmValues for $T {
                #[inline]
                fn from_wasm_values(values: &mut impl Iterator<Item = WasmValue>) -> Result<Self> {
                    next_value(values)
                }
            }
        )+
    };
}

macro_rules! impl_tuple_traits {
    (@next $head:ident, $($tail:ident),+) => {
        impl_tuple_traits!($($tail),+);
    };
    (@next $head:ident) => {};
    ($($T:ident),+) => {
        impl<$($T),+> ToWasmTypes for ($($T,)+)
        where
            $($T: ToWasmType,)+
        {
            const WASM_TYPES: Option<&'static [WasmType]> = Some(&[$($T::WASM_TYPE,)+]);
        }

        impl<$($T),+> IntoWasmValues for ($($T,)+)
        where
            $($T: Into<WasmValue>,)+
        {
            #[allow(non_snake_case)]
            #[inline]
            fn into_wasm_values(self) -> impl Iterator<Item = WasmValue> {
                let ($($T,)+) = self;
                [$($T.into(),)+].into_iter()
            }
        }

        impl<$($T),+> FromWasmValues for ($($T,)+)
        where
            $($T: TryFrom<WasmValue, Error = ()>,)+
        {
            #[inline]
            fn from_wasm_values(values: &mut impl Iterator<Item = WasmValue>) -> Result<Self> {
                Ok(($(next_value::<$T>(values)?,)+))
            }
        }

        impl_tuple_traits!(@next $($T),+);
    }
}

impl_scalar_wasm_traits!(
    i32 => WasmType::I32,
    i64 => WasmType::I64,
    f32 => WasmType::F32,
    f64 => WasmType::F64,
    [u8; 16] => WasmType::V128,
    FuncRef => WasmType::Ref(tinywasm_types::RefType::FUNCREF.with_nullability(false)),
    AnyRef => WasmType::Ref(tinywasm_types::RefType::new_abstract(false, tinywasm_types::AbstractHeapType::Any)),
    EqRef => WasmType::Ref(tinywasm_types::RefType::new_abstract(false, tinywasm_types::AbstractHeapType::Eq)),
    I31Ref => WasmType::Ref(tinywasm_types::RefType::new_abstract(false, tinywasm_types::AbstractHeapType::I31)),
    StructRef => WasmType::Ref(tinywasm_types::RefType::new_abstract(false, tinywasm_types::AbstractHeapType::Struct)),
    ArrayRef => WasmType::Ref(tinywasm_types::RefType::new_abstract(false, tinywasm_types::AbstractHeapType::Array)),
    ExternRef => WasmType::Ref(tinywasm_types::RefType::EXTERNREF.with_nullability(false)),
    ExnRef => WasmType::Ref(tinywasm_types::RefType::EXNREF.with_nullability(false)),
);

impl_scalar_wasm_traits!(
    Option<FuncRef> => WasmType::Ref(tinywasm_types::RefType::FUNCREF),
    Option<AnyRef> => WasmType::Ref(tinywasm_types::RefType::new_abstract(true, tinywasm_types::AbstractHeapType::Any)),
    Option<EqRef> => WasmType::Ref(tinywasm_types::RefType::new_abstract(true, tinywasm_types::AbstractHeapType::Eq)),
    Option<I31Ref> => WasmType::Ref(tinywasm_types::RefType::new_abstract(true, tinywasm_types::AbstractHeapType::I31)),
    Option<StructRef> => WasmType::Ref(tinywasm_types::RefType::new_abstract(true, tinywasm_types::AbstractHeapType::Struct)),
    Option<ArrayRef> => WasmType::Ref(tinywasm_types::RefType::new_abstract(true, tinywasm_types::AbstractHeapType::Array)),
    Option<ExternRef> => WasmType::Ref(tinywasm_types::RefType::EXTERNREF),
    Option<ExnRef> => WasmType::Ref(tinywasm_types::RefType::EXNREF),
);
impl_tuple_traits!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20);

impl ToWasmTypes for () {
    const WASM_TYPES: Option<&'static [WasmType]> = Some(&[]);
}

impl IntoWasmValues for () {
    #[inline]
    fn into_wasm_values(self) -> impl Iterator<Item = WasmValue> {
        core::iter::empty()
    }
}

impl FromWasmValues for () {
    #[inline]
    fn from_wasm_values(_values: &mut impl Iterator<Item = WasmValue>) -> Result<Self> {
        Ok(())
    }
}

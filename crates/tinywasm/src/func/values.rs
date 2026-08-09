use crate::{Error, Result};
use alloc::{borrow::Cow, vec::Vec};
use tinywasm_types::{ExternRef, FuncRef, WasmType, WasmValue};

/// Convert a Rust value or tuple into WebAssembly values.
pub trait IntoWasmValues {
    /// Return the flattened WebAssembly values.
    fn into_wasm_values(self) -> impl Iterator<Item = WasmValue>;
}

/// Convert WebAssembly values into a Rust value or tuple.
pub trait FromWasmValues: Sized {
    /// Read this value from a flattened WebAssembly value iterator.
    fn from_wasm_values(values: &mut impl Iterator<Item = WasmValue>) -> Result<Self>;
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
    FuncRef => WasmType::Ref(tinywasm_types::RefType::FUNCREF),
    ExternRef => WasmType::Ref(tinywasm_types::RefType::EXTERNREF),
);
impl_tuple_traits!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20);

/// Concatenates two typed parameter or result groups.
///
/// Direct tuple conversions are supported up to arity 20. Use untyped functions
/// for larger signatures.
#[deprecated(note = "direct tuples are supported up to arity 20, use untyped functions for larger signatures")]
#[derive(Default)]
pub struct WasmTupleChain<T1, T2>(T1, T2);

#[allow(deprecated)]
impl<T1, T2> WasmTupleChain<T1, T2> {
    /// Create a new concatenated tuple wrapper.
    pub const fn new(left: T1, right: T2) -> Self {
        Self(left, right)
    }

    /// Split the wrapper back into its two component values.
    pub fn into_inner(self) -> (T1, T2) {
        (self.0, self.1)
    }
}

#[allow(deprecated)]
impl<T1, T2> From<(T1, T2)> for WasmTupleChain<T1, T2> {
    fn from((left, right): (T1, T2)) -> Self {
        Self::new(left, right)
    }
}

#[allow(deprecated)]
impl<T1: ToWasmTypes, T2: ToWasmTypes> ToWasmTypes for WasmTupleChain<T1, T2> {
    const WASM_TYPES: Option<&'static [WasmType]> = None;

    #[inline]
    fn wasm_types() -> Cow<'static, [WasmType]> {
        let mut types = Vec::new();
        types.extend_from_slice(&T1::wasm_types());
        types.extend_from_slice(&T2::wasm_types());
        Cow::Owned(types)
    }
}

#[allow(deprecated)]
impl<T1: IntoWasmValues, T2: IntoWasmValues> IntoWasmValues for WasmTupleChain<T1, T2> {
    #[inline]
    fn into_wasm_values(self) -> impl Iterator<Item = WasmValue> {
        let (left, right) = self.into_inner();
        left.into_wasm_values().chain(right.into_wasm_values())
    }
}

#[allow(deprecated)]
impl<T1: FromWasmValues, T2: FromWasmValues> FromWasmValues for WasmTupleChain<T1, T2> {
    #[inline]
    fn from_wasm_values(values: &mut impl Iterator<Item = WasmValue>) -> Result<Self> {
        let left = T1::from_wasm_values(values)?;
        let right = T2::from_wasm_values(values)?;
        Ok(Self::new(left, right))
    }
}

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

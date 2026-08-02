#[cfg(all(feature = "validate", parallel_parser))]
pub(crate) use wasmparser::FuncToValidate;
#[cfg(feature = "validate")]
pub(crate) use wasmparser::{FuncValidator, FuncValidatorAllocations, Validator, ValidatorResources};

#[cfg(all(not(feature = "validate"), parallel_parser))]
pub(crate) type FuncToValidate<T> = core::marker::PhantomData<T>;
#[cfg(not(feature = "validate"))]
pub(crate) type FuncValidator<T> = core::marker::PhantomData<T>;
#[cfg(not(feature = "validate"))]
pub(crate) type FuncValidatorAllocations = ();
#[cfg(not(feature = "validate"))]
pub(crate) type Validator = ();
#[cfg(not(feature = "validate"))]
pub(crate) type ValidatorResources = ();

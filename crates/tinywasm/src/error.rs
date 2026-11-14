use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::{Debug, Display};
use tinywasm_types::FuncType;
use tinywasm_types::archive::TwasmError;

use crate::{ExnRef, WasmValue};

#[cfg(feature = "parser")]
pub use tinywasm_parser::ParseError;

/// Errors that can occur for `TinyWasm` operations
#[non_exhaustive]
pub enum Error {
    /// A WebAssembly trap occurred
    Trap(Trap),

    /// A host callback failed, retaining its original error outside interpreter dispatch.
    HostFunction(Box<dyn core::error::Error + Send + Sync>),

    /// An uncaught WebAssembly exception occurred.
    Exception(ExnRef),

    /// A linking error occurred
    Linker(LinkingError),

    /// A WebAssembly feature is not supported
    UnsupportedFeature(&'static str),

    /// An unknown error occurred
    Other(String),

    /// A host function returned an invalid value
    InvalidHostFnReturn {
        /// The expected type
        expected: Box<FuncType>,
        /// The actual value
        actual: Vec<WasmValue>,
    },

    /// An invalid label type was encountered
    InvalidLabelType,

    #[cfg(feature = "std")]
    /// An I/O error occurred
    Io(crate::std::io::Error),

    #[cfg(feature = "parser")]
    /// A parsing error occurred
    Parser(ParseError),

    /// A serialization error occurred
    Twasm(TwasmError),
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Trap(a), Self::Trap(b)) => a == b,
            (Self::HostFunction(_), Self::HostFunction(_)) => true,
            (Self::Exception(a), Self::Exception(b)) => a == b,
            (Self::Linker(a), Self::Linker(b)) => a == b,
            (Self::UnsupportedFeature(a), Self::UnsupportedFeature(b)) => a == b,
            (Self::Other(a), Self::Other(b)) => a == b,
            #[cfg(feature = "std")]
            (Self::Io(a), Self::Io(b)) => a.kind() == b.kind(),
            #[cfg(feature = "parser")]
            (Self::Parser(a), Self::Parser(b)) => a == b,
            (Self::Twasm(a), Self::Twasm(b)) => a == b,
            _ => false,
        }
    }
}

/// Errors that can occur when linking a WebAssembly module
#[non_exhaustive]
#[cfg_attr(feature = "debug", derive(Debug))]
#[derive(PartialEq, Eq)]
pub enum LinkingError {
    /// An unknown import was encountered
    UnknownImport {
        /// The module name
        module: String,
        /// The import name
        name: String,
    },

    /// A mismatched import type was encountered
    IncompatibleImportType {
        /// The module name
        module: String,
        /// The import name
        name: String,
    },
}

impl LinkingError {
    pub(crate) fn incompatible_import_type(import: &tinywasm_types::Import) -> Self {
        Self::IncompatibleImportType { module: import.module.to_string(), name: import.name.to_string() }
    }

    pub(crate) fn unknown_import(import: &tinywasm_types::Import) -> Self {
        Self::UnknownImport { module: import.module.to_string(), name: import.name.to_string() }
    }
}

impl Error {
    pub(crate) fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}

/// An execution or runtime trap.
///
/// Variant names follow WebAssembly testsuite trap messages where possible. This also includes
/// TinyWasm runtime failures, such as stack exhaustion, allocation failure, and invalid host handles.
/// Host callback errors are reported separately as [`Error::HostFunction`].
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#syntax-trap>
#[non_exhaustive]
#[cfg_attr(feature = "debug", derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Trap {
    /// An unreachable instruction was executed
    Unreachable,

    /// An out-of-bounds memory access occurred
    MemoryOutOfBounds,

    /// An atomic memory access was not naturally aligned.
    UnalignedAtomic,

    /// An out-of-bounds table access occurred
    TableOutOfBounds,

    /// An out-of-bounds GC array access occurred.
    ArrayOutOfBounds,

    /// An integer division or remainder operation used a zero divisor.
    IntegerDivisionByZero,

    /// A NaN value was converted to an integer.
    InvalidConversionToInteger,

    /// A Store-owned handle or reference was used with a different Store.
    InvalidStore,

    /// A reference does not identify a live value of the expected kind.
    InvalidReference,

    /// Integer Overflow
    IntegerOverflow,

    /// Call stack overflow
    CallStackOverflow,

    /// Value stack overflow
    ValueStackOverflow,

    /// An allocation or requested resource size could not be satisfied.
    OutOfMemory,

    /// An indirect call used an out-of-bounds table index.
    UndefinedElement,

    /// An indirect call selected a null table element.
    UninitializedElement,

    /// A null reference was used where a non-null reference was required.
    NullReference,

    /// A null struct reference was dereferenced.
    NullStructReference,

    /// A null array reference was dereferenced.
    NullArrayReference,

    /// A null function reference was called.
    NullFunctionReference,

    /// A null i31 reference was unwrapped.
    NullI31Reference,

    /// A reference cast failed.
    CastFailure,

    /// Indirect call type mismatch
    IndirectCallTypeMismatch,

    /// An atomic wait was attempted on an ordinary memory.
    AtomicWaitNonSharedMemory,

    /// A value does not match a table's element type.
    InvalidTableValueType,
}

impl Trap {
    /// Get the message of the trap
    pub fn message(&self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::MemoryOutOfBounds => "out of bounds memory access",
            Self::UnalignedAtomic => "unaligned atomic",
            Self::TableOutOfBounds => "out of bounds table access",
            Self::ArrayOutOfBounds => "out of bounds array access",
            Self::IntegerDivisionByZero => "integer divide by zero",
            Self::InvalidConversionToInteger => "invalid conversion to integer",
            Self::IntegerOverflow => "integer overflow",
            Self::CallStackOverflow => "call stack exhausted",
            Self::ValueStackOverflow => "value stack exhausted",
            Self::OutOfMemory => "out of memory",
            Self::UndefinedElement => "undefined element",
            Self::UninitializedElement => "uninitialized element",
            Self::NullReference => "null reference",
            Self::NullStructReference => "null structure reference",
            Self::NullArrayReference => "null array reference",
            Self::NullFunctionReference => "null function reference",
            Self::NullI31Reference => "null i31 reference",
            Self::CastFailure => "cast failure",
            Self::IndirectCallTypeMismatch => "indirect call type mismatch",
            Self::AtomicWaitNonSharedMemory => "atomic wait requires shared memory",
            Self::InvalidTableValueType => "invalid table value type",
            Self::InvalidStore => "invalid store",
            Self::InvalidReference => "invalid reference",
        }
    }
}

impl LinkingError {
    /// Get the message of the linking error
    pub fn message(&self) -> &'static str {
        match self {
            Self::UnknownImport { .. } => "unknown import",
            Self::IncompatibleImportType { .. } => "incompatible import type",
        }
    }
}

impl From<LinkingError> for Error {
    #[cold]
    fn from(value: LinkingError) -> Self {
        Self::Linker(value)
    }
}

impl From<TwasmError> for Error {
    #[cold]
    fn from(value: TwasmError) -> Self {
        Self::Twasm(value)
    }
}

impl From<Trap> for Error {
    #[cold]
    fn from(value: Trap) -> Self {
        Self::Trap(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(feature = "parser")]
            Self::Parser(err) => write!(f, "error parsing module: {err:?}"),

            #[cfg(feature = "std")]
            Self::Io(err) => write!(f, "I/O error: {err}"),

            Self::Twasm(err) => write!(f, "serialization error: {err}"),
            Self::Trap(trap) => write!(f, "trap: {trap}"),
            Self::HostFunction(error) => write!(f, "host function trap: {error}"),
            Self::Exception(_) => write!(f, "uncaught WebAssembly exception"),
            Self::Linker(err) => write!(f, "linking error: {err}"),
            Self::InvalidLabelType => write!(f, "invalid label type"),
            Self::Other(message) => write!(f, "unknown error: {message}"),
            Self::UnsupportedFeature(feature) => write!(f, "unsupported feature: {feature}"),
            #[cfg(feature = "debug")]
            Self::InvalidHostFnReturn { expected, actual } => {
                write!(f, "invalid host function return: expected={expected:?}, actual={actual:?}")
            }
            #[cfg(not(feature = "debug"))]
            Self::InvalidHostFnReturn { .. } => write!(f, "invalid host function return"),
        }
    }
}

impl Display for LinkingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownImport { module, name } => write!(f, "unknown import: {module}.{name}"),
            Self::IncompatibleImportType { module, name } => {
                write!(f, "incompatible import type: {module}.{name}")
            }
        }
    }
}

impl Display for Trap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.message())
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self)
    }
}

impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::HostFunction(error) => Some(&**error),
            _ => None,
        }
    }
}

#[cfg(feature = "std")]
impl From<Error> for crate::std::io::Error {
    fn from(value: Error) -> Self {
        match value {
            Error::Io(err) => err,
            other => Self::other(other.to_string()),
        }
    }
}

#[cfg(feature = "parser")]
impl From<tinywasm_parser::ParseError> for Error {
    #[cold]
    fn from(value: tinywasm_parser::ParseError) -> Self {
        Self::Parser(value)
    }
}

/// A wrapper around [`core::result::Result`] for tinywasm operations
pub type Result<T, E = Error> = crate::std::result::Result<T, E>;

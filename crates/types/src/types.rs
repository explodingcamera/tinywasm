use alloc::{boxed::Box, sync::Arc};

use crate::{TypeAddr, WasmType};

/// The dense type index space of a WebAssembly module.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#syntax-rectype>
#[derive(Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct TypeSection {
    /// Types in module index order.
    pub types: Box<[SubType]>,
    /// Number of types in each recursive group, in section order.
    pub rec_group_lengths: Box<[u32]>,
}

impl TypeSection {
    #[inline]
    pub fn get(&self, index: TypeAddr) -> Option<&SubType> {
        self.types.get(index as usize)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

/// A type with optional declared subtyping.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#syntax-subtype>
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct SubType {
    pub is_final: bool,
    pub supertype: Option<TypeAddr>,
    pub composite: CompositeType,
}

impl SubType {
    #[inline]
    pub const fn as_func(&self) -> Option<&FuncType> {
        self.composite.as_func()
    }

    #[inline]
    pub const fn as_struct(&self) -> Option<&StructType> {
        self.composite.as_struct()
    }

    #[inline]
    pub const fn as_array(&self) -> Option<&ArrayType> {
        self.composite.as_array()
    }
}

/// A function, struct, or array type.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#syntax-comptype>
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum CompositeType {
    Func(FuncType),
    Struct(StructType),
    Array(ArrayType),
}

impl CompositeType {
    #[inline]
    pub const fn as_func(&self) -> Option<&FuncType> {
        match self {
            Self::Func(ty) => Some(ty),
            _ => None,
        }
    }

    #[inline]
    pub const fn as_struct(&self) -> Option<&StructType> {
        match self {
            Self::Struct(ty) => Some(ty),
            _ => None,
        }
    }

    #[inline]
    pub const fn as_array(&self) -> Option<&ArrayType> {
        match self {
            Self::Array(ty) => Some(ty),
            _ => None,
        }
    }
}

/// The type of a WebAssembly function.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#function-types>
#[derive(Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct FuncType {
    data: Arc<[WasmType]>,
    param_count: u16,
}

impl FuncType {
    /// Create a new function type.
    pub fn new(params: &[WasmType], results: &[WasmType]) -> Self {
        let data: Box<[WasmType]> = params.iter().cloned().chain(results.iter().cloned()).collect();
        Self { data: data.into(), param_count: params.len() as u16 }
    }

    /// Get the parameter types of this function type.
    pub fn params(&self) -> &[WasmType] {
        &self.data[..self.param_count as usize]
    }

    /// Get the result types of this function type.
    pub fn results(&self) -> &[WasmType] {
        &self.data[self.param_count as usize..]
    }
}

/// A WebAssembly struct type.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#syntax-structtype>
#[derive(Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct StructType {
    pub fields: Box<[FieldType]>,
}

/// A WebAssembly array type.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#syntax-arraytype>
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ArrayType {
    pub field: FieldType,
}

/// A struct field or array element type.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#syntax-fieldtype>
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct FieldType {
    pub storage: StorageType,
    pub mutable: bool,
}

/// A field's packed or unpacked storage type.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#syntax-storagetype>
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum StorageType {
    I8,
    I16,
    Value(WasmType),
}

impl StorageType {
    /// Returns the unpacked WebAssembly value type used on the stack.
    pub const fn unpacked(self) -> WasmType {
        match self {
            Self::I8 | Self::I16 => WasmType::I32,
            Self::Value(ty) => ty,
        }
    }
}

#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_assignments, unused_variables))
))]
#![warn(rust_2018_idioms, unreachable_pub)]
#![no_std]
#![deny(unsafe_code)]

//! Types used by [`tinywasm`](https://docs.rs/tinywasm) and [`tinywasm_parser`](https://docs.rs/tinywasm_parser).

extern crate alloc;
use alloc::{boxed::Box, sync::Arc};
use core::{
    fmt::Debug,
    ops::{Deref, Range},
};

// Memory defaults
const MEM_PAGE_SIZE: u64 = 65536;
const MAX_MEMORY_SIZE: u64 = 4294967296;

const fn max_page_count(page_size: u64) -> u64 {
    MAX_MEMORY_SIZE / page_size
}

// log for logging (optional).
#[cfg(feature = "log")]
#[allow(clippy::single_component_path_imports, unused_imports)]
use log;

// noop fallback if logging is disabled.
#[cfg(not(feature = "log"))]
#[allow(unused_imports, unused_macros)]
pub(crate) mod log {
    macro_rules! debug    ( ($($tt:tt)*) => {{}} );
    macro_rules! info    ( ($($tt:tt)*) => {{}} );
    macro_rules! error    ( ($($tt:tt)*) => {{}} );
    pub(crate) use debug;
    pub(crate) use error;
    pub(crate) use info;
}

mod instructions;
mod value;
pub use instructions::*;
pub use value::*;

#[cfg(feature = "archive")]
pub mod archive;

#[cfg(not(feature = "archive"))]
pub mod archive {
    #[cfg_attr(feature = "debug", derive(Debug))]
    pub enum TwasmError {}
    impl core::fmt::Display for TwasmError {
        fn fmt(&self, _: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            Err(core::fmt::Error)
        }
    }
    impl core::error::Error for TwasmError {}
}

/// A `TinyWasm` WebAssembly Module
///
/// This is the internal representation of a WebAssembly module in `TinyWasm`.
/// `TinyWasmModules` are validated before being created, so they are guaranteed to be valid (as long as they were created by `TinyWasm`).
/// This means you should not trust a `TinyWasmModule` created by a third party to be valid.
#[derive(Clone, Default, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct TinyWasmModule {
    /// Optional address of the start function
    ///
    /// Corresponds to the `start` section of the original WebAssembly module.
    pub start_func: Option<FuncAddr>,

    /// Optimized and validated WebAssembly functions
    ///
    /// Contains data from to the `code`, `func`, and `type` sections of the original WebAssembly module.
    pub funcs: ArcSlice<WasmFunction>,

    /// A vector of type definitions, indexed by `TypeAddr`
    ///
    /// Corresponds to the `type` section of the original WebAssembly module.
    pub func_types: ArcSlice<FuncType>,

    /// Exported items of the WebAssembly module.
    ///
    /// Corresponds to the `export` section of the original WebAssembly module.
    pub exports: ArcSlice<Export>,

    /// Global components of the WebAssembly module.
    ///
    /// Corresponds to the `global` section of the original WebAssembly module.
    pub globals: ArcSlice<Global>,

    /// Table components of the WebAssembly module used to initialize tables.
    ///
    /// Corresponds to the `table` section of the original WebAssembly module.
    pub table_types: ArcSlice<TableType>,

    /// Memory components of the WebAssembly module used to initialize memories.
    ///
    /// Corresponds to the `memory` section of the original WebAssembly module.
    pub memory_types: ArcSlice<MemoryType>,

    /// Imports of the WebAssembly module.
    ///
    /// Corresponds to the `import` section of the original WebAssembly module.
    pub imports: ArcSlice<Import>,

    /// Data segments of the WebAssembly module.
    ///
    /// Corresponds to the `data` section of the original WebAssembly module.
    pub data: ArcSlice<Data>,

    /// Element segments of the WebAssembly module.
    ///
    /// Corresponds to the `elem` section of the original WebAssembly module.
    pub elements: ArcSlice<Element>,
}

/// A WebAssembly External Kind.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#external-types>
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ExternalKind {
    /// A WebAssembly Function.
    Func,
    /// A WebAssembly Table.
    Table,
    /// A WebAssembly Memory.
    Memory,
    /// A WebAssembly Global.
    Global,
}

/// A WebAssembly Address.
///
/// These are indexes into the respective stores.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#addresses>
pub type Addr = u32;

// aliases for clarity
pub type FuncAddr = Addr;
pub type TableAddr = Addr;
pub type MemAddr = Addr;
pub type GlobalAddr = Addr;
pub type ElemAddr = Addr;
pub type DataAddr = Addr;
pub type ExternAddr = Addr;
pub type ConstIdx = Addr;

// additional internal addresses
pub type TypeAddr = Addr;
pub type LocalAddr = u16; // there can't be more than 50.000 locals in a function
pub type ModuleInstanceAddr = Addr;

/// A WebAssembly External Value.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#external-values>
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum ExternVal {
    Func(FuncAddr),
    Table(TableAddr),
    Memory(MemAddr),
    Global(GlobalAddr),
}

impl ExternVal {
    #[inline]
    pub const fn kind(&self) -> ExternalKind {
        match self {
            Self::Func(_) => ExternalKind::Func,
            Self::Table(_) => ExternalKind::Table,
            Self::Memory(_) => ExternalKind::Memory,
            Self::Global(_) => ExternalKind::Global,
        }
    }

    #[inline]
    pub const fn new(kind: ExternalKind, addr: Addr) -> Self {
        match kind {
            ExternalKind::Func => Self::Func(addr),
            ExternalKind::Table => Self::Table(addr),
            ExternalKind::Memory => Self::Memory(addr),
            ExternalKind::Global => Self::Global(addr),
        }
    }
}

/// The type of a WebAssembly Function.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#function-types>
#[derive(Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct FuncType {
    pub params: Box<[ValType]>,
    pub results: Box<[ValType]>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueCounts {
    pub c32: u32,
    pub c64: u32,
    pub c128: u32,
    pub cref: u32,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueCountsSmall {
    pub c32: u16,
    pub c64: u16,
    pub c128: u16,
    pub cref: u16,
}

impl<'a, T: IntoIterator<Item = &'a ValType>> From<T> for ValueCounts {
    #[inline]
    fn from(types: T) -> Self {
        let mut counts = Self::default();
        for ty in types {
            match ty {
                ValType::I32 | ValType::F32 => counts.c32 += 1,
                ValType::I64 | ValType::F64 => counts.c64 += 1,
                ValType::V128 => counts.c128 += 1,
                ValType::RefExtern | ValType::RefFunc => counts.cref += 1,
            }
        }
        counts
    }
}

impl<'a, T: IntoIterator<Item = &'a ValType>> From<T> for ValueCountsSmall {
    #[inline]
    fn from(types: T) -> Self {
        let mut counts = Self::default();
        for ty in types {
            match ty {
                ValType::I32 | ValType::F32 => counts.c32 += 1,
                ValType::I64 | ValType::F64 => counts.c64 += 1,
                ValType::V128 => counts.c128 += 1,
                ValType::RefExtern | ValType::RefFunc => counts.cref += 1,
            }
        }
        counts
    }
}

#[derive(Clone, PartialEq, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct WasmFunction {
    pub instructions: ArcSlice<Instruction>,
    pub data: WasmFunctionData,
    pub locals: ValueCountsSmall,
    pub params: ValueCountsSmall,
    pub ty: FuncType,
}

#[derive(Clone, PartialEq)]
#[doc(hidden)]
// wrapper around Arc<[T]> to support serde serialization and deserialization
pub struct ArcSlice<T>(pub Arc<[T]>);

impl<T: Debug> Debug for ArcSlice<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.as_ref().fmt(f)
    }
}

impl<T> From<alloc::vec::Vec<T>> for ArcSlice<T> {
    fn from(vec: alloc::vec::Vec<T>) -> Self {
        Self(Arc::from(vec))
    }
}

impl<T> Default for ArcSlice<T> {
    fn default() -> Self {
        Self(Arc::from([]))
    }
}

impl<T> Deref for ArcSlice<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[cfg(feature = "archive")]
impl<T: serde::Serialize> serde::Serialize for ArcSlice<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_ref().serialize(serializer)
    }
}

#[cfg(feature = "archive")]
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for ArcSlice<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let vec: alloc::vec::Vec<T> = alloc::vec::Vec::deserialize(deserializer)?;
        Ok(Self(Arc::from(vec)))
    }
}

#[derive(Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct WasmFunctionData {
    pub v128_constants: Box<[i128]>,
}

/// A WebAssembly Module Export
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct Export {
    /// The name of the export.
    pub name: Box<str>,
    /// The kind of the export.
    pub kind: ExternalKind,
    /// The index of the exported item.
    pub index: u32,
}

#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct Global {
    pub ty: GlobalType,
    pub init: ConstInstruction,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct GlobalType {
    pub mutable: bool,
    pub ty: ValType,
}

#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct TableType {
    pub element_type: ValType,
    pub size_initial: u32,
    pub size_max: Option<u32>,
}

impl TableType {
    pub fn empty() -> Self {
        Self { element_type: ValType::RefFunc, size_initial: 0, size_max: None }
    }

    pub fn new(element_type: ValType, size_initial: u32, size_max: Option<u32>) -> Self {
        Self { element_type, size_initial, size_max }
    }
}

/// Represents a memory's type.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct MemoryType {
    arch: MemoryArch,
    page_count_initial: u64,
    page_count_max: Option<u64>,
    page_size: Option<u64>,
}

impl MemoryType {
    pub const fn new(
        arch: MemoryArch,
        page_count_initial: u64,
        page_count_max: Option<u64>,
        page_size: Option<u64>,
    ) -> Self {
        Self { arch, page_count_initial, page_count_max, page_size }
    }

    #[inline]
    pub const fn arch(&self) -> MemoryArch {
        self.arch
    }

    #[inline]
    pub const fn page_count_initial(&self) -> u64 {
        self.page_count_initial
    }

    #[inline]
    pub const fn page_count_max(&self) -> u64 {
        if let Some(page_count_max) = self.page_count_max { page_count_max } else { max_page_count(self.page_size()) }
    }

    #[inline]
    pub const fn page_size(&self) -> u64 {
        if let Some(page_size) = self.page_size { page_size } else { MEM_PAGE_SIZE }
    }

    #[inline]
    pub const fn initial_size(&self) -> u64 {
        self.page_count_initial * self.page_size()
    }

    #[inline]
    pub const fn max_size(&self) -> u64 {
        self.page_count_max() * self.page_size()
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum MemoryArch {
    I32,
    I64,
}

#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct Import {
    pub module: Box<str>,
    pub name: Box<str>,
    pub kind: ImportKind,
}

#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ImportKind {
    Function(TypeAddr),
    Table(TableType),
    Memory(MemoryType),
    Global(GlobalType),
}

impl From<&ImportKind> for ExternalKind {
    fn from(kind: &ImportKind) -> Self {
        match kind {
            ImportKind::Function(_) => Self::Func,
            ImportKind::Table(_) => Self::Table,
            ImportKind::Memory(_) => Self::Memory,
            ImportKind::Global(_) => Self::Global,
        }
    }
}

#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct Data {
    pub data: Box<[u8]>,
    pub range: Range<usize>,
    pub kind: DataKind,
}

#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum DataKind {
    Active { mem: MemAddr, offset: ConstInstruction },
    Passive,
}

#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct Element {
    pub kind: ElementKind,
    pub items: Box<[ElementItem]>,
    pub range: Range<usize>,
    pub ty: ValType,
}

#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ElementKind {
    Passive,
    Active { table: TableAddr, offset: ConstInstruction },
    Declared,
}

#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ElementItem {
    Func(FuncAddr),
    Expr(ConstInstruction),
}

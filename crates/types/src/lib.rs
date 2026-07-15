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
use core::ops::{Deref, Range};

// Memory defaults
const MEM_PAGE_SIZE: u64 = 65536;
const MAX_MEMORY_SIZE: u64 = 4294967296;

const fn max_page_count(page_size: u64) -> u64 {
    MAX_MEMORY_SIZE / page_size
}

mod instructions;
mod reference;
mod value;
pub use instructions::*;
pub use reference::*;
pub use value::*;

#[cfg(feature = "archive")]
pub mod archive;

#[cfg(not(feature = "archive"))]
pub mod archive {
    #[derive(Debug, PartialEq, Eq)]
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
/// [`Module`] are validated before being created, so they are guaranteed to be valid (as long as they were created by `TinyWasm`).
/// This means you should not trust a [`Module`] created by a third party to be valid.
#[derive(Clone, Default, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct Module(Arc<ModuleInner>);

impl From<ModuleInner> for Module {
    fn from(inner: ModuleInner) -> Self {
        Self(Arc::new(inner))
    }
}

impl Deref for Module {
    type Target = ModuleInner;

    fn deref(&self) -> &ModuleInner {
        &self.0
    }
}

#[doc(hidden)]
#[derive(Clone, Default, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ModuleInner {
    /// Optional address of the start function
    ///
    /// Corresponds to the `start` section of the original WebAssembly module.
    pub start_func: Option<FuncAddr>,

    /// Optimized and validated WebAssembly functions
    ///
    /// Contains data from to the `code`, `func`, and `type` sections of the original WebAssembly module.
    pub funcs: Box<[Arc<WasmFunction>]>,

    /// A vector of type definitions, indexed by `TypeAddr`
    ///
    /// Corresponds to the `type` section of the original WebAssembly module.
    pub func_types: Arc<[Arc<FuncType>]>,

    /// Function index to type index mapping in module index space, including imports.
    pub func_type_idxs: Arc<[u32]>,

    /// Exported items of the WebAssembly module.
    ///
    /// Corresponds to the `export` section of the original WebAssembly module.
    pub exports: Arc<[Export]>,

    /// Global components of the WebAssembly module.
    ///
    /// Corresponds to the `global` section of the original WebAssembly module.
    pub globals: Box<[Global]>,

    /// Table components of the WebAssembly module used to initialize tables.
    ///
    /// Corresponds to the `table` section of the original WebAssembly module.
    pub table_types: Box<[TableType]>,

    /// Memory components of the WebAssembly module used to initialize memories.
    ///
    /// Corresponds to the `memory` section of the original WebAssembly module.
    pub memory_types: Box<[MemoryType]>,

    /// Imports of the WebAssembly module.
    ///
    /// Corresponds to the `import` section of the original WebAssembly module.
    pub imports: Box<[Import]>,

    /// Data segments of the WebAssembly module.
    ///
    /// Corresponds to the `data` section of the original WebAssembly module.
    pub data: Box<[Data]>,

    /// Element segments of the WebAssembly module.
    ///
    /// Corresponds to the `elem` section of the original WebAssembly module.
    pub elements: Box<[Element]>,

    /// How instantiation should prepare the module's local memories.
    pub local_memory_allocation: LocalMemoryAllocation,
}

impl Module {
    /// Returns an iterator over the module's import descriptors.
    ///
    /// The returned data mirrors the module's import section and preserves order.
    pub fn imports(&self) -> impl Iterator<Item = ModuleImport<'_>> {
        self.0.imports.iter().filter_map(|import| {
            let ty = match &import.kind {
                ImportKind::Function(type_idx) => Some(ImportType::Func(self.0.func_types.get(*type_idx as usize)?)),
                ImportKind::Table(table_ty) => Some(ImportType::Table(table_ty)),
                ImportKind::Memory(memory_ty) => Some(ImportType::Memory(memory_ty)),
                ImportKind::Global(global_ty) => Some(ImportType::Global(global_ty)),
            }?;

            Some(ModuleImport { module: import.module.as_ref(), name: import.name.as_ref(), ty })
        })
    }

    /// Returns an iterator over the module's export descriptors.
    ///
    /// The returned data mirrors the module's export section and preserves order.
    pub fn exports(&self) -> impl Iterator<Item = ModuleExport<'_>> {
        fn imported_count(module: &ModuleInner, kind: ExternalKind) -> usize {
            module
                .imports
                .iter()
                .filter(|import| {
                    matches!(
                        (kind, &import.kind),
                        (ExternalKind::Func, ImportKind::Function(_))
                            | (ExternalKind::Table, ImportKind::Table(_))
                            | (ExternalKind::Memory, ImportKind::Memory(_))
                            | (ExternalKind::Global, ImportKind::Global(_))
                    )
                })
                .count()
        }

        fn imported_type(module: &ModuleInner, kind: ExternalKind, index: usize) -> Option<ExportType<'_>> {
            let mut imports = module.imports.iter().filter(|import| {
                matches!(
                    (kind, &import.kind),
                    (ExternalKind::Func, ImportKind::Function(_))
                        | (ExternalKind::Table, ImportKind::Table(_))
                        | (ExternalKind::Memory, ImportKind::Memory(_))
                        | (ExternalKind::Global, ImportKind::Global(_))
                )
            });
            let import = imports.nth(index)?;

            match &import.kind {
                ImportKind::Function(type_idx) => Some(ExportType::Func(module.func_types.get(*type_idx as usize)?)),
                ImportKind::Table(table_ty) => Some(ExportType::Table(table_ty)),
                ImportKind::Memory(memory_ty) => Some(ExportType::Memory(memory_ty)),
                ImportKind::Global(global_ty) => Some(ExportType::Global(global_ty)),
            }
        }

        self.0.exports.iter().filter_map(move |export| {
            let idx = export.index as usize;
            let ty = match export.kind {
                ExternalKind::Func => {
                    let imported_funcs = imported_count(&self.0, ExternalKind::Func);
                    if idx < imported_funcs {
                        imported_type(&self.0, ExternalKind::Func, idx)?
                    } else {
                        ExportType::Func(&self.0.funcs.get(idx - imported_funcs)?.ty)
                    }
                }
                ExternalKind::Table => {
                    let imported_tables = imported_count(&self.0, ExternalKind::Table);
                    if idx < imported_tables {
                        imported_type(&self.0, ExternalKind::Table, idx)?
                    } else {
                        ExportType::Table(self.0.table_types.get(idx - imported_tables)?)
                    }
                }
                ExternalKind::Memory => {
                    let imported_memories = imported_count(&self.0, ExternalKind::Memory);
                    if idx < imported_memories {
                        imported_type(&self.0, ExternalKind::Memory, idx)?
                    } else {
                        ExportType::Memory(self.0.memory_types.get(idx - imported_memories)?)
                    }
                }
                ExternalKind::Global => {
                    let imported_globals = imported_count(&self.0, ExternalKind::Global);
                    if idx < imported_globals {
                        imported_type(&self.0, ExternalKind::Global, idx)?
                    } else {
                        ExportType::Global(&self.0.globals.get(idx - imported_globals)?.ty)
                    }
                }
            };

            Some(ModuleExport { name: export.name.as_ref(), ty })
        })
    }
}

/// A module export descriptor.
pub struct ModuleExport<'a> {
    /// Export name.
    pub name: &'a str,
    /// Export type.
    pub ty: ExportType<'a>,
}

/// A module import descriptor.
pub struct ModuleImport<'a> {
    /// Importing module name.
    pub module: &'a str,
    /// Import name.
    pub name: &'a str,
    /// Import type.
    pub ty: ImportType<'a>,
}

/// Imported entity type.
pub enum ImportType<'a> {
    /// Imported function type.
    Func(&'a FuncType),
    /// Imported table type.
    Table(&'a TableType),
    /// Imported memory type.
    Memory(&'a MemoryType),
    /// Imported global type.
    Global(&'a GlobalType),
}

/// Exported entity type.
pub enum ExportType<'a> {
    /// Exported function type.
    Func(&'a FuncType),
    /// Exported table type.
    Table(&'a TableType),
    /// Exported memory type.
    Memory(&'a MemoryType),
    /// Exported global type.
    Global(&'a GlobalType),
}

/// How instantiation should prepare local memories declared by the module.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum LocalMemoryAllocation {
    /// The module's local memories are unobservable and can be skipped entirely.
    #[default]
    Skip,
    /// The module's local memories may be observed through exports, but can be delayed until first use.
    Lazy,
    /// The module's local memories must be allocated during instantiation.
    Eager,
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
    data: Box<[WasmType]>,
    param_count: u16,
}

impl FuncType {
    /// Create a new function type.
    pub fn new(params: &[WasmType], results: &[WasmType]) -> Self {
        let data: Box<[WasmType]> = params.iter().cloned().chain(results.iter().cloned()).collect();
        Self { data, param_count: params.len() as u16 }
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

#[derive(Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueCounts {
    pub c32: u16,
    pub c64: u16,
    pub c128: u16,
}

impl ValueCounts {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.c32 == 0 && self.c64 == 0 && self.c128 == 0
    }
}

impl<'a> FromIterator<&'a WasmType> for ValueCounts {
    #[inline]
    fn from_iter<I: IntoIterator<Item = &'a WasmType>>(iter: I) -> Self {
        let mut counts = Self::default();

        for ty in iter {
            match ty {
                WasmType::I32 | WasmType::F32 | WasmType::RefExtern | WasmType::RefFunc => counts.c32 += 1,
                WasmType::I64 | WasmType::F64 => counts.c64 += 1,
                WasmType::V128 => counts.c128 += 1,
            }
        }
        counts
    }
}

#[derive(Clone, PartialEq, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct WasmFunction {
    pub instructions: Box<[Instruction]>,
    pub data: WasmFunctionData,
    pub locals: ValueCounts,
    pub params: ValueCounts,
    pub results: ValueCounts,
    pub ty: Arc<FuncType>,
}

#[derive(Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct WasmFunctionData {
    pub v128_constants: Box<[[u8; 16]]>,
    pub branch_table_targets: Box<[u32]>,
}

impl WasmFunctionData {
    /// Panics if `idx` is out of bounds.
    #[inline(always)]
    pub fn v128_const(&self, idx: ConstIdx) -> [u8; 16] {
        *self.v128_constants.get(idx as usize).unwrap_or_else(|| unreachable!("invalid v128 constant index: {idx}"))
    }
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
    pub init: Box<[ConstInstruction]>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct GlobalType {
    pub mutable: bool,
    pub ty: WasmType,
}

impl GlobalType {
    /// Create a new global type.
    pub const fn new(ty: WasmType, mutable: bool) -> Self {
        Self { mutable, ty }
    }

    /// Set a different value type.
    pub const fn with_ty(mut self, ty: WasmType) -> Self {
        self.ty = ty;
        self
    }

    /// Set global mutability.
    pub const fn with_mutable(mut self, mutable: bool) -> Self {
        self.mutable = mutable;
        self
    }
}

impl Default for GlobalType {
    fn default() -> Self {
        Self::new(WasmType::I32, false)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct TableType {
    arch: MemoryArch,
    pub element_type: WasmType,
    pub size_initial: u64,
    pub size_max: Option<u64>,
}

impl TableType {
    pub const fn empty() -> Self {
        Self::new(WasmType::RefFunc, 0, None)
    }

    /// Create a table with 32-bit indices.
    pub const fn new(element_type: WasmType, size_initial: u64, size_max: Option<u64>) -> Self {
        Self { arch: MemoryArch::I32, element_type, size_initial, size_max }
    }

    /// Create a table with 64-bit indices.
    pub const fn new64(element_type: WasmType, size_initial: u64, size_max: Option<u64>) -> Self {
        Self { arch: MemoryArch::I64, element_type, size_initial, size_max }
    }

    #[inline]
    pub const fn arch(&self) -> MemoryArch {
        self.arch
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
    /// Create a new memory type.
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

    /// Set a different memory architecture.
    pub const fn with_arch(mut self, arch: MemoryArch) -> Self {
        self.arch = arch;
        self
    }

    /// Set a different initial page count.
    pub const fn with_page_count_initial(mut self, page_count_initial: u64) -> Self {
        self.page_count_initial = page_count_initial;
        self
    }

    /// Set a different maximum page count.
    pub const fn with_page_count_max(mut self, page_count_max: Option<u64>) -> Self {
        self.page_count_max = page_count_max;
        self
    }

    /// Set a different page size.
    pub const fn with_page_size(mut self, page_size: Option<u64>) -> Self {
        self.page_size = page_size;
        self
    }
}

impl Default for MemoryType {
    fn default() -> Self {
        Self::new(MemoryArch::I32, 0, None, None)
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
    Active { mem: MemAddr, offset: Box<[ConstInstruction]> },
    Passive,
}

#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct Element {
    pub kind: ElementKind,
    pub items: Box<[ElementItem]>,
    pub range: Range<usize>,
    pub ty: WasmType,
}

#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ElementKind {
    Passive,
    Active { table: TableAddr, offset: Box<[ConstInstruction]> },
    Declared,
}

#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ElementItem {
    Func(FuncAddr),
    Expr(Box<[ConstInstruction]>),
}

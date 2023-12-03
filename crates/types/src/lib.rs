extern crate alloc;

mod instructions;
use core::fmt::Debug;

pub use instructions::*;

#[derive(Debug)]
pub struct TinyWasmModule {
    pub version: Option<u16>,
    pub start_func: Option<FuncAddr>,

    pub types: Box<[FuncType]>,
    pub funcs: Box<[Function]>,
    pub exports: Box<[Export]>,
    // pub tables: Option<TableType>,
    // pub memories: Option<MemoryType>,
    // pub globals: Option<GlobalType>,
    // pub elements: Option<ElementSectionReader<'a>>,
    // pub imports: Option<ImportSectionReader<'a>>,
    // pub data_segments: Option<DataSectionReader<'a>>,
}

/// A WebAssembly value.
/// See https://webassembly.github.io/spec/core/syntax/types.html#value-types
#[derive(Clone, PartialEq)]
pub enum WasmValue {
    // Num types
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),

    // Vec types
    V128(i128),
}

impl Debug for WasmValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmValue::I32(i) => write!(f, "i32({})", i),
            WasmValue::I64(i) => write!(f, "i64({})", i),
            WasmValue::F32(i) => write!(f, "f32({})", i),
            WasmValue::F64(i) => write!(f, "f64({})", i),
            WasmValue::V128(i) => write!(f, "v128({})", i),
        }
    }
}

impl WasmValue {
    pub fn val_type(&self) -> ValType {
        match self {
            Self::I32(_) => ValType::I32,
            Self::I64(_) => ValType::I64,
            Self::F32(_) => ValType::F32,
            Self::F64(_) => ValType::F64,
            Self::V128(_) => ValType::V128,
        }
    }
}

/// Type of a WebAssembly value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
    V128,
    FuncRef,
    ExternRef,
}

/// A WebAssembly External Kind.
/// See https://webassembly.github.io/spec/core/syntax/types.html#external-types
#[derive(Debug, Clone, PartialEq)]
pub enum ExternalKind {
    Func,
    Table,
    Memory,
    Global,
}

/// A WebAssembly Address.
/// These are indexes into the respective stores.
/// See https://webassembly.github.io/spec/core/exec/runtime.html#addresses
pub type Addr = u32;
pub type FuncAddr = Addr;
pub type TableAddr = Addr;
pub type MemAddr = Addr;
pub type GlobalAddr = Addr;
pub type ElmAddr = Addr;
pub type DataAddr = Addr;
pub type ExternAddr = Addr;
// additional internal addresses
pub type TypeAddr = Addr;
pub type LocalAddr = Addr;
pub type LabelAddr = Addr;

/// A WebAssembly Export Instance.
/// https://webassembly.github.io/spec/core/exec/runtime.html#export-instances
#[derive(Debug)]
pub struct ExportInst {
    pub name: String,
    pub value: ExternVal,
}

/// A WebAssembly External Value.
/// https://webassembly.github.io/spec/core/exec/runtime.html#external-values
#[derive(Debug)]
pub enum ExternVal {
    Func(FuncAddr),
    Table(TableAddr),
    Mem(MemAddr),
    Global(GlobalAddr),
}

/// The type of a WebAssembly Function.
/// See https://webassembly.github.io/spec/core/syntax/types.html#function-types
#[derive(Debug, Clone, PartialEq)]
pub struct FuncType {
    pub params: Box<[ValType]>,
    pub results: Box<[ValType]>,
}

/// A WebAssembly Function
#[derive(Debug)]
pub struct Function {
    pub ty: TypeAddr,
    pub locals: Box<[ValType]>,
    pub instructions: Box<[Instruction]>,
}

/// A WebAssembly Module Export
#[derive(Debug)]
pub struct Export {
    /// The name of the export.
    pub name: Box<str>,
    /// The kind of the export.
    pub kind: ExternalKind,
    /// The index of the exported item.
    pub index: u32,
}

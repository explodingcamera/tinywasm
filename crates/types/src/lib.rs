#![no_std]
#![forbid(unsafe_code)]
#![doc(test(
    no_crate_inject,
    attr(
        deny(warnings, rust_2018_idioms),
        allow(dead_code, unused_assignments, unused_variables)
    )
))]
#![warn(missing_debug_implementations, rust_2018_idioms, unreachable_pub)]

//! Types used by [`tinywasm`](https://docs.rs/tinywasm) and [`tinywasm_parser`](https://docs.rs/tinywasm_parser).

extern crate alloc;

// log for logging (optional).
// #[cfg(feature = "logging")]
// #[allow(unused_imports)]
// use log;

// #[cfg(not(feature = "logging"))]
// #[macro_use]
// pub(crate) mod log {
//     // macro_rules! debug    ( ($($tt:tt)*) => {{}} );
//     // pub(crate) use debug;
// }

mod instructions;
use core::{fmt::Debug, ops::Range};

use alloc::boxed::Box;
pub use instructions::*;

/// A TinyWasm WebAssembly Module
///
/// This is the internal representation of a WebAssembly module in TinyWasm.
/// TinyWasmModules are validated before being created, so they are guaranteed to be valid (as long as they were created by TinyWasm).
/// This means you should not trust a TinyWasmModule created by a third party to be valid.
#[derive(Debug, Clone)]
pub struct TinyWasmModule {
    /// The version of the WebAssembly module.
    pub version: Option<u16>,

    /// The start function of the WebAssembly module.
    pub start_func: Option<FuncAddr>,

    /// The functions of the WebAssembly module.
    pub funcs: Box<[Function]>,

    /// The types of the WebAssembly module.
    pub func_types: Box<[FuncType]>,

    /// The exports of the WebAssembly module.
    pub exports: Box<[Export]>,

    /// The tables of the WebAssembly module.
    pub globals: Box<[Global]>,

    /// The tables of the WebAssembly module.
    pub table_types: Box<[TableType]>,

    /// The memories of the WebAssembly module.
    pub memory_types: Box<[MemoryType]>,

    /// The imports of the WebAssembly module.
    pub imports: Box<[Import]>,

    /// Data segments of the WebAssembly module.
    pub data: Box<[Data]>,

    /// Element segments of the WebAssembly module.
    pub elements: Box<[Element]>,
}

/// A WebAssembly value.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#value-types>
#[derive(Clone, PartialEq, Copy)]
pub enum WasmValue {
    // Num types
    /// A 32-bit integer.
    I32(i32),
    /// A 64-bit integer.
    I64(i64),
    /// A 32-bit float.
    F32(f32),
    /// A 64-bit float.
    F64(f64),
    // Vec types
    // V128(i128),
    // RefExtern(ExternAddr),
    // RefHost(FuncAddr),
    RefNull(ValType),
}

impl WasmValue {
    pub fn const_instr(&self) -> ConstInstruction {
        match self {
            Self::I32(i) => ConstInstruction::I32Const(*i),
            Self::I64(i) => ConstInstruction::I64Const(*i),
            Self::F32(i) => ConstInstruction::F32Const(*i),
            Self::F64(i) => ConstInstruction::F64Const(*i),
            Self::RefNull(ty) => ConstInstruction::RefNull(*ty),
            // Self::RefExtern(addr) => ConstInstruction::RefExtern(*addr),
            // _ => unimplemented!("const_instr for {:?}", self),
        }
    }

    /// Get the default value for a given type.
    pub fn default_for(ty: ValType) -> Self {
        match ty {
            ValType::I32 => Self::I32(0),
            ValType::I64 => Self::I64(0),
            ValType::F32 => Self::F32(0.0),
            ValType::F64 => Self::F64(0.0),
            ValType::V128 => unimplemented!("V128 is not yet supported"),
            ValType::FuncRef => unimplemented!("FuncRef is not yet supported"),
            ValType::ExternRef => unimplemented!("ExternRef is not yet supported"),
        }
    }

    pub fn eq_loose(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::I32(a), Self::I32(b)) => a == b,
            (Self::I64(a), Self::I64(b)) => a == b,
            (Self::F32(a), Self::F32(b)) => {
                if a.is_nan() && b.is_nan() {
                    true // Both are NaN, treat them as equal
                } else {
                    a.to_bits() == b.to_bits()
                }
            }
            (Self::F64(a), Self::F64(b)) => {
                if a.is_nan() && b.is_nan() {
                    true // Both are NaN, treat them as equal
                } else {
                    a.to_bits() == b.to_bits()
                }
            }
            _ => false,
        }
    }
}

impl From<i32> for WasmValue {
    fn from(i: i32) -> Self {
        Self::I32(i)
    }
}

impl From<i64> for WasmValue {
    fn from(i: i64) -> Self {
        Self::I64(i)
    }
}

impl From<f32> for WasmValue {
    fn from(i: f32) -> Self {
        Self::F32(i)
    }
}

impl From<f64> for WasmValue {
    fn from(i: f64) -> Self {
        Self::F64(i)
    }
}

// impl From<i128> for WasmValue {
//     fn from(i: i128) -> Self {
//         Self::V128(i)
//     }
// }

impl TryFrom<WasmValue> for i32 {
    type Error = ();

    fn try_from(value: WasmValue) -> Result<Self, Self::Error> {
        match value {
            WasmValue::I32(i) => Ok(i),
            _ => Err(()),
        }
    }
}

impl TryFrom<WasmValue> for i64 {
    type Error = ();

    fn try_from(value: WasmValue) -> Result<Self, Self::Error> {
        match value {
            WasmValue::I64(i) => Ok(i),
            _ => Err(()),
        }
    }
}

impl TryFrom<WasmValue> for f32 {
    type Error = ();

    fn try_from(value: WasmValue) -> Result<Self, Self::Error> {
        match value {
            WasmValue::F32(i) => Ok(i),
            _ => Err(()),
        }
    }
}

impl TryFrom<WasmValue> for f64 {
    type Error = ();

    fn try_from(value: WasmValue) -> Result<Self, Self::Error> {
        match value {
            WasmValue::F64(i) => Ok(i),
            _ => Err(()),
        }
    }
}

impl Debug for WasmValue {
    fn fmt(&self, f: &mut alloc::fmt::Formatter<'_>) -> alloc::fmt::Result {
        match self {
            WasmValue::I32(i) => write!(f, "i32({})", i),
            WasmValue::I64(i) => write!(f, "i64({})", i),
            WasmValue::F32(i) => write!(f, "f32({})", i),
            WasmValue::F64(i) => write!(f, "f64({})", i),
            WasmValue::RefNull(ty) => write!(f, "ref.null({:?})", ty),
            // WasmValue::V128(i) => write!(f, "v128({})", i),
        }
    }
}

impl WasmValue {
    /// Get the type of a [`WasmValue`]
    pub fn val_type(&self) -> ValType {
        match self {
            Self::I32(_) => ValType::I32,
            Self::I64(_) => ValType::I64,
            Self::F32(_) => ValType::F32,
            Self::F64(_) => ValType::F64,
            Self::RefNull(ty) => *ty,
            // Self::V128(_) => ValType::V128,
        }
    }
}

/// Type of a WebAssembly value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValType {
    /// A 32-bit integer.
    I32,
    /// A 64-bit integer.
    I64,
    /// A 32-bit float.
    F32,
    /// A 64-bit float.
    F64,
    /// A 128-bit vector.
    V128,
    /// A reference to a function.
    FuncRef,
    /// A reference to an external value.
    ExternRef,
}

impl ValType {
    pub fn default_value(&self) -> WasmValue {
        WasmValue::default_for(*self)
    }
}

/// A WebAssembly External Kind.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#external-types>
#[derive(Debug, Clone, PartialEq)]
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
pub type ModuleInstanceAddr = Addr;

/// A WebAssembly External Value.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#external-values>
#[derive(Debug)]
pub enum ExternVal {
    Func(FuncAddr),
    Table(TableAddr),
    Mem(MemAddr),
    Global(GlobalAddr),
}

/// The type of a WebAssembly Function.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#function-types>
#[derive(Debug, Clone, PartialEq)]
pub struct FuncType {
    pub params: Box<[ValType]>,
    pub results: Box<[ValType]>,
}

impl FuncType {
    /// Get the number of parameters of a function type.
    pub fn empty() -> Self {
        Self {
            params: Box::new([]),
            results: Box::new([]),
        }
    }
}

/// A WebAssembly Function
#[derive(Debug, Clone)]
pub struct Function {
    pub ty: TypeAddr,
    pub locals: Box<[ValType]>,
    pub instructions: Box<[Instruction]>,
}

/// A WebAssembly Module Export
#[derive(Debug, Clone)]
pub struct Export {
    /// The name of the export.
    pub name: Box<str>,
    /// The kind of the export.
    pub kind: ExternalKind,
    /// The index of the exported item.
    pub index: u32,
}

#[derive(Debug, Clone)]
pub struct Global {
    pub ty: GlobalType,
    pub init: ConstInstruction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlobalType {
    pub mutable: bool,
    pub ty: ValType,
}

#[derive(Debug, Clone)]
pub struct TableType {
    pub element_type: ValType,
    pub size_initial: u32,
    pub size_max: Option<u32>,
}

#[derive(Debug, Clone)]

/// Represents a memory's type.
#[derive(Copy, PartialEq, Eq, Hash)]
pub struct MemoryType {
    pub arch: MemoryArch,
    pub page_count_initial: u64,
    pub page_count_max: Option<u64>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum MemoryArch {
    I32,
    I64,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module: Box<str>,
    pub name: Box<str>,
    pub kind: ImportKind,
}

#[derive(Debug, Clone)]
pub enum ImportKind {
    Func(TypeAddr),
    Table(TableType),
    Mem(MemoryType),
    Global(GlobalType),
}

#[derive(Debug, Clone)]
pub struct Data {
    pub data: Box<[u8]>,
    pub range: Range<usize>,
    pub kind: DataKind,
}

#[derive(Debug, Clone)]
pub enum DataKind {
    Active { mem: MemAddr, offset: ConstInstruction },
    Passive,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub kind: ElementKind,
    pub items: Box<[ElementItem]>,
    pub range: Range<usize>,
    pub ty: ValType,
}

#[derive(Debug, Clone)]
pub enum ElementKind {
    Passive,
    Active { table: TableAddr, offset: ConstInstruction },
    Declared,
}

#[derive(Debug, Clone)]
pub enum ElementItem {
    Func(FuncAddr),
    Expr(ConstInstruction),
}

impl ElementItem {
    pub fn addr(&self) -> Option<FuncAddr> {
        match self {
            Self::Func(addr) => Some(*addr),
            Self::Expr(ConstInstruction::RefFunc(addr)) => Some(*addr),
            Self::Expr(ConstInstruction::RefNull(_ty)) => Some(0),
            _ => None,
        }
    }
}

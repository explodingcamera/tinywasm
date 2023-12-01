use alloc::string::String;

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

/// A WebAssembly Function Instance.
/// See https://webassembly.github.io/spec/core/exec/runtime.html#function-instances
// #[derive(Debug)]
// pub enum FuncInst {
//     Host(HostFunc),
//     Module(ModuleFunc),
// }
// #[derive(Debug)]
// pub struct HostFunc {
//     pub ty: FuncType,
//     pub hostcode: fn() -> (),
// }

// pub struct Func<'a> {
//     pub ty: FuncType,
//     pub locals: Vec<ValType>,
//     pub body: Vec<OperatorsIterator<'a>>,
// }

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

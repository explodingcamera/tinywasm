use alloc::{string::String, vec::Vec};
use wasmparser::{FuncType, OperatorsIterator, ValType};

/// A WebAssembly Label
pub struct Label(Addr);

/// A WebAssembly Address.
/// These are indexes into the respective stores.
/// See https://webassembly.github.io/spec/core/exec/runtime.html#addresses
pub type Addr = u32;
pub struct FuncAddr(pub Addr);
pub struct TableAddr(pub Addr);
pub struct MemAddr(pub Addr);
pub struct GlobalAddr(pub Addr);
pub struct ElmAddr(pub Addr);
pub struct DataAddr(pub Addr);
pub struct ExternAddr(pub Addr);

/// A WebAssembly Module Instance.
/// See https://webassembly.github.io/spec/core/exec/runtime.html#module-instances
pub struct ModuleInstance {
    pub types: Vec<FuncType>,
    pub func_addrs: Vec<FuncAddr>,
    pub table_addrs: Vec<TableAddr>,
    pub mem_addrs: Vec<MemAddr>,
    pub global_addrs: Vec<GlobalAddr>,
    pub elem_addrs: Vec<ElmAddr>,
    pub data_addrs: Vec<DataAddr>,
    pub exports: Vec<ExportInst>,
}

/// A WebAssembly Function Instance.
/// See https://webassembly.github.io/spec/core/exec/runtime.html#function-instances
pub enum FuncInst {
    Host(HostFunc),
    Module(ModuleFunc),
}
pub struct HostFunc {
    pub ty: FuncType,
    pub hostcode: fn() -> (),
}
pub struct ModuleFunc {
    pub ty: FuncType,
    pub module: ModuleInstance,
    pub code: FuncAddr,
}
pub struct Func<'a> {
    pub ty: FuncType,
    pub locals: Vec<ValType>,
    pub body: Vec<OperatorsIterator<'a>>,
}

/// A WebAssembly Export Instance.
/// https://webassembly.github.io/spec/core/exec/runtime.html#export-instances
pub struct ExportInst {
    pub name: String,
    pub value: ExternVal,
}

/// A WebAssembly External Value.
/// https://webassembly.github.io/spec/core/exec/runtime.html#external-values
pub enum ExternVal {
    Func(FuncAddr),
    Table(TableAddr),
    Mem(MemAddr),
    Global(GlobalAddr),
}

use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use log::info;
use tinywasm_types::{Export, ExternalKind, FuncAddr, FuncType, TinyWasmModule};

use crate::{
    runtime::Runtime,
    store::{self, StoreData},
    Error, Result, Store, WasmValue,
};

#[derive(Debug)]
pub struct Module {
    data: TinyWasmModule,
}

impl From<TinyWasmModule> for Module {
    fn from(data: TinyWasmModule) -> Self {
        Self { data }
    }
}

impl Module {
    pub fn parse_bytes(wasm: &[u8]) -> Result<Self> {
        let parser = tinywasm_parser::Parser::new();
        let data = parser.parse_module_bytes(wasm)?;
        Ok(data.into())
    }

    #[cfg(feature = "std")]
    pub fn parse_file(path: impl AsRef<crate::std::path::Path>) -> Result<Self> {
        let parser = tinywasm_parser::Parser::new();
        let data = parser.parse_module_file(path)?;
        Ok(data.into())
    }

    #[cfg(feature = "std")]
    pub fn parse_stream(stream: impl crate::std::io::Read) -> Result<Self> {
        let parser = tinywasm_parser::Parser::new();
        let data = parser.parse_module_stream(stream)?;
        Ok(data.into())
    }

    /// Instantiate the module in the given store
    /// See https://webassembly.github.io/spec/core/exec/modules.html#exec-instantiation
    /// Runs the start function if it exists
    /// If you want to run the start function yourself, use `ModuleInstance::new`
    pub fn instantiate(
        self,
        store: &mut Store,
        // imports: Option<()>,
    ) -> Result<ModuleInstance> {
        let i = ModuleInstance::new(store, self)?;
        let _ = i.start(store)?;
        Ok(i)
    }
}

/// A WebAssembly Module Instance.
/// Addrs are indices into the store's data structures.
/// See https://webassembly.github.io/spec/core/exec/runtime.html#module-instances
#[derive(Debug)]
pub struct ModuleInstance {
    pub(crate) func_start: Option<FuncAddr>,
    pub(crate) types: Box<[FuncType]>,
    pub(crate) exports: Box<[Export]>,
    // pub(crate) func_addrs: Vec<FuncAddr>,
    // pub table_addrs: Vec<TableAddr>,
    // pub mem_addrs: Vec<MemAddr>,
    // pub global_addrs: Vec<GlobalAddr>,
    // pub elem_addrs: Vec<ElmAddr>,
    // pub data_addrs: Vec<DataAddr>,
}

impl ModuleInstance {
    /// Get an exported function by name
    pub fn get_func(&self, store: &store::Store, name: &str) -> Result<FuncHandle> {
        let export = self
            .exports
            .iter()
            .find(|e| e.name == name.into() && e.kind == ExternalKind::Func)
            .ok_or(Error::Other(format!("export {} not found", name)))?;

        let func = store.get_func(export.index as usize)?;
        let ty = &self.types[func.ty as usize];

        Ok(FuncHandle {
            addr: export.index,
            module: &self,
            name: Some(name.to_string()),
            ty: ty.clone(),
        })
    }

    /// Get the start  function of the module
    pub fn get_start_func(&self, store: &store::Store) -> Result<Option<FuncHandle>> {
        let Some(func_addr) = self.func_start else {
            return Ok(None);
        };

        let func = store.get_func(func_addr as usize)?;
        let ty = &self.types[func.ty as usize];
        Ok(Some(FuncHandle {
            module: &self,
            addr: func_addr,
            ty: ty.clone(),
            name: None,
        }))
    }

    pub fn new(store: &mut Store, module: Module) -> Result<Self> {
        let store_data = StoreData {
            funcs: module.data.funcs,
        };

        store.initialize(store_data)?;

        Ok(Self {
            types: module.data.types,
            func_start: module.data.start_func,
            // table_addrs,
            // mem_addrs,
            // global_addrs,
            // elem_addrs,
            // data_addrs,
            exports: module.data.exports,
        })
    }

    /// Invoke the start function of the module
    /// Returns None if the module has no start function
    /// https://webassembly.github.io/spec/core/syntax/modules.html#syntax-start
    pub fn start(&self, store: &mut store::Store) -> Result<Option<()>> {
        let Some(func) = self.get_start_func(store)? else {
            return Ok(None);
        };

        let _ = func.call(store, vec![]);
        Ok(Some(()))
    }
}

#[derive(Debug)]
pub struct FuncHandle<'a> {
    module: &'a ModuleInstance,
    addr: FuncAddr,
    ty: FuncType,
    pub name: Option<String>,
}

impl<'a> FuncHandle<'a> {
    /// Call a function
    pub fn call(&self, store: &mut Store, params: Vec<WasmValue>) -> Result<Vec<WasmValue>> {
        let func = store.get_func(self.addr as usize)?;
        let func_ty = &self.ty;

        let mut runtime = Runtime::default();
        let stack = &mut runtime.stack;
        let locals = &mut stack.locals;
        locals.extend(params);

        let mut instrs = func.body.iter();

        while let Some(instr) = instrs.next() {
            use tinywasm_types::Instruction::*;
            match instr {
                LocalGet(local_index) => {
                    let val = &locals[*local_index as usize];
                    info!("local: {:#?}", val);
                    stack.value_stack.push(val.clone());
                }
                I64Add => {
                    let a = stack.value_stack.pop().unwrap();
                    let b = stack.value_stack.pop().unwrap();
                    let (WasmValue::I64(a), WasmValue::I64(b)) = (a, b) else {
                        panic!("Invalid type");
                    };
                    let c = WasmValue::I64(a + b);
                    stack.value_stack.push(c);
                }
                I32Add => {
                    let a = stack.value_stack.pop().unwrap();
                    let b = stack.value_stack.pop().unwrap();
                    let (WasmValue::I32(a), WasmValue::I32(b)) = (a, b) else {
                        panic!("Invalid type");
                    };
                    let c = WasmValue::I32(a + b);
                    stack.value_stack.push(c);
                }
                End => {
                    let res = func_ty
                        .results
                        .iter()
                        .map(|_| runtime.stack.value_stack.pop())
                        .collect::<Option<Vec<_>>>()
                        .ok_or(Error::Other(
                            "function did not return the correct number of values".into(),
                        ))?;

                    return Ok(res);
                }
                _ => todo!(),
            }
        }

        Err(Error::FuncDidNotReturn)
    }
}

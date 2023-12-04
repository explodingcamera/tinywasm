use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};
use tinywasm_types::{
    Export, ExternalKind, FuncAddr, FuncType, ModuleInstanceAddr, TinyWasmModule, ValType,
    WasmValue,
};

use crate::{runtime::Stack, store, Error, Result, Store};

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
        let idx = store.next_module_instance_idx();

        let func_addrs = store.add_funcs(self.data.funcs, idx);
        let instance = ModuleInstance::new(
            self.data.types,
            self.data.start_func,
            self.data.exports,
            func_addrs,
            idx,
        );

        store.add_instance(instance.clone())?;
        // let _ = instance.start(store)?;
        Ok(instance)
    }
}

/// A WebAssembly Module Instance.
/// Addrs are indices into the store's data structures.
/// See https://webassembly.github.io/spec/core/exec/runtime.html#module-instances
#[derive(Debug, Clone)]
pub struct ModuleInstance(Arc<ModuleInstanceInner>);

#[derive(Debug)]
struct ModuleInstanceInner {
    pub(crate) _idx: ModuleInstanceAddr,
    pub(crate) func_start: Option<FuncAddr>,
    pub(crate) types: Box<[FuncType]>,
    pub exports: ExportInstance,

    pub(crate) func_addrs: Vec<FuncAddr>,
    // pub table_addrs: Vec<TableAddr>,
    // pub mem_addrs: Vec<MemAddr>,
    // pub global_addrs: Vec<GlobalAddr>,
    // pub elem_addrs: Vec<ElmAddr>,
    // pub data_addrs: Vec<DataAddr>,
}

#[derive(Debug)]
pub struct ExportInstance(Box<[Export]>);

impl ExportInstance {
    pub fn func(&self, name: &str) -> Result<&Export> {
        self.0
            .iter()
            .find(|e| e.name == name.into() && e.kind == ExternalKind::Func)
            .ok_or(Error::Other(format!("export {} not found", name)))
    }
}

impl ModuleInstance {
    fn new(
        types: Box<[FuncType]>,
        func_start: Option<FuncAddr>,
        exports: Box<[Export]>,
        func_addrs: Vec<FuncAddr>,
        idx: ModuleInstanceAddr,
    ) -> Self {
        Self(Arc::new(ModuleInstanceInner {
            _idx: idx,
            types,
            func_start,
            func_addrs,
            exports: ExportInstance(exports),
        }))
    }

    /// Get an exported function by name
    pub fn get_func(&self, store: &store::Store, name: &str) -> Result<FuncHandle> {
        let export = self.0.exports.func(name)?;
        let func_addr = self.0.func_addrs[export.index as usize];
        let func = store.get_func(func_addr as usize)?;
        let ty = self.0.types[func.ty_addr() as usize].clone();

        Ok(FuncHandle {
            addr: export.index,
            _module: self.clone(),
            name: Some(name.to_string()),
            ty,
        })
    }

    /// Get the start  function of the module
    pub fn get_start_func(&mut self, store: &store::Store) -> Result<Option<FuncHandle>> {
        let Some(func_index) = self.0.func_start else {
            return Ok(None);
        };

        let func_addr = self.0.func_addrs[func_index as usize];
        let func = store.get_func(func_addr as usize)?;
        let ty = self.0.types[func.ty_addr() as usize].clone();

        Ok(Some(FuncHandle {
            _module: self.clone(),
            addr: func_addr,
            ty,
            name: None,
        }))
    }

    /// Invoke the start function of the module
    /// Returns None if the module has no start function
    /// https://webassembly.github.io/spec/core/syntax/modules.html#syntax-start
    pub fn start(&mut self, store: &mut store::Store) -> Result<Option<()>> {
        let Some(func) = self.get_start_func(store)? else {
            return Ok(None);
        };

        let _ = func.call(store, vec![]);
        Ok(Some(()))
    }
}

#[derive(Debug)]
pub struct FuncHandle {
    _module: ModuleInstance,
    addr: FuncAddr,
    ty: FuncType,
    pub name: Option<String>,
}

impl FuncHandle {
    /// Call a function
    pub fn call(&self, store: &mut Store, params: Vec<WasmValue>) -> Result<Vec<WasmValue>> {
        let func = store
            .data
            .funcs
            .get(self.addr as usize)
            .ok_or(Error::Other(format!("function {} not found", self.addr)))?;

        let func_ty = &self.ty;

        // check that params match func_ty params
        for (ty, param) in func_ty.params.iter().zip(params.clone()) {
            if ty != &param.val_type() {
                return Err(Error::Other(format!(
                    "param type mismatch: expected {:?}, got {:?}",
                    ty, param
                )));
            }
        }

        let mut local_types: Vec<ValType> = Vec::new();
        local_types.extend(func_ty.params.iter());
        local_types.extend(func.locals().iter());

        // let runtime = &mut store.runtime;

        let mut stack = Stack::default();
        stack.locals.extend(params);

        let instrs = func.instructions().iter();
        store.runtime.exec(&mut stack, instrs)?;

        let res = func_ty
            .results
            .iter()
            .map(|_| stack.value_stack.pop())
            .collect::<Option<Vec<_>>>()
            .ok_or(Error::Other(
                "function did not return the correct number of values".into(),
            ))?;

        Ok(res)
    }
}

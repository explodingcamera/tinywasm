use crate::{Imports, ModuleInstance, Result, Store};
use tinywasm_types::{ExternalKind, FuncType, TinyWasmModule};

fn imported_func_type(module: &TinyWasmModule, function_index: usize) -> Option<&FuncType> {
    let mut seen = 0usize;
    for import in module.imports.iter() {
        if let tinywasm_types::ImportKind::Function(type_idx) = import.kind {
            if seen == function_index {
                return module.func_types.get(type_idx as usize);
            }
            seen += 1;
        }
    }
    None
}

fn imported_global_type(module: &TinyWasmModule, global_index: usize) -> Option<&tinywasm_types::GlobalType> {
    let mut seen = 0usize;
    for import in module.imports.iter() {
        if let tinywasm_types::ImportKind::Global(global_ty) = &import.kind {
            if seen == global_index {
                return Some(global_ty);
            }
            seen += 1;
        }
    }
    None
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

/// A module export descriptor.
pub struct ModuleExport<'a> {
    /// Export name.
    pub name: &'a str,
    /// Export type.
    pub ty: ExportType<'a>,
}

/// Imported entity type.
pub enum ImportType<'a> {
    /// Imported function type.
    Func(&'a FuncType),
    /// Imported table type.
    Table(&'a tinywasm_types::TableType),
    /// Imported memory type.
    Memory(&'a tinywasm_types::MemoryType),
    /// Imported global type.
    Global(&'a tinywasm_types::GlobalType),
}

/// Exported entity type.
pub enum ExportType<'a> {
    /// Exported function type.
    Func(&'a FuncType),
    /// Exported table type.
    Table(&'a tinywasm_types::TableType),
    /// Exported memory type.
    Memory(&'a tinywasm_types::MemoryType),
    /// Exported global type.
    Global(&'a tinywasm_types::GlobalType),
}

/// A WebAssembly Module
///
/// See <https://webassembly.github.io/spec/core/syntax/modules.html#syntax-module>
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct Module(pub(crate) alloc::sync::Arc<TinyWasmModule>);

impl From<&TinyWasmModule> for Module {
    fn from(data: &TinyWasmModule) -> Self {
        Self(alloc::sync::Arc::new(data.clone()))
    }
}

impl From<TinyWasmModule> for Module {
    fn from(data: TinyWasmModule) -> Self {
        Self(alloc::sync::Arc::new(data))
    }
}

impl Module {
    #[cfg(feature = "parser")]
    /// Parse a module from bytes. Requires `parser` feature.
    pub fn parse_bytes(wasm: &[u8]) -> Result<Self> {
        let data = tinywasm_parser::Parser::new().parse_module_bytes(wasm)?;
        Ok(data.into())
    }

    #[cfg(all(feature = "parser", feature = "std"))]
    /// Parse a module from a file. Requires `parser` and `std` features.
    pub fn parse_file(path: impl AsRef<crate::std::path::Path> + Clone) -> Result<Self> {
        let data = tinywasm_parser::Parser::new().parse_module_file(path)?;
        Ok(data.into())
    }

    #[cfg(all(feature = "parser", feature = "std"))]
    /// Parse a module from a stream. Requires `parser` and `std` features.
    pub fn parse_stream(stream: impl crate::std::io::Read) -> Result<Self> {
        let data = tinywasm_parser::Parser::new().parse_module_stream(stream)?;
        Ok(data.into())
    }

    /// Instantiate the module in the given store
    ///
    /// Runs the start function if it exists
    ///
    /// If you want to run the start function yourself, use `ModuleInstance::instantiate`
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#exec-instantiation>
    pub fn instantiate(self, store: &mut Store, imports: Option<Imports>) -> Result<ModuleInstance> {
        let instance = ModuleInstance::instantiate(store, self, imports)?;
        let _ = instance.start(store)?;
        Ok(instance)
    }

    /// Returns an iterator over the module's import descriptors.
    ///
    /// The returned data mirrors the module's import section and preserves order.
    pub fn imports(&self) -> impl Iterator<Item = ModuleImport<'_>> {
        self.0.imports.iter().filter_map(|import| {
            let ty = match &import.kind {
                tinywasm_types::ImportKind::Function(type_idx) => {
                    Some(ImportType::Func(self.0.func_types.get(*type_idx as usize)?))
                }
                tinywasm_types::ImportKind::Table(table_ty) => Some(ImportType::Table(table_ty)),
                tinywasm_types::ImportKind::Memory(memory_ty) => Some(ImportType::Memory(memory_ty)),
                tinywasm_types::ImportKind::Global(global_ty) => Some(ImportType::Global(global_ty)),
            }?;

            Some(ModuleImport { module: import.module.as_ref(), name: import.name.as_ref(), ty })
        })
    }

    /// Returns an iterator over the module's export descriptors.
    ///
    /// The returned data mirrors the module's export section and preserves order.
    pub fn exports(&self) -> impl Iterator<Item = ModuleExport<'_>> {
        self.0.exports.iter().filter_map(|export| {
            let ty = match export.kind {
                ExternalKind::Func => {
                    let idx = export.index as usize;
                    let imported_funcs = self
                        .0
                        .imports
                        .iter()
                        .filter(|import| matches!(import.kind, tinywasm_types::ImportKind::Function(_)))
                        .count();

                    if idx < imported_funcs {
                        ExportType::Func(imported_func_type(&self.0, idx)?)
                    } else {
                        let local_idx = idx - imported_funcs;
                        ExportType::Func(&self.0.funcs.get(local_idx)?.ty)
                    }
                }
                ExternalKind::Table => ExportType::Table(self.0.table_types.get(export.index as usize)?),
                ExternalKind::Memory => ExportType::Memory(self.0.memory_types.get(export.index as usize)?),
                ExternalKind::Global => {
                    let idx = export.index as usize;
                    let imported_globals = self
                        .0
                        .imports
                        .iter()
                        .filter(|import| matches!(import.kind, tinywasm_types::ImportKind::Global(_)))
                        .count();
                    if idx < imported_globals {
                        ExportType::Global(imported_global_type(&self.0, idx)?)
                    } else {
                        let local_idx = idx - imported_globals;
                        ExportType::Global(&self.0.globals.get(local_idx)?.ty)
                    }
                }
            };

            Some(ModuleExport { name: export.name.as_ref(), ty })
        })
    }
}

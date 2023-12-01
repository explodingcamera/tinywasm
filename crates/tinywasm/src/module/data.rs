use alloc::{boxed::Box, string::ToString, vec::Vec};
use wasmparser::{ExternalKind, FuncType, FunctionBody, ValType};

use crate::Result;

use super::instructions::Instruction;

/// A WebAssembly Function
pub struct Function {
    pub locals: Box<[ValType]>,
    pub body: Box<[Instruction]>,
}

impl Function {
    pub fn new(body: FunctionBody) -> Result<Self> {
        let locals_reader = body.get_locals_reader()?;
        let count = locals_reader.get_count();
        let mut locals = Vec::with_capacity(count as usize);
        locals.extend(
            locals_reader
                .into_iter()
                .filter_map(|l| l.ok())
                .map(|l| l.1),
        );

        if locals.len() != count as usize {
            return Err(crate::Error::Other("Invalid local index".to_string()));
        }

        let body_reader = body.get_operators_reader()?;
        let body = body_reader
            .into_iter()
            .map(|op| (op?).try_into())
            .collect::<Result<Vec<Instruction>>>()?;

        Ok(Self {
            locals: locals.into_boxed_slice(),
            body: body.into_boxed_slice(),
        })
    }
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

// TODO: maybe support rkyv serialization
pub struct ModuleData {
    pub version: Option<u16>,
    pub start_func: Option<u32>,

    pub types: Box<[FuncType]>,
    pub functions: Box<[Function]>,
    pub exports: Box<[Export]>,
    // pub tables: Option<TableType>,
    // pub memories: Option<MemoryType>,
    // pub globals: Option<GlobalType>,
    // pub elements: Option<ElementSectionReader<'a>>,
    // pub imports: Option<ImportSectionReader<'a>>,
    // pub data_segments: Option<DataSectionReader<'a>>,
}

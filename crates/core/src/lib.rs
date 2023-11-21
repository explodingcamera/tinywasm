#![no_std]
#![forbid(unsafe_code)]

#[cfg(feature = "std")]
extern crate std;
use std::println;

extern crate alloc;
use alloc::vec::Vec;

use wasmparser::{
    DataSectionReader, ElementSectionReader, ExportSectionReader, FunctionBody,
    FunctionSectionReader, GlobalSectionReader, ImportSectionReader, MemorySectionReader, Payload,
    TableSectionReader, TypeSectionReader, Validator,
};
mod instructions;

struct Store {}

pub struct Module<'a> {
    reader: ModuleReader<'a>,
}

impl<'a> core::fmt::Debug for Module<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Module")
            .field("version", &self.reader.version)
            .field("type_section", &self.reader.type_section)
            .field("function_section", &self.reader.function_section)
            .field("table_section", &self.reader.table_section)
            .field("memory_section", &self.reader.memory_section)
            .field("global_section", &self.reader.global_section)
            .field("element_section", &self.reader.element_section)
            .field("data_section", &self.reader.data_section)
            .field("code_section", &self.reader.code_section)
            .field("import_section", &self.reader.import_section)
            .field("export_section", &self.reader.export_section)
            .finish()
    }
}

#[derive(Default)]
pub struct ModuleReader<'a> {
    pub version: Option<u16>,
    pub type_section: Option<TypeSectionReader<'a>>,
    pub function_section: Option<FunctionSectionReader<'a>>,
    pub table_section: Option<TableSectionReader<'a>>,
    pub memory_section: Option<MemorySectionReader<'a>>,
    pub global_section: Option<GlobalSectionReader<'a>>,
    pub element_section: Option<ElementSectionReader<'a>>,
    pub data_section: Option<DataSectionReader<'a>>,
    pub code_section: Option<CodeSection<'a>>,
    pub import_section: Option<ImportSectionReader<'a>>,
    pub export_section: Option<ExportSectionReader<'a>>,
}

#[derive(Debug)]
pub struct CodeSection<'a> {
    pub(crate) functions: Vec<FunctionBody<'a>>,
}

impl<'a> CodeSection<'a> {
    fn new() -> Self {
        Self {
            functions: Vec::new(),
        }
    }
}

impl<'a> Module<'a> {
    pub fn new(wasm: &'a [u8]) -> Result<Module, ()> {
        let mut validator = Validator::new();
        let mut reader = ModuleReader::new();

        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            reader.process_payload(payload.unwrap(), &mut validator)?;
        }

        Ok(Self { reader })
    }
}

impl<'a> ModuleReader<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn process_payload(
        &mut self,
        payload: Payload<'a>,
        validator: &mut Validator,
    ) -> Result<bool, ()> {
        use wasmparser::Payload::*;
        match payload {
            Version {
                num,
                encoding,
                range,
            } => {
                validator.version(num, encoding, &range).map_err(|_| ())?;
                self.version = Some(num);
                match encoding {
                    wasmparser::Encoding::Module => {}
                    wasmparser::Encoding::Component => return Err(()),
                }
            }
            TypeSection(reader) => {
                validator.type_section(&reader).map_err(|_| ())?;
                self.type_section = Some(reader);
            }
            FunctionSection(reader) => {
                validator.function_section(&reader).map_err(|_| ())?;
                self.function_section = Some(reader);
            }
            TableSection(reader) => {
                validator.table_section(&reader).map_err(|_| ())?;
                self.table_section = Some(reader);
            }
            MemorySection(reader) => {
                validator.memory_section(&reader).map_err(|_| ())?;
                self.memory_section = Some(reader);
            }
            GlobalSection(reader) => {
                validator.global_section(&reader).map_err(|_| ())?;
                self.global_section = Some(reader);
            }
            ElementSection(reader) => {
                validator.element_section(&reader).map_err(|_| ())?;
                self.element_section = Some(reader);
            }
            DataSection(reader) => {
                validator.data_section(&reader).map_err(|_| ())?;
                self.data_section = Some(reader);
            }
            CodeSectionStart { count, range, .. } => {
                validator
                    .code_section_start(count, &range)
                    .map_err(|_| ())?;

                self.code_section = Some(CodeSection::new());
            }
            CodeSectionEntry(function) => {
                validator.code_section_entry(&function).map_err(|_| ())?;

                if let Some(code_section) = &mut self.code_section {
                    code_section.functions.push(function);
                } else {
                    return Err(());
                }
            }
            ImportSection(reader) => {
                validator.import_section(&reader).map_err(|_| ())?;
                self.import_section = Some(reader);
            }
            ExportSection(reader) => {
                validator.export_section(&reader).map_err(|_| ())?;
                self.export_section = Some(reader);
            }

            End(offset) => {
                validator.end(offset).map_err(|_| ())?;
                return Ok(true);
            }
            x => println!("Unknown payload: {:?}", x),
        };

        Ok(false)
    }
}
struct Instance {}

pub fn parse(wasm: &[u8]) -> Result<Payload<'_>, ()> {
    for payload in wasmparser::Parser::new(0).parse_all(wasm) {
        return Ok(payload.unwrap());
    }

    return Err(());
}

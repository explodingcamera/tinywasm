use crate::log::debug;
use alloc::{boxed::Box, format, vec::Vec};
use core::fmt::Debug;
use tinywasm_types::{Export, FuncType, Global, Instruction, MemoryType, TableType, ValType};
use wasmparser::{Payload, Validator};

use crate::{conversion, ParseError, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct CodeSection {
    pub locals: Box<[ValType]>,
    pub body: Box<[Instruction]>,
}

#[derive(Default)]
pub struct ModuleReader {
    pub version: Option<u16>,
    pub start_func: Option<u32>,

    pub func_types: Vec<FuncType>,
    pub func_addrs: Vec<u32>,
    pub exports: Vec<Export>,
    pub code: Vec<CodeSection>,
    pub globals: Vec<Global>,
    pub table_types: Vec<TableType>,
    pub memory_types: Vec<MemoryType>,

    // pub element_section: Option<ElementSectionReader<'a>>,
    // pub data_section: Option<DataSectionReader<'a>>,
    // pub import_section: Option<ImportSectionReader<'a>>,
    pub end_reached: bool,
}

impl Debug for ModuleReader {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_struct("ModuleReader")
            .field("version", &self.version)
            .field("func_types", &self.func_types)
            .field("func_addrs", &self.func_addrs)
            .field("code", &self.code)
            .field("exports", &self.exports)
            .field("globals", &self.globals)
            .field("table_types", &self.table_types)
            .field("memory_types", &self.memory_types)
            // .field("element_section", &self.element_section)
            // .field("data_section", &self.data_section)
            // .field("import_section", &self.import_section)
            .finish()
    }
}

impl ModuleReader {
    pub fn new() -> ModuleReader {
        Self::default()
    }

    pub fn process_payload(&mut self, payload: Payload, validator: &mut Validator) -> Result<()> {
        use wasmparser::Payload::*;

        match payload {
            Version { num, encoding, range } => {
                validator.version(num, encoding, &range)?;
                self.version = Some(num);
                match encoding {
                    wasmparser::Encoding::Module => {}
                    wasmparser::Encoding::Component => return Err(ParseError::InvalidEncoding(encoding)),
                }
            }
            StartSection { func, range } => {
                debug!("Found start section");
                validator.start_section(func, &range)?;
                self.start_func = Some(func);
            }
            TypeSection(reader) => {
                debug!("Found type section");
                validator.type_section(&reader)?;
                self.func_types = reader
                    .into_iter()
                    .map(|t| conversion::convert_module_type(t?))
                    .collect::<Result<Vec<FuncType>>>()?;
            }
            FunctionSection(reader) => {
                debug!("Found function section");
                validator.function_section(&reader)?;
                self.func_addrs = reader.into_iter().map(|f| Ok(f?)).collect::<Result<Vec<_>>>()?;
            }
            GlobalSection(reader) => {
                debug!("Found global section");
                validator.global_section(&reader)?;
                self.globals = conversion::convert_module_globals(reader)?;
            }
            TableSection(reader) => {
                debug!("Found table section");
                validator.table_section(&reader)?;
                self.table_types = conversion::convert_module_tables(reader)?;
            }
            MemorySection(reader) => {
                debug!("Found memory section");
                validator.memory_section(&reader)?;
                self.memory_types = conversion::convert_module_memories(reader)?;
            }
            ElementSection(_reader) => {
                return Err(ParseError::UnsupportedSection("Element section".into()));
                // debug!("Found element section");
                // validator.element_section(&reader)?;
                // self.element_section = Some(reader);
            }
            DataSection(_reader) => {
                return Err(ParseError::UnsupportedSection("Data section".into()));
                // debug!("Found data section");
                // validator.data_section(&reader)?;
                // self.data_section = Some(reader);
            }
            CodeSectionStart { count, range, .. } => {
                debug!("Found code section ({} functions)", count);
                if !self.code.is_empty() {
                    return Err(ParseError::DuplicateSection("Code section".into()));
                }

                validator.code_section_start(count, &range)?;
            }
            CodeSectionEntry(function) => {
                debug!("Found code section entry");
                let v = validator.code_section_entry(&function)?;
                let func_validator = v.into_validator(Default::default());

                self.code
                    .push(conversion::convert_module_code(function, func_validator)?);
            }
            ImportSection(_reader) => {
                return Err(ParseError::UnsupportedSection("Import section".into()));

                // debug!("Found import section");
                // validator.import_section(&reader)?;
                // self.import_section = Some(reader);
            }
            ExportSection(reader) => {
                debug!("Found export section");
                validator.export_section(&reader)?;
                self.exports = reader
                    .into_iter()
                    .map(|e| conversion::convert_module_export(e?))
                    .collect::<Result<Vec<_>>>()?;
            }
            End(offset) => {
                debug!("Reached end of module");
                if self.end_reached {
                    return Err(ParseError::DuplicateSection("End section".into()));
                }

                validator.end(offset)?;
                self.end_reached = true;
            }
            CustomSection(reader) => {
                debug!("Found custom section");
                debug!("Skipping custom section: {:?}", reader.name());
            }
            // TagSection(tag) => {
            //     debug!("Found tag section");
            //     validator.tag_section(&tag)?;
            // }
            UnknownSection { .. } => return Err(ParseError::UnsupportedSection("Unknown section".into())),
            section => {
                return Err(ParseError::UnsupportedSection(format!(
                    "Unsupported section: {:?}",
                    section
                )))
            }
        };

        Ok(())
    }
}

use alloc::{boxed::Box, format, vec::Vec};
use core::fmt::Debug;
use log::debug;
use tinywasm_types::{Export, FuncType, Instruction, ValType};
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

    pub type_section: Vec<FuncType>,
    pub function_section: Vec<u32>,
    pub export_section: Vec<Export>,
    pub code_section: Vec<CodeSection>,

    // pub table_section: Option<TableSectionReader<'a>>,
    // pub memory_section: Option<MemorySectionReader<'a>>,
    // pub global_section: Option<GlobalSectionReader<'a>>,
    // pub element_section: Option<ElementSectionReader<'a>>,
    // pub data_section: Option<DataSectionReader<'a>>,
    // pub import_section: Option<ImportSectionReader<'a>>,
    pub end_reached: bool,
}

impl Debug for ModuleReader {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_struct("ModuleReader")
            .field("version", &self.version)
            .field("type_section", &self.type_section)
            .field("function_section", &self.function_section)
            .field("code_section", &self.code_section)
            .field("export_section", &self.export_section)
            // .field("table_section", &self.table_section)
            // .field("memory_section", &self.memory_section)
            // .field("global_section", &self.global_section)
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
            Version {
                num,
                encoding,
                range,
            } => {
                validator.version(num, encoding, &range)?;
                self.version = Some(num);
                match encoding {
                    wasmparser::Encoding::Module => {}
                    wasmparser::Encoding::Component => {
                        return Err(ParseError::InvalidEncoding(encoding))
                    }
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
                self.type_section = reader
                    .into_iter()
                    .map(|t| conversion::convert_module_type(t?))
                    .collect::<Result<Vec<FuncType>>>()?;
            }
            FunctionSection(reader) => {
                debug!("Found function section");
                validator.function_section(&reader)?;
                self.function_section = reader
                    .into_iter()
                    .map(|f| Ok(f?))
                    .collect::<Result<Vec<_>>>()?;
            }
            TableSection(_reader) => {
                return Err(ParseError::UnsupportedSection("Table section".into()));
                // debug!("Found table section");
                // validator.table_section(&reader)?;
                // self.table_section = Some(reader);
            }
            MemorySection(_reader) => {
                return Err(ParseError::UnsupportedSection("Memory section".into()));
                // debug!("Found memory section");
                // validator.memory_section(&reader)?;
                // self.memory_section = Some(reader);
            }
            GlobalSection(_reader) => {
                return Err(ParseError::UnsupportedSection("Global section".into()));
                // debug!("Found global section");
                // validator.global_section(&reader)?;
                // self.global_section = Some(reader);
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
                if !self.code_section.is_empty() {
                    return Err(ParseError::DuplicateSection("Code section".into()));
                }

                validator.code_section_start(count, &range)?;
            }
            CodeSectionEntry(function) => {
                debug!("Found code section entry");
                validator.code_section_entry(&function)?;

                self.code_section
                    .push(conversion::convert_module_code(function)?);
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
                self.export_section = reader
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
            UnknownSection { .. } => {
                return Err(ParseError::UnsupportedSection("Unknown section".into()))
            }
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

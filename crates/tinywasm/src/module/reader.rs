use alloc::{format, vec::Vec};
use core::fmt::Debug;
use tracing::debug;
use wasmparser::{
    DataSectionReader, ElementSectionReader, ExportSectionReader, FunctionBody,
    FunctionSectionReader, GlobalSectionReader, ImportSectionReader, MemorySectionReader, Payload,
    TableSectionReader, TypeSectionReader, Validator,
};

use crate::{Error, Result};

#[derive(Default)]
pub struct ModuleReader<'a> {
    pub version: Option<u16>,

    pub type_section: Option<TypeSectionReader<'a>>,
    pub function_section: Option<FunctionSectionReader<'a>>,
    pub export_section: Option<ExportSectionReader<'a>>,
    pub code_section: Option<CodeSection<'a>>,

    pub table_section: Option<TableSectionReader<'a>>,
    pub memory_section: Option<MemorySectionReader<'a>>,
    pub global_section: Option<GlobalSectionReader<'a>>,
    pub element_section: Option<ElementSectionReader<'a>>,
    pub data_section: Option<DataSectionReader<'a>>,
    pub import_section: Option<ImportSectionReader<'a>>,
    pub end_reached: bool,
}

impl Debug for ModuleReader<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ModuleReader")
            .field("version", &self.version)
            .field("type_section", &self.type_section)
            .field("function_section", &self.function_section)
            .field("table_section", &self.table_section)
            .field("memory_section", &self.memory_section)
            .field("global_section", &self.global_section)
            .field("element_section", &self.element_section)
            .field("data_section", &self.data_section)
            .field("code_section", &self.code_section)
            .field("import_section", &self.import_section)
            .field("export_section", &self.export_section)
            .finish()
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
    ) -> Result<()> {
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
                    wasmparser::Encoding::Component => return Error::other("Component"),
                }
            }
            TypeSection(reader) => {
                debug!("Found type section");
                validator.type_section(&reader)?;
                self.type_section = Some(reader);
            }
            FunctionSection(reader) => {
                debug!("Found function section");
                validator.function_section(&reader)?;
                self.function_section = Some(reader);
            }
            TableSection(_reader) => {
                return Error::unsupported("Table section");
                // debug!("Found table section");
                // validator.table_section(&reader)?;
                // self.table_section = Some(reader);
            }
            MemorySection(_reader) => {
                return Error::unsupported("Memory section");
                // debug!("Found memory section");
                // validator.memory_section(&reader)?;
                // self.memory_section = Some(reader);
            }
            GlobalSection(_reader) => {
                return Error::unsupported("Global section");
                // debug!("Found global section");
                // validator.global_section(&reader)?;
                // self.global_section = Some(reader);
            }
            ElementSection(_reader) => {
                return Error::unsupported("Element section");
                // debug!("Found element section");
                // validator.element_section(&reader)?;
                // self.element_section = Some(reader);
            }
            DataSection(_reader) => {
                return Error::unsupported("Data section");
                // debug!("Found data section");
                // validator.data_section(&reader)?;
                // self.data_section = Some(reader);
            }
            CodeSectionStart { count, range, .. } => {
                debug!("Found code section ({} functions)", count);
                validator.code_section_start(count, &range)?;
                self.code_section = Some(CodeSection::new());
            }
            CodeSectionEntry(function) => {
                debug!("Found code section entry");
                validator.code_section_entry(&function)?;

                if let Some(code_section) = &mut self.code_section {
                    code_section.functions.push(function);
                } else {
                    return Error::other("Empty code section");
                }
            }
            ImportSection(_reader) => {
                return Error::unsupported("Import section");

                // debug!("Found import section");
                // validator.import_section(&reader)?;
                // self.import_section = Some(reader);
            }
            ExportSection(reader) => {
                debug!("Found export section");
                validator.export_section(&reader)?;
                self.export_section = Some(reader);
            }
            End(offset) => {
                debug!("Reached end of module");
                if self.end_reached {
                    return Error::other("End reached twice");
                }

                validator.end(offset)?;
                self.end_reached = true;
            }
            x => Error::other(&format!("Unknown payload: {:?}", x))?,
        };

        Ok(())
    }

    // fn exports(&mut self) -> Result<Vec<Export>> {
    //     let mut exports = Vec::new();

    //     if let Some(export_section) = self.export_section {
    //         for export in export_section.into_iter() {
    //             let export = export?;
    //             let name = export.name;
    //             let kind = export.kind;
    //             let index = export.index;
    //             exports.push(Export { name, kind, index });
    //         }
    //     }

    //     Ok(exports)
    // }
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

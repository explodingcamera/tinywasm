use crate::error::{Error, Result};
use alloc::{format, vec::Vec};
use tracing::error;
use wasmparser::*;

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
    pub fn new(wasm: &'a [u8]) -> Result<Self> {
        let mut validator = Validator::new();
        let mut reader = ModuleReader::new();

        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            reader.process_payload(payload?, &mut validator)?;
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
                validator.type_section(&reader)?;
                self.type_section = Some(reader);
            }
            FunctionSection(reader) => {
                validator.function_section(&reader)?;
                self.function_section = Some(reader);
            }
            TableSection(reader) => {
                validator.table_section(&reader)?;
                self.table_section = Some(reader);
            }
            MemorySection(reader) => {
                validator.memory_section(&reader)?;
                self.memory_section = Some(reader);
            }
            GlobalSection(reader) => {
                validator.global_section(&reader)?;
                self.global_section = Some(reader);
            }
            ElementSection(reader) => {
                validator.element_section(&reader)?;
                self.element_section = Some(reader);
            }
            DataSection(reader) => {
                validator.data_section(&reader)?;
                self.data_section = Some(reader);
            }
            CodeSectionStart { count, range, .. } => {
                validator.code_section_start(count, &range)?;

                self.code_section = Some(CodeSection::new());
            }
            CodeSectionEntry(function) => {
                validator.code_section_entry(&function)?;

                if let Some(code_section) = &mut self.code_section {
                    code_section.functions.push(function);
                } else {
                    return Error::other("Empty code section");
                }
            }
            ImportSection(reader) => {
                validator.import_section(&reader)?;
                self.import_section = Some(reader);
            }
            ExportSection(reader) => {
                validator.export_section(&reader)?;
                self.export_section = Some(reader);
            }

            End(offset) => {
                validator.end(offset)?;
                return Ok(());
            }
            x => Error::other(&format!("Unknown payload: {:?}", x))?,
        };

        error!("Missing end");

        Ok(())
    }
}

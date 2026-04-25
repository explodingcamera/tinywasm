use crate::log::debug;
use crate::{ParseError, ParserOptions, Result, conversion, optimize};
use alloc::sync::Arc;
use alloc::{format, string::ToString, vec::Vec};
use tinywasm_types::*;
use wasmparser::{FuncValidatorAllocations, Payload, Validator};

pub(crate) type Code = (Vec<Instruction>, WasmFunctionData, ValueCounts);

#[derive(Default)]
pub(crate) struct ModuleReader {
    func_validator_allocations: Option<FuncValidatorAllocations>,

    pub(crate) version: Option<u16>,
    pub(crate) start_func: Option<u32>,
    pub(crate) func_types: Vec<Arc<FuncType>>,
    pub(crate) code_type_addrs: Vec<u32>,
    pub(crate) exports: Vec<Export>,
    pub(crate) code: Vec<Code>,
    pub(crate) globals: Vec<Global>,
    pub(crate) table_types: Vec<TableType>,
    pub(crate) memory_types: Vec<MemoryType>,
    pub(crate) imports: Vec<Import>,
    pub(crate) data: Vec<Data>,
    pub(crate) elements: Vec<Element>,
    pub(crate) end_reached: bool,
}

impl ModuleReader {
    pub(crate) fn process_payload(&mut self, payload: Payload<'_>, validator: &mut Validator) -> Result<()> {
        match payload {
            Payload::Version { num, encoding, range } => {
                validator.version(num, encoding, &range)?;
                self.version = Some(num);
                if let wasmparser::Encoding::Component = encoding {
                    return Err(ParseError::InvalidEncoding(encoding));
                }
            }
            Payload::StartSection { func, range } => {
                if self.start_func.is_some() {
                    return Err(ParseError::DuplicateSection("Start section".into()));
                }

                debug!("Found start section");
                validator.start_section(func, &range)?;
                self.start_func = Some(func);
            }
            Payload::TypeSection(reader) => {
                if !self.func_types.is_empty() {
                    return Err(ParseError::DuplicateSection("Type section".into()));
                }

                debug!("Found type section");
                validator.type_section(&reader)?;
                self.func_types =
                    reader.into_iter().map(|t| conversion::convert_module_type(t?)).collect::<Result<Vec<_>>>()?;
            }

            Payload::GlobalSection(reader) => {
                if !self.globals.is_empty() {
                    return Err(ParseError::DuplicateSection("Global section".into()));
                }

                debug!("Found global section");
                validator.global_section(&reader)?;
                self.globals = conversion::convert_module_globals(reader)?;
            }
            Payload::TableSection(reader) => {
                if !self.table_types.is_empty() {
                    return Err(ParseError::DuplicateSection("Table section".into()));
                }
                debug!("Found table section");
                validator.table_section(&reader)?;
                self.table_types = conversion::convert_module_tables(reader)?;
            }
            Payload::MemorySection(reader) => {
                if !self.memory_types.is_empty() {
                    return Err(ParseError::DuplicateSection("Memory section".into()));
                }

                debug!("Found memory section");
                validator.memory_section(&reader)?;
                self.memory_types = conversion::convert_module_memories(reader)?;
            }
            Payload::ElementSection(reader) => {
                debug!("Found element section");
                validator.element_section(&reader)?;
                self.elements = conversion::convert_module_elements(reader)?;
            }
            Payload::DataSection(reader) => {
                if !self.data.is_empty() {
                    return Err(ParseError::DuplicateSection("Data section".into()));
                }

                debug!("Found data section");
                validator.data_section(&reader)?;
                self.data = conversion::convert_module_data_sections(reader)?;
            }
            Payload::DataCountSection { count, range } => {
                debug!("Found data count section");
                if !self.data.is_empty() {
                    return Err(ParseError::DuplicateSection("Data count section".into()));
                }
                validator.data_count_section(count, &range)?;
            }
            Payload::FunctionSection(reader) => {
                if !self.code_type_addrs.is_empty() {
                    return Err(ParseError::DuplicateSection("Function section".into()));
                }

                debug!("Found function section");
                validator.function_section(&reader)?;
                self.code_type_addrs = reader.into_iter().map(|f| Ok(f?)).collect::<Result<Vec<_>>>()?;
            }
            Payload::CodeSectionStart { count, range, .. } => {
                debug!("Found code section ({count} functions)");
                if !self.code.is_empty() {
                    return Err(ParseError::DuplicateSection("Code section".into()));
                }
                self.code.reserve(count as usize);
                validator.code_section_start(&range)?;
            }
            Payload::CodeSectionEntry(function) => {
                debug!("Found code section entry");
                let v = validator.code_section_entry(&function)?;
                let func_validator = v.into_validator(self.func_validator_allocations.take().unwrap_or_default());
                let (code, allocations) = conversion::convert_module_code(function, func_validator)?;
                self.code.push(code);
                self.func_validator_allocations = Some(allocations);
            }
            Payload::ImportSection(reader) => {
                if !self.imports.is_empty() {
                    return Err(ParseError::DuplicateSection("Import section".into()));
                }

                debug!("Found import section");
                validator.import_section(&reader)?;
                self.imports = conversion::convert_module_imports(reader.into_imports())?;
            }
            Payload::ExportSection(reader) => {
                if !self.exports.is_empty() {
                    return Err(ParseError::DuplicateSection("Export section".into()));
                }

                debug!("Found export section");
                validator.export_section(&reader)?;
                self.exports =
                    reader.into_iter().map(|e| conversion::convert_module_export(e?)).collect::<Result<Vec<_>>>()?;
            }
            Payload::End(offset) => {
                debug!("Reached end of module");
                if self.end_reached {
                    return Err(ParseError::DuplicateSection("End section".into()));
                }

                validator.end(offset)?;
                self.end_reached = true;
            }
            Payload::CustomSection(_reader) => {
                debug!("Found custom section");
                debug!("Skipping custom section: {:?}", _reader.name());
            }
            Payload::UnknownSection { .. } => return Err(ParseError::UnsupportedSection("Unknown section".into())),
            section => return Err(ParseError::UnsupportedSection(format!("Unsupported section: {section:?}"))),
        };
        Ok(())
    }

    pub(crate) fn into_module(self, options: &ParserOptions) -> Result<Module> {
        if !self.end_reached {
            return Err(ParseError::EndNotReached);
        }

        if self.code_type_addrs.len() != self.code.len() {
            return Err(ParseError::Other("Code and code type address count mismatch".to_string()));
        }

        let imported_func_count = self.imports.iter().filter(|i| matches!(&i.kind, ImportKind::Function(_))).count();
        let import_mem_count = self.imports.iter().filter(|i| matches!(&i.kind, ImportKind::Memory(_))).count() as u32;
        let has_local_mem_export =
            self.exports.iter().any(|export| export.kind == ExternalKind::Memory && export.index >= import_mem_count);
        let has_active_data_segment_on_local_memory = self.data.iter().any(|data| match &data.kind {
            DataKind::Active { mem, .. } => *mem >= import_mem_count,
            DataKind::Passive => false,
        });
        let optimize_local_memory_allocation = options.optimize_local_memory_allocation();
        let mut local_memory_allocation = if self.memory_types.is_empty() {
            LocalMemoryAllocation::Skip
        } else if !optimize_local_memory_allocation || has_active_data_segment_on_local_memory {
            LocalMemoryAllocation::Eager
        } else if has_local_mem_export {
            LocalMemoryAllocation::Lazy
        } else {
            LocalMemoryAllocation::Skip
        };
        let mut funcs = Vec::with_capacity(self.code.len());

        for (func_idx, ((instructions, mut data, locals), ty_idx)) in
            self.code.into_iter().zip(self.code_type_addrs).enumerate()
        {
            let ty = self.func_types.get(ty_idx as usize).expect("No func type for func, this is a bug").clone();
            let params = ValueCounts::from_iter(ty.params());
            let results = ValueCounts::from_iter(ty.results());
            let self_func = (imported_func_count + func_idx) as u32;
            let local_mem_alloc =
                optimize_local_memory_allocation && local_memory_allocation != LocalMemoryAllocation::Eager;
            let optimized = optimize::optimize_instructions(
                instructions,
                &mut data,
                options,
                self_func,
                import_mem_count,
                local_mem_alloc,
            );

            if optimized.uses_local_memory {
                local_memory_allocation = LocalMemoryAllocation::Eager;
            }

            funcs.push(
                WasmFunction { instructions: optimized.instructions.into(), data, locals, params, results, ty }.into(),
            );
        }

        Ok(ModuleInner {
            funcs: funcs.into(),
            func_types: self.func_types.into(),
            globals: self.globals.into(),
            table_types: self.table_types.into(),
            imports: self.imports.into(),
            start_func: self.start_func,
            data: self.data.into(),
            exports: self.exports.into(),
            elements: self.elements.into(),
            memory_types: self.memory_types.into(),
            local_memory_allocation,
        }
        .into())
    }
}

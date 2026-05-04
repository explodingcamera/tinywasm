use crate::log::debug;
use crate::{ParseError, ParserOptions, Result, conversion::*, optimize};
use alloc::{boxed::Box, format, string::ToString, sync::Arc, vec::Vec};
use core::marker::PhantomData;
use core::ops::Range;
use tinywasm_types::*;
use wasmparser::{FuncValidatorAllocations, OperatorsReaderAllocations, Payload, Validator};

pub(crate) struct FunctionCode {
    pub instructions: Vec<Instruction>,
    pub data: WasmFunctionData,
    pub locals: ValueCounts,
    pub uses_local_memory: bool,
}

pub(crate) fn imported_func_count(imports: &[Import]) -> usize {
    imports.iter().filter(|i| matches!(&i.kind, ImportKind::Function(_))).count()
}

pub(crate) fn imported_memory_count(imports: &[Import]) -> u32 {
    imports.iter().filter(|i| matches!(&i.kind, ImportKind::Memory(_))).count() as u32
}

pub(crate) fn optimize_function_code(
    mut code: FunctionCode,
    options: &ParserOptions,
    function_results: ValueCounts,
    self_func_addr: u32,
    imported_memory_count: u32,
) -> FunctionCode {
    let optimized = optimize::optimize_instructions(
        code.instructions,
        &mut code.data,
        options,
        function_results,
        self_func_addr,
        imported_memory_count,
    );

    code.instructions = optimized.instructions;
    code.uses_local_memory = optimized.uses_local_memory;
    code
}

#[derive(Default)]
pub(crate) struct ModuleReader<'a> {
    func_validator_allocations: Option<FuncValidatorAllocations>,
    operators_reader_allocations: Option<OperatorsReaderAllocations>,

    has_code_section: bool,
    marker: PhantomData<&'a [u8]>,

    pub(crate) version: Option<u16>,
    pub(crate) start_func: Option<u32>,
    pub(crate) func_types: Arc<[Arc<FuncType>]>,
    pub(crate) code_type_addrs: Box<[u32]>,
    pub(crate) exports: Arc<[Export]>,
    pub(crate) code: Vec<FunctionCode>,
    pub(crate) globals: Box<[Global]>,
    pub(crate) table_types: Box<[TableType]>,
    pub(crate) memory_types: Box<[MemoryType]>,
    pub(crate) imports: Box<[Import]>,
    pub(crate) data: Box<[Data]>,
    pub(crate) elements: Box<[Element]>,
    pub(crate) end_reached: bool,

    #[cfg(parallel_parser)]
    pending_functions: Option<Vec<crate::parallel::PendingFunction<'a>>>,
}

impl<'a> ModuleReader<'a> {
    fn function_results(&self, ordinal: usize) -> ValueCounts {
        let ty_idx = self.code_type_addrs[ordinal];
        let ty = self.func_types.get(ty_idx as usize).expect("No func type for func, this is a bug");
        ValueCounts::from_iter(ty.results())
    }

    pub(crate) fn process_payload(&mut self, payload: Payload<'_>, validator: &mut Validator) -> Result<()> {
        fn check_section(section: &str, duplicate: bool) -> Result<()> {
            debug!("found {section} section");
            if duplicate {
                return Err(ParseError::DuplicateSection(format!("{section} section")));
            }
            Ok(())
        }

        match payload {
            Payload::Version { num, encoding, range } => {
                validator.version(num, encoding, &range)?;
                self.version = Some(num);
                if let wasmparser::Encoding::Component = encoding {
                    return Err(ParseError::InvalidEncoding(encoding));
                }
            }
            Payload::StartSection { func, range } => {
                check_section("start", self.start_func.is_some())?;
                validator.start_section(func, &range)?;
                self.start_func = Some(func);
            }
            Payload::TypeSection(reader) => {
                check_section("type", !self.func_types.is_empty())?;
                validator.type_section(&reader)?;
                self.func_types = reader.into_iter().map(|t| convert_module_type(t?)).collect::<Result<_>>()?;
            }
            Payload::GlobalSection(reader) => {
                check_section("global", !self.globals.is_empty())?;
                validator.global_section(&reader)?;
                self.globals = convert_module_globals(reader)?;
            }
            Payload::TableSection(reader) => {
                check_section("table", !self.table_types.is_empty())?;
                validator.table_section(&reader)?;
                self.table_types =
                    reader.into_iter().map(|table| convert_module_table(table?)).collect::<Result<_>>()?;
            }
            Payload::MemorySection(reader) => {
                check_section("memory", !self.memory_types.is_empty())?;
                validator.memory_section(&reader)?;
                self.memory_types =
                    reader.into_iter().map(|memory| Ok(convert_module_memory(memory?))).collect::<Result<_>>()?;
            }
            Payload::ElementSection(reader) => {
                debug!("Found element section");
                validator.element_section(&reader)?;
                self.elements =
                    reader.into_iter().map(|element| convert_module_element(element?)).collect::<Result<_>>()?;
            }
            Payload::DataSection(reader) => {
                check_section("data", !self.data.is_empty())?;
                validator.data_section(&reader)?;
                self.data = reader.into_iter().map(|data| convert_module_data(data?)).collect::<Result<_>>()?;
            }
            Payload::DataCountSection { count, range } => {
                debug!("Found data count section");
                if !self.data.is_empty() {
                    return Err(ParseError::UnsupportedSection("Data count section after data section".into()));
                }
                validator.data_count_section(count, &range)?;
            }
            Payload::FunctionSection(reader) => {
                check_section("function", !self.code_type_addrs.is_empty())?;
                validator.function_section(&reader)?;
                self.code_type_addrs = reader.into_iter().map(|f| Ok(f?)).collect::<Result<_>>()?;
            }
            Payload::ImportSection(reader) => {
                check_section("import", !self.imports.is_empty())?;
                validator.import_section(&reader)?;
                self.imports =
                    reader.into_imports().map(|import| convert_module_import(import?)).collect::<Result<_>>()?;
            }
            Payload::ExportSection(reader) => {
                check_section("export", !self.exports.is_empty())?;
                validator.export_section(&reader)?;
                self.exports = reader.into_iter().map(|e| convert_module_export(e?)).collect::<Result<_>>()?;
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
            Payload::CodeSectionStart { .. } | Payload::CodeSectionEntry(_) => {
                unreachable!("code section payload handled separately")
            }
            Payload::UnknownSection { .. } => return Err(ParseError::UnsupportedSection("Unknown section".into())),
            section => return Err(ParseError::UnsupportedSection(format!("Unsupported section: {section:?}"))),
        }

        Ok(())
    }

    pub(crate) fn begin_code_section(
        &mut self,
        count: u32,
        range: Range<usize>,
        size: u32,
        validator: &mut Validator,
        options: &ParserOptions,
    ) -> Result<bool> {
        debug!("Found code section ({count} functions)");
        if self.has_code_section {
            return Err(ParseError::DuplicateSection("Code section".into()));
        }

        self.has_code_section = true;
        self.code.reserve(count as usize);
        validator.code_section_start(&range)?;

        #[cfg(parallel_parser)]
        {
            let defer = crate::parallel::should_use_parallel(options, count as usize, size as usize);
            if defer {
                debug!("Queuing {count} functions from {size} byte code section");
                self.pending_functions = Some(Vec::with_capacity(count as usize));
            }
            Ok(defer)
        }

        #[cfg(not(parallel_parser))]
        {
            let _ = (size, options);
            Ok(false)
        }
    }

    pub(crate) fn process_inline_code_section_entry(
        &mut self,
        function: wasmparser::FunctionBody<'_>,
        validator: &mut Validator,
        options: &ParserOptions,
    ) -> Result<()> {
        debug!("Found code section entry");

        let func_validator_allocs = self.func_validator_allocations.take().unwrap_or_default();
        let operators_reader_allocs = self.operators_reader_allocations.take().unwrap_or_default();

        let func_to_validate = validator.code_section_entry(&function)?;
        let func_validator = func_to_validate.into_validator(func_validator_allocs);

        let (code, func_validator_allocs, operators_reader_allocs) =
            convert_module_code(function, func_validator, operators_reader_allocs)?;

        self.code.push(optimize_function_code(
            code,
            options,
            self.function_results(self.code.len()),
            (imported_func_count(&self.imports) + self.code.len()) as u32,
            imported_memory_count(&self.imports),
        ));

        self.func_validator_allocations = Some(func_validator_allocs);
        self.operators_reader_allocations = Some(operators_reader_allocs);
        Ok(())
    }

    pub(crate) fn process_borrowed_code_section_entry(
        &mut self,
        function: wasmparser::FunctionBody<'a>,
        validator: &mut Validator,
        options: &ParserOptions,
    ) -> Result<()> {
        debug!("Found code section entry");

        #[cfg(parallel_parser)]
        if let Some(pending) = self.pending_functions.as_mut() {
            let func_to_validate = validator.code_section_entry(&function)?;
            let ordinal = self.code.len() + pending.len();
            let ty_idx = self.code_type_addrs[ordinal];
            pending.push(crate::parallel::PendingFunction {
                ordinal,
                ty_idx,
                func_to_validate,
                body: crate::parallel::FunctionBodyInput::Borrowed(function),
            });
            return Ok(());
        }

        self.process_inline_code_section_entry(function, validator, options)
    }

    #[cfg(parallel_parser)]
    pub(crate) fn queue_owned_code_section(
        &mut self,
        count: u32,
        body_offset: usize,
        section_bytes: Arc<[u8]>,
        validator: &mut Validator,
    ) -> Result<()> {
        let code_len = self.code.len();
        let pending = self
            .pending_functions
            .as_mut()
            .ok_or_else(|| ParseError::Other("owned code section queued without pending storage".into()))?;

        let mut reader = wasmparser::BinaryReader::new(&section_bytes, body_offset);
        for _ in 0..count {
            let body_reader = reader.read_reader()?;
            let body_range = body_reader.range();
            let function = wasmparser::FunctionBody::new(body_reader);
            let func_to_validate = validator.code_section_entry(&function)?;
            let ordinal = code_len + pending.len();
            let ty_idx = self.code_type_addrs[ordinal];
            pending.push(crate::parallel::PendingFunction {
                ordinal,
                ty_idx,
                func_to_validate,
                body: crate::parallel::FunctionBodyInput::Owned(crate::parallel::OwnedFunctionBody {
                    section_bytes: section_bytes.clone(),
                    body_range: (body_range.start - body_offset)..(body_range.end - body_offset),
                    body_offset: body_range.start,
                }),
            });
        }

        if reader.bytes_remaining() != 0 {
            return Err(ParseError::ParseError {
                message: "trailing bytes at end of section".into(),
                offset: reader.original_position(),
            });
        }

        Ok(())
    }

    #[cfg(parallel_parser)]
    pub(crate) fn process_pending_functions(&mut self, options: &ParserOptions) -> Result<()> {
        let Some(pending) = self.pending_functions.take().filter(|pending| !pending.is_empty()) else {
            return Ok(());
        };

        self.code.extend(crate::parallel::process_pending(
            pending,
            options,
            &self.func_types,
            imported_func_count(&self.imports),
            imported_memory_count(&self.imports),
        )?);
        Ok(())
    }

    #[cfg(not(parallel_parser))]
    pub(crate) fn process_pending_functions(&mut self, _options: &ParserOptions) -> Result<()> {
        Ok(())
    }

    pub(crate) fn into_module(self, options: &ParserOptions) -> Result<Module> {
        if !self.end_reached {
            return Err(ParseError::EndNotReached);
        }

        if self.code_type_addrs.len() != self.code.len() {
            return Err(ParseError::Other("Code and code type address count mismatch".to_string()));
        }

        let import_mem_count = imported_memory_count(&self.imports);
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

        let func_type_idxs = self
            .imports
            .iter()
            .filter_map(|import| match import.kind {
                ImportKind::Function(type_idx) => Some(type_idx),
                _ => None,
            })
            .chain(self.code_type_addrs.iter().copied())
            .collect();

        let funcs = self
            .code
            .into_iter()
            .zip(self.code_type_addrs)
            .map(|(code, ty_idx)| {
                let ty = self.func_types.get(ty_idx as usize).expect("No func type for func, this is a bug").clone();
                let params = ValueCounts::from_iter(ty.params());
                let results = ValueCounts::from_iter(ty.results());
                if code.uses_local_memory {
                    local_memory_allocation = LocalMemoryAllocation::Eager;
                }

                Arc::new(WasmFunction {
                    instructions: code.instructions.into(),
                    data: code.data,
                    locals: code.locals,
                    params,
                    results,
                    ty,
                })
            })
            .collect();

        Ok(ModuleInner {
            funcs,
            func_types: self.func_types,
            func_type_idxs,
            globals: self.globals,
            table_types: self.table_types,
            imports: self.imports,
            start_func: self.start_func,
            data: self.data,
            exports: self.exports,
            elements: self.elements,
            memory_types: self.memory_types,
            local_memory_allocation,
        }
        .into())
    }
}

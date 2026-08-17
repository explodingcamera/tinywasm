use crate::module::{OptimizedFunctionCode, optimize_function_code};
use crate::validation::{FuncToValidate, FuncValidatorAllocations, ValidatorResources};
use crate::{ParseError, ParserOptions, Result, conversion};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;
use tinywasm_types::ValueCounts;
use wasmparser::OperatorsReaderAllocations;

pub(crate) enum FunctionBodyInput<'a> {
    Borrowed(wasmparser::FunctionBody<'a>),
    Owned(OwnedFunctionBody),
}

pub(crate) struct OwnedFunctionBody {
    // A deferred stream code section is copied once, then shared by all queued
    // function jobs from that section.
    pub section_bytes: Arc<[u8]>,
    pub body_range: Range<usize>,
    pub body_offset: usize,
}

pub(crate) struct PendingFunction<'a> {
    pub ordinal: usize,
    pub results: ValueCounts,
    pub func_to_validate: Option<FuncToValidate<ValidatorResources>>,
    pub ty_idx: u32,
    pub body: FunctionBodyInput<'a>,
}

const MIN_FUNCTIONS: usize = 8;
const MIN_CODE_SECTION_BYTES: usize = 16 * 1024;
const MIN_FUNCTION_BODY_BYTES: usize = 16;
const MIN_FUNCTIONS_PER_WORKER: usize = 4;
const MAX_WORKERS: usize = 12;

fn body_len(body: &FunctionBodyInput<'_>) -> usize {
    match body {
        FunctionBodyInput::Borrowed(func) => func.as_bytes().len(),
        FunctionBodyInput::Owned(body) => body.body_range.len(),
    }
}

pub(crate) fn should_use_parallel(options: &ParserOptions, num_functions: usize, code_section_bytes: usize) -> bool {
    num_functions >= MIN_FUNCTIONS
        && code_section_bytes >= MIN_CODE_SECTION_BYTES
        && code_section_bytes / num_functions >= MIN_FUNCTION_BODY_BYTES
        && worker_count(options, num_functions) > 1
}

fn worker_count(options: &ParserOptions, num_functions: usize) -> usize {
    let requested = options
        .parser_threads()
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
        .max(1);
    requested.min(MAX_WORKERS).min(num_functions.div_ceil(MIN_FUNCTIONS_PER_WORKER)).max(1)
}

fn process_function_job(
    job: PendingFunction<'_>,
    metadata: &crate::visit::ModuleMetadata,
    options: &ParserOptions,
    imported_func_count: usize,
    validator_allocs: Option<FuncValidatorAllocations>,
    reader_allocs: OperatorsReaderAllocations,
) -> Result<(OptimizedFunctionCode, Option<FuncValidatorAllocations>, OperatorsReaderAllocations)> {
    #[cfg(feature = "validate")]
    let validator = job.func_to_validate.map(|func| func.into_validator(validator_allocs.unwrap_or_default()));
    #[cfg(not(feature = "validate"))]
    let validator = {
        let _ = (job.func_to_validate, validator_allocs);
        None
    };
    let (code, validator_allocs, reader_allocs) = match job.body {
        FunctionBodyInput::Borrowed(func) => {
            conversion::convert_module_code(func, validator, reader_allocs, metadata, job.ty_idx, options)?
        }
        FunctionBodyInput::Owned(body) => {
            let reader = wasmparser::BinaryReader::new(&body.section_bytes[body.body_range], body.body_offset);
            let func = wasmparser::FunctionBody::new(reader);
            conversion::convert_module_code(func, validator, reader_allocs, metadata, job.ty_idx, options)?
        }
    };

    let code = optimize_function_code(code, options, job.results, (imported_func_count + job.ordinal) as u32)?;

    Ok((code, validator_allocs, reader_allocs))
}

fn process_chunk<'a>(
    jobs: impl IntoIterator<Item = PendingFunction<'a>>,
    metadata: &crate::visit::ModuleMetadata,
    options: &ParserOptions,
    imported_func_count: usize,
) -> Result<Vec<OptimizedFunctionCode>> {
    let mut validator_allocs = None;
    let mut reader_allocs = OperatorsReaderAllocations::default();
    let jobs = jobs.into_iter();
    let mut codes = Vec::with_capacity(jobs.size_hint().0);

    for job in jobs {
        let (code, next_validator_allocs, next_reader_allocs) =
            process_function_job(job, metadata, options, imported_func_count, validator_allocs, reader_allocs)?;
        codes.push(code);
        validator_allocs = next_validator_allocs;
        reader_allocs = next_reader_allocs;
    }

    Ok(codes)
}

pub(crate) fn process_pending(
    pending: Vec<PendingFunction<'_>>,
    metadata: &crate::visit::ModuleMetadata,
    options: &ParserOptions,
    imported_func_count: usize,
) -> Result<Vec<OptimizedFunctionCode>> {
    let num_workers = worker_count(options, pending.len());
    if num_workers == 1 {
        return process_chunk(pending, metadata, options, imported_func_count);
    }
    let code_count = pending.len();
    let chunk_size = pending.len().div_ceil(num_workers);
    let chunk_bytes = pending.iter().map(|job| body_len(&job.body)).sum::<usize>().div_ceil(num_workers);
    std::thread::scope(|scope| {
        let mut jobs = pending.into_iter();
        let mut handles = Vec::with_capacity(num_workers);
        while let Some(first) = jobs.next() {
            let mut chunk = Vec::with_capacity(chunk_size);
            let mut bytes = body_len(&first.body);
            chunk.push(first);
            while bytes < chunk_bytes
                && let Some(job) = jobs.next()
            {
                bytes += body_len(&job.body);
                chunk.push(job);
            }
            handles.push(scope.spawn(move || process_chunk(chunk, metadata, options, imported_func_count)));
        }

        let mut codes = Vec::with_capacity(code_count);
        for handle in handles {
            let chunk = handle.join().map_err(|_| ParseError::Other("worker thread panicked".into()))??;
            codes.extend(chunk);
        }
        Ok(codes)
    })
}

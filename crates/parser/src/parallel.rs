use crate::module::{FunctionCode, optimize_function_code};
use crate::{ParseError, ParserOptions, Result, conversion};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;
use tinywasm_types::{FuncType, ValueCounts};
use wasmparser::{FuncValidatorAllocations, ValidatorResources};

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
    pub ty_idx: u32,
    pub func_to_validate: wasmparser::FuncToValidate<ValidatorResources>,
    pub body: FunctionBodyInput<'a>,
}

pub(crate) const MIN_FUNCTIONS: usize = 8;
const MIN_CODE_SECTION_BYTES: usize = 32 * 1024;
const MIN_FUNCTION_BODY_BYTES: usize = 4;

pub(crate) fn should_parallelize_function(body_len: usize) -> bool {
    body_len >= MIN_FUNCTION_BODY_BYTES
}

pub(crate) fn should_use_parallel(options: &ParserOptions, num_functions: usize, code_section_bytes: usize) -> bool {
    if num_functions < MIN_FUNCTIONS || code_section_bytes < MIN_CODE_SECTION_BYTES {
        return false;
    }

    worker_count(options, num_functions) > 1
}

fn worker_count(options: &ParserOptions, num_functions: usize) -> usize {
    let requested = options
        .parser_threads()
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
        .max(1);

    requested.min(num_functions).max(1)
}

fn body_len(body: &FunctionBodyInput<'_>) -> usize {
    match body {
        FunctionBodyInput::Borrowed(func) => func.as_bytes().len(),
        FunctionBodyInput::Owned(body) => body.body_range.len(),
    }
}

fn process_function_job(
    job: PendingFunction<'_>,
    options: &ParserOptions,
    func_types: &[Arc<FuncType>],
    imported_func_count: usize,
    imported_memory_count: u32,
) -> Result<(usize, FunctionCode)> {
    let validator = job.func_to_validate.into_validator(FuncValidatorAllocations::default());
    let (code, _allocations) = match job.body {
        FunctionBodyInput::Borrowed(func) => conversion::convert_module_code(func, validator)?,
        FunctionBodyInput::Owned(body) => {
            let reader = wasmparser::BinaryReader::new(&body.section_bytes[body.body_range], body.body_offset);
            let func = wasmparser::FunctionBody::new(reader);
            conversion::convert_module_code(func, validator)?
        }
    };

    let ty = func_types.get(job.ty_idx as usize).expect("No func type for func, this is a bug");
    let code = optimize_function_code(
        code,
        options,
        ValueCounts::from_iter(ty.results()),
        (imported_func_count + job.ordinal) as u32,
        imported_memory_count,
    );

    Ok((job.ordinal, code))
}

pub(crate) fn process_pending(
    pending: Vec<PendingFunction<'_>>,
    options: &ParserOptions,
    func_types: &[Arc<FuncType>],
    imported_func_count: usize,
    imported_memory_count: u32,
) -> Result<Vec<FunctionCode>> {
    if pending.is_empty() {
        return Ok(Vec::new());
    }

    let (small_jobs, large_jobs): (Vec<_>, Vec<_>) =
        pending.into_iter().partition(|job| !should_parallelize_function(body_len(&job.body)));

    let mut codes = small_jobs
        .into_iter()
        .map(|job| process_function_job(job, options, func_types, imported_func_count, imported_memory_count))
        .collect::<Result<Vec<_>>>()?;

    if large_jobs.is_empty() {
        codes.sort_by_key(|(ordinal, _)| *ordinal);
        return Ok(codes.into_iter().map(|(_, code)| code).collect());
    }

    let num_workers = worker_count(options, large_jobs.len());
    if num_workers == 1 {
        codes.extend(
            large_jobs
                .into_iter()
                .map(|job| process_function_job(job, options, func_types, imported_func_count, imported_memory_count))
                .collect::<Result<Vec<_>>>()?,
        );
        codes.sort_by_key(|(ordinal, _)| *ordinal);
        return Ok(codes.into_iter().map(|(_, code)| code).collect());
    }

    let chunk_size = large_jobs.len().div_ceil(num_workers);
    let chunks = {
        let mut chunks = Vec::with_capacity(num_workers);
        let mut iter = large_jobs.into_iter();
        while let Some(first) = iter.next() {
            let mut chunk = alloc::vec![first];
            for _ in 1..chunk_size {
                match iter.next() {
                    Some(job) => chunk.push(job),
                    None => break,
                }
            }
            chunks.push(chunk);
        }
        chunks
    };

    let results: Vec<Result<(usize, FunctionCode)>> = std::thread::scope(|s| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                s.spawn(move || {
                    chunk
                        .into_iter()
                        .map(|job| {
                            process_function_job(job, options, func_types, imported_func_count, imported_memory_count)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();

        handles
            .into_iter()
            .flat_map(|handle| match handle.join() {
                Ok(results) => results,
                Err(_) => alloc::vec![Err(ParseError::Other("worker thread panicked".into()))],
            })
            .collect()
    });

    for result in results {
        let (ordinal, code) = result?;
        codes.push((ordinal, code));
    }

    codes.sort_by_key(|(ordinal, _)| *ordinal);
    Ok(codes.into_iter().map(|(_, code)| code).collect())
}

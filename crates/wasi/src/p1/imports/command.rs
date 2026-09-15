use tinywasm::FuncContext;

use crate::p1::abi::{Errno, OVERFLOW, SUCCESS};
use crate::p1::ctx::state;
use crate::p1::memory::GuestMemory;

use super::WasiResult;

pub(super) fn args_sizes_get(ctx: FuncContext<'_>, pointers: (i32, i32)) -> WasiResult<Errno> {
    let sizes = sizes(&state(&ctx)?.args)?;
    sizes_get(ctx, pointers, sizes)
}

pub(super) fn environ_sizes_get(ctx: FuncContext<'_>, pointers: (i32, i32)) -> WasiResult<Errno> {
    let sizes = sizes(&state(&ctx)?.env)?;
    sizes_get(ctx, pointers, sizes)
}

fn sizes_get(
    mut ctx: FuncContext<'_>,
    (count_ptr, size_ptr): (i32, i32),
    (count, size): (u32, u32),
) -> WasiResult<Errno> {
    let memory = GuestMemory::new(&ctx)?;
    memory.check_range(&ctx, count_ptr as u32 as usize, 4)?;
    memory.check_range(&ctx, size_ptr as u32 as usize, 4)?;
    memory.write(ctx.store_mut(), count_ptr, &count.to_le_bytes())?;
    Ok(memory.write(ctx.store_mut(), size_ptr, &size.to_le_bytes()).map_or_else(|errno| errno, |()| SUCCESS))
}

fn sizes(values: &[Vec<u8>]) -> Result<(u32, u32), Errno> {
    Ok((values.len().try_into().map_err(|_| OVERFLOW)?, string_array_size(values).ok_or(OVERFLOW)?))
}

pub(super) fn args_get(ctx: FuncContext<'_>, pointers: (i32, i32)) -> WasiResult<Errno> {
    let values = state(&ctx)?.args.clone();
    array_get(ctx, pointers, values)
}

pub(super) fn environ_get(ctx: FuncContext<'_>, pointers: (i32, i32)) -> WasiResult<Errno> {
    let values = state(&ctx)?.env.clone();
    array_get(ctx, pointers, values)
}

fn array_get(mut ctx: FuncContext<'_>, (array_ptr, buffer_ptr): (i32, i32), values: Vec<Vec<u8>>) -> WasiResult<Errno> {
    let Some(pointer_len) = values.len().checked_mul(4) else { return Ok(OVERFLOW) };
    let Some(buffer_len) = string_array_size(&values).map(|value| value as usize) else { return Ok(OVERFLOW) };
    let array_offset = array_ptr as u32 as usize;
    let buffer_offset = buffer_ptr as u32 as usize;
    let mut pointers = Vec::with_capacity(pointer_len);
    let mut buffer = Vec::with_capacity(buffer_len);
    for value in values {
        let Some(pointer) = buffer_offset.checked_add(buffer.len()).and_then(|value| u32::try_from(value).ok()) else {
            return Ok(OVERFLOW);
        };
        pointers.extend_from_slice(&pointer.to_le_bytes());
        buffer.extend_from_slice(&value);
        buffer.push(0);
    }
    let memory = GuestMemory::new(&ctx)?;
    memory.check_range(&ctx, array_offset, pointers.len())?;
    memory.check_range(&ctx, buffer_offset, buffer.len())?;
    memory.write_at(ctx.store_mut(), array_offset, &pointers)?;
    Ok(memory.write_at(ctx.store_mut(), buffer_offset, &buffer).map_or_else(|errno| errno, |()| SUCCESS))
}

fn string_array_size(values: &[Vec<u8>]) -> Option<u32> {
    values
        .iter()
        .try_fold(0usize, |size, value| size.checked_add(value.len())?.checked_add(1))
        .and_then(|size| u32::try_from(size).ok())
}

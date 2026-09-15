use tinywasm::FuncContext;

use crate::p1::abi::{Errno, INVAL, SUCCESS, realtime_now};
use crate::p1::ctx::state;
use crate::p1::memory::GuestMemory;

use super::WasiResult;

pub(super) fn clock_res_get(mut ctx: FuncContext<'_>, (clock_id, result_ptr): (i32, i32)) -> WasiResult<Errno> {
    if !matches!(clock_id, 0 | 1) {
        return Ok(INVAL);
    }
    let memory = GuestMemory::new(&ctx)?;
    Ok(memory.write(ctx.store_mut(), result_ptr, &1_u64.to_le_bytes()).map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn clock_time_get(
    mut ctx: FuncContext<'_>,
    (clock_id, _precision, result_ptr): (i32, i64, i32),
) -> WasiResult<Errno> {
    let timestamp = match clock_id {
        0 => realtime_now(),
        1 => state(&ctx)?.monotonic_start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX),
        _ => return Ok(INVAL),
    };
    let memory = GuestMemory::new(&ctx)?;
    Ok(memory.write(ctx.store_mut(), result_ptr, &timestamp.to_le_bytes()).map_or_else(|errno| errno, |()| SUCCESS))
}

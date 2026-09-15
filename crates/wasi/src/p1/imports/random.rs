use tinywasm::FuncContext;

use crate::p1::abi::{Errno, IO, SUCCESS};
use crate::p1::memory::GuestMemory;

use super::WasiResult;

pub(super) fn random_get(mut ctx: FuncContext<'_>, (buffer_ptr, buffer_len): (i32, i32)) -> WasiResult<Errno> {
    let memory = GuestMemory::new(&ctx)?;
    let offset = buffer_ptr as u32 as usize;
    let len = buffer_len as u32 as usize;
    memory.check_range(&ctx, offset, len)?;

    let mut chunk = [0_u8; 64 * 1024];
    let mut written = 0;
    while written < len {
        let chunk_len = (len - written).min(chunk.len());
        if getrandom::fill(&mut chunk[..chunk_len]).is_err() {
            return Ok(IO);
        }
        memory.write_at(ctx.store_mut(), offset + written, &chunk[..chunk_len])?;
        written += chunk_len;
    }
    Ok(SUCCESS)
}

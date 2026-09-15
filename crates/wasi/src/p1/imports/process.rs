use tinywasm::{Error, FuncContext};

use crate::p1::abi::{Errno, NOTSUP, SUCCESS};
use crate::p1::ctx::state_mut;

use super::WasiResult;

pub(super) fn proc_exit(mut ctx: FuncContext<'_>, status: i32) -> tinywasm::Result<()> {
    state_mut(&mut ctx)?.exit_status = Some(status as u32);
    Err(Error::Other("WASI process exited".into()))
}

pub(super) fn proc_raise(_ctx: FuncContext<'_>, _signal: i32) -> WasiResult<Errno> {
    Ok(NOTSUP)
}

pub(super) fn sched_yield(_ctx: FuncContext<'_>, (): ()) -> WasiResult<Errno> {
    std::thread::yield_now();
    Ok(SUCCESS)
}

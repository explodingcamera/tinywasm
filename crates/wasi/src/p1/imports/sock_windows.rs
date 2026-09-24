use tinywasm::FuncContext;

use crate::p1::abi::{Errno, NOTSUP};

use super::WasiResult;

pub(super) fn sock_accept(_ctx: FuncContext<'_>, _args: (i32, i32, i32)) -> WasiResult<Errno> {
    Ok(NOTSUP)
}

pub(super) fn sock_recv(_ctx: FuncContext<'_>, _args: (i32, i32, i32, i32, i32, i32)) -> WasiResult<Errno> {
    Ok(NOTSUP)
}

pub(super) fn sock_send(_ctx: FuncContext<'_>, _args: (i32, i32, i32, i32, i32)) -> WasiResult<Errno> {
    Ok(NOTSUP)
}

pub(super) fn sock_shutdown(_ctx: FuncContext<'_>, _args: (i32, i32)) -> WasiResult<Errno> {
    Ok(NOTSUP)
}

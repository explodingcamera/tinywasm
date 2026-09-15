use std::io::{IoSliceMut, Write};

use tinywasm::FuncContext;

use crate::p1::abi::*;
use crate::p1::ctx::{Descriptor, Resource, state, state_mut};
use crate::p1::memory::GuestMemory;

use super::WasiResult;

pub(super) fn sock_accept(mut ctx: FuncContext<'_>, (fd, flags, result_ptr): (i32, i32, i32)) -> WasiResult<Errno> {
    if flags as u32 > FDFLAGS_ALL.into() {
        return Ok(INVAL);
    }
    let flags = flags as u16;
    let memory = GuestMemory::new(&ctx)?;
    memory.check_range(&ctx, result_ptr as u32 as usize, 4)?;
    let descriptor = state(&ctx)?.descriptor(fd)?;
    descriptor.require_rights(RIGHT_SOCK_ACCEPT)?;
    let Resource::TcpListener(listener) = &descriptor.resource else { return Ok(NOTSOCK) };
    let rights = descriptor.inheriting_rights;
    let (stream, _) = listener.accept().map_err(|error| io_errno(&error))?;
    stream.set_nonblocking(flags & FDFLAG_NONBLOCK != 0).map_err(|error| io_errno(&error))?;
    let new_fd = state_mut(&mut ctx)?.insert_descriptor(Descriptor {
        resource: Resource::TcpStream(stream),
        rights,
        inheriting_rights: 0,
        flags,
        preopen_path: None,
    })?;
    if let Err(errno) = memory.write(ctx.store_mut(), result_ptr, &new_fd.to_le_bytes()) {
        let _ = state_mut(&mut ctx)?.descriptors[new_fd as usize].take();
        return Ok(errno);
    }
    Ok(SUCCESS)
}

pub(super) fn sock_recv(
    mut ctx: FuncContext<'_>,
    (fd, iovecs_ptr, iovecs_len, flags, data_len_ptr, result_flags_ptr): (i32, i32, i32, i32, i32, i32),
) -> WasiResult<Errno> {
    if flags as u32 & !3 != 0 {
        return Ok(INVAL);
    }
    let memory = GuestMemory::new(&ctx)?;
    let iovecs = memory.read_iovecs(&ctx, iovecs_ptr, iovecs_len)?;
    memory.check_range(&ctx, data_len_ptr as u32 as usize, 4)?;
    memory.check_range(&ctx, result_flags_ptr as u32 as usize, 2)?;
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(iovecs.byte_len).map_err(|_| NOMEM)?;
    buffer.resize(iovecs.byte_len, 0);
    let descriptor = state(&ctx)?.descriptor(fd)?;
    descriptor.require_rights(RIGHT_FD_READ)?;
    let mut recv_flags = rustix::net::RecvFlags::empty();
    if flags & 1 != 0 {
        recv_flags |= rustix::net::RecvFlags::PEEK;
    }
    if flags & 2 != 0 {
        recv_flags |= rustix::net::RecvFlags::WAITALL;
    }
    let (received, truncated) = match &descriptor.resource {
        Resource::TcpStream(stream) => {
            (rustix::net::recv(stream, &mut buffer[..], recv_flags).map_err(Errno::from)?.1, false)
        }
        Resource::UdpSocket(socket) => {
            let mut slices = [IoSliceMut::new(&mut buffer)];
            let mut control = rustix::net::RecvAncillaryBuffer::default();
            let message = rustix::net::recvmsg(socket, &mut slices, &mut control, recv_flags).map_err(Errno::from)?;
            (message.bytes, message.flags.contains(rustix::net::ReturnFlags::TRUNC))
        }
        _ => return Ok(NOTSOCK),
    };
    let written = received;
    memory.scatter(ctx.store_mut(), &iovecs, &buffer[..written])?;
    memory.write(ctx.store_mut(), data_len_ptr, &(written as u32).to_le_bytes())?;
    let result_flags = u16::from(truncated);
    Ok(memory
        .write(ctx.store_mut(), result_flags_ptr, &result_flags.to_le_bytes())
        .map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn sock_send(
    mut ctx: FuncContext<'_>,
    (fd, iovecs_ptr, iovecs_len, flags, data_len_ptr): (i32, i32, i32, i32, i32),
) -> WasiResult<Errno> {
    if flags != 0 {
        return Ok(INVAL);
    }
    let memory = GuestMemory::new(&ctx)?;
    let iovecs = memory.read_iovecs(&ctx, iovecs_ptr, iovecs_len)?;
    memory.check_range(&ctx, data_len_ptr as u32 as usize, 4)?;
    let data = memory.gather(&ctx, &iovecs)?;
    let descriptor = state(&ctx)?.descriptor(fd)?;
    descriptor.require_rights(RIGHT_FD_WRITE)?;
    let sent = match &descriptor.resource {
        Resource::TcpStream(stream) => {
            let mut stream = stream;
            stream.write(&data)
        }
        Resource::UdpSocket(socket) => socket.send(&data),
        _ => return Ok(NOTSOCK),
    }
    .map_err(|error| io_errno(&error))?;
    Ok(memory
        .write(ctx.store_mut(), data_len_ptr, &(sent as u32).to_le_bytes())
        .map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn sock_shutdown(ctx: FuncContext<'_>, (fd, how): (i32, i32)) -> WasiResult<Errno> {
    let how = match how {
        1 => rustix::net::Shutdown::Read,
        2 => rustix::net::Shutdown::Write,
        3 => rustix::net::Shutdown::Both,
        _ => return Ok(INVAL),
    };
    let descriptor = state(&ctx)?.descriptor(fd)?;
    descriptor.require_rights(RIGHT_SOCK_SHUTDOWN)?;
    let result = match &descriptor.resource {
        Resource::TcpStream(stream) => rustix::net::shutdown(stream, how),
        Resource::UdpSocket(socket) => rustix::net::shutdown(socket, how),
        _ => return Ok(NOTSOCK),
    };
    Ok(result.map_or_else(Errno::from, |()| SUCCESS))
}

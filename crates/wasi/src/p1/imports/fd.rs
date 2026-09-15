use std::io::{Read, Seek, SeekFrom, Write};

use cap_fs_ext::{DirEntryExt, DirExt, MetadataExt};
use cap_std::fs::FileExt;
use tinywasm::FuncContext;

use crate::p1::abi::*;
use crate::p1::ctx::{MAX_DESCRIPTORS, Resource, state, state_mut};
use crate::p1::memory::GuestMemory;

use super::WasiResult;

pub(super) fn fd_close(mut ctx: FuncContext<'_>, fd: i32) -> WasiResult<Errno> {
    let index = fd as u32 as usize;
    let Some(slot) = state_mut(&mut ctx)?.descriptors.get_mut(index) else { return Ok(BADF) };
    if slot.take().is_none() { Ok(BADF) } else { Ok(SUCCESS) }
}

pub(super) fn fd_renumber(mut ctx: FuncContext<'_>, (from, to): (i32, i32)) -> WasiResult<Errno> {
    let from = from as u32 as usize;
    let to = to as u32 as usize;
    let wasi = state_mut(&mut ctx)?;
    if from >= wasi.descriptors.len() || wasi.descriptors[from].is_none() {
        return Ok(BADF);
    }
    if from == to {
        return Ok(SUCCESS);
    }
    if to >= MAX_DESCRIPTORS {
        return Ok(MFILE);
    }
    if to >= wasi.descriptors.len() {
        wasi.descriptors.resize_with(to + 1, || None);
    }
    wasi.descriptors[to] = wasi.descriptors[from].take();
    Ok(SUCCESS)
}

pub(super) fn fd_fdstat_get(mut ctx: FuncContext<'_>, (fd, result_ptr): (i32, i32)) -> WasiResult<Errno> {
    let descriptor = state(&ctx)?.descriptor(fd)?;
    let mut bytes = [0; 24];
    bytes[0] = match &descriptor.resource {
        Resource::File(file) => file_type(&file.metadata().map_err(|error| io_errno(&error))?),
        _ => descriptor.file_type(),
    };
    put(&mut bytes, 2, descriptor.flags.to_le_bytes());
    put(&mut bytes, 8, descriptor.rights.to_le_bytes());
    put(&mut bytes, 16, descriptor.inheriting_rights.to_le_bytes());
    let memory = GuestMemory::new(&ctx)?;
    Ok(memory.write(ctx.store_mut(), result_ptr, &bytes).map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn fd_fdstat_set_flags(mut ctx: FuncContext<'_>, (fd, flags): (i32, i32)) -> WasiResult<Errno> {
    let flags = flags as u32;
    if flags > FDFLAGS_ALL.into() {
        return Ok(INVAL);
    }
    if flags & u32::from(FDFLAG_DSYNC | FDFLAG_RSYNC | FDFLAG_SYNC) != 0 {
        return Ok(INVAL);
    }
    let descriptor = state_mut(&mut ctx)?.descriptor_mut(fd)?;
    descriptor.require_rights(RIGHT_FD_FDSTAT_SET_FLAGS)?;
    let flags = flags as u16;
    let nonblocking = flags & FDFLAG_NONBLOCK != 0;
    match &descriptor.resource {
        Resource::TcpListener(listener) if flags == FDFLAG_NONBLOCK || flags == 0 => {
            listener.set_nonblocking(nonblocking).map_err(|error| io_errno(&error))?
        }
        Resource::TcpStream(stream) if flags == FDFLAG_NONBLOCK || flags == 0 => {
            stream.set_nonblocking(nonblocking).map_err(|error| io_errno(&error))?
        }
        Resource::UdpSocket(socket) if flags == FDFLAG_NONBLOCK || flags == 0 => {
            socket.set_nonblocking(nonblocking).map_err(|error| io_errno(&error))?
        }
        Resource::TcpListener(_) | Resource::TcpStream(_) | Resource::UdpSocket(_) => return Ok(INVAL),
        Resource::File(file) => set_file_status_flags(file, flags)?,
        Resource::Stdin | Resource::Stdout | Resource::Stderr | Resource::Directory(_) => return Ok(BADF),
    };
    descriptor.flags = flags;
    Ok(SUCCESS)
}

pub(super) fn fd_fdstat_set_rights(
    mut ctx: FuncContext<'_>,
    (fd, rights, inheriting_rights): (i32, i64, i64),
) -> WasiResult<Errno> {
    let descriptor = state_mut(&mut ctx)?.descriptor_mut(fd)?;
    let rights = rights as u64;
    let inheriting_rights = inheriting_rights as u64;
    if rights & !descriptor.rights != 0 || inheriting_rights & !descriptor.inheriting_rights != 0 {
        return Ok(NOTCAPABLE);
    }
    descriptor.rights = rights;
    descriptor.inheriting_rights = inheriting_rights;
    Ok(SUCCESS)
}

pub(super) fn fd_filestat_get(mut ctx: FuncContext<'_>, (fd, result_ptr): (i32, i32)) -> WasiResult<Errno> {
    let descriptor = state(&ctx)?.descriptor(fd)?;
    descriptor.require_rights(RIGHT_FD_FILESTAT_GET)?;
    let bytes = match &descriptor.resource {
        Resource::File(file) => filestat(&file.metadata().map_err(|error| io_errno(&error))?),
        Resource::Directory(dir) => filestat(&dir.dir_metadata().map_err(|error| io_errno(&error))?),
        Resource::Stdin
        | Resource::Stdout
        | Resource::Stderr
        | Resource::TcpListener(_)
        | Resource::TcpStream(_)
        | Resource::UdpSocket(_) => {
            let mut bytes = [0; 64];
            bytes[16] = descriptor.file_type();
            bytes
        }
    };
    let memory = GuestMemory::new(&ctx)?;
    Ok(memory.write(ctx.store_mut(), result_ptr, &bytes).map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn fd_filestat_set_size(mut ctx: FuncContext<'_>, (fd, size): (i32, i64)) -> WasiResult<Errno> {
    let descriptor = state_mut(&mut ctx)?.descriptor_mut(fd)?;
    descriptor.require_rights(RIGHT_FD_FILESTAT_SET_SIZE)?;
    let Resource::File(file) = &descriptor.resource else { return Ok(ISDIR) };
    Ok(file.set_len(size as u64).map_or_else(|error| io_errno(&error), |()| SUCCESS))
}

pub(super) fn fd_filestat_set_times(
    mut ctx: FuncContext<'_>,
    (fd, atim, mtim, flags): (i32, i64, i64, i32),
) -> WasiResult<Errno> {
    let flags = flags as u32;
    if flags > u32::from(FSTFLAGS_ALL)
        || flags & u32::from(FSTFLAG_ATIM | FSTFLAG_ATIM_NOW) == u32::from(FSTFLAG_ATIM | FSTFLAG_ATIM_NOW)
        || flags & u32::from(FSTFLAG_MTIM | FSTFLAG_MTIM_NOW) == u32::from(FSTFLAG_MTIM | FSTFLAG_MTIM_NOW)
    {
        return Ok(INVAL);
    }
    let descriptor = state_mut(&mut ctx)?.descriptor_mut(fd)?;
    descriptor.require_rights(RIGHT_FD_FILESTAT_SET_TIMES)?;
    if flags == 0 {
        return Ok(SUCCESS);
    }
    if let Resource::Directory(dir) = &descriptor.resource {
        let atime = time_spec(flags as u16, FSTFLAG_ATIM, FSTFLAG_ATIM_NOW, atim as u64);
        let mtime = time_spec(flags as u16, FSTFLAG_MTIM, FSTFLAG_MTIM_NOW, mtim as u64);
        return Ok(dir.set_times(".", atime, mtime).map_or_else(|error| io_errno(&error), |()| SUCCESS));
    }
    #[cfg(any(target_os = "android", target_os = "linux"))]
    {
        let Resource::File(file) = &descriptor.resource else { return Ok(NOTSUP) };
        let access = rustix_time(flags as u16, FSTFLAG_ATIM, FSTFLAG_ATIM_NOW, atim as u64)?;
        let modification = rustix_time(flags as u16, FSTFLAG_MTIM, FSTFLAG_MTIM_NOW, mtim as u64)?;
        let times = rustix::fs::Timestamps { last_access: access, last_modification: modification };
        Ok(rustix::fs::futimens(file, &times).map_or_else(Errno::from, |()| SUCCESS))
    }
    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    {
        let _ = (atim, mtim, descriptor);
        Ok(NOTSUP)
    }
}

pub(super) fn fd_read(ctx: FuncContext<'_>, args: (i32, i32, i32, i32)) -> WasiResult<Errno> {
    read_from_fd(ctx, args, None)
}

pub(super) fn fd_pread(
    ctx: FuncContext<'_>,
    (fd, iovecs_ptr, iovecs_len, offset, result_ptr): (i32, i32, i32, i64, i32),
) -> WasiResult<Errno> {
    read_from_fd(ctx, (fd, iovecs_ptr, iovecs_len, result_ptr), Some(offset as u64))
}

fn read_from_fd(
    mut ctx: FuncContext<'_>,
    (fd, iovecs_ptr, iovecs_len, result_ptr): (i32, i32, i32, i32),
    offset: Option<u64>,
) -> WasiResult<Errno> {
    let memory = GuestMemory::new(&ctx)?;
    let iovecs = memory.read_iovecs(&ctx, iovecs_ptr, iovecs_len)?;
    memory.check_range(&ctx, result_ptr as u32 as usize, 4)?;
    let descriptor = state_mut(&mut ctx)?.descriptor_mut(fd)?;
    descriptor.require_rights(RIGHT_FD_READ | if offset.is_some() { RIGHT_FD_SEEK } else { 0 })?;
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(iovecs.byte_len).map_err(|_| NOMEM)?;
    buffer.resize(iovecs.byte_len, 0);
    let read = match &mut descriptor.resource {
        Resource::Stdin => std::io::stdin().lock().read(&mut buffer),
        Resource::File(file) if let Some(offset) = offset => {
            file.read_at(&mut buffer, checked_offset(offset, iovecs.byte_len)?)
        }
        Resource::File(file) => file.read(&mut buffer),
        Resource::TcpStream(stream) => stream.read(&mut buffer),
        Resource::UdpSocket(socket) => socket.recv(&mut buffer),
        Resource::Directory(_) => return Ok(ISDIR),
        _ => return Ok(BADF),
    }
    .map_err(|error| io_errno(&error))?;
    memory.scatter(ctx.store_mut(), &iovecs, &buffer[..read])?;
    Ok(memory.write(ctx.store_mut(), result_ptr, &(read as u32).to_le_bytes()).map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn fd_write(ctx: FuncContext<'_>, args: (i32, i32, i32, i32)) -> WasiResult<Errno> {
    write_to_fd(ctx, args, None)
}

pub(super) fn fd_pwrite(
    ctx: FuncContext<'_>,
    (fd, iovecs_ptr, iovecs_len, offset, result_ptr): (i32, i32, i32, i64, i32),
) -> WasiResult<Errno> {
    write_to_fd(ctx, (fd, iovecs_ptr, iovecs_len, result_ptr), Some(offset as u64))
}

fn write_to_fd(
    mut ctx: FuncContext<'_>,
    (fd, iovecs_ptr, iovecs_len, result_ptr): (i32, i32, i32, i32),
    offset: Option<u64>,
) -> WasiResult<Errno> {
    let memory = GuestMemory::new(&ctx)?;
    let iovecs = memory.read_iovecs(&ctx, iovecs_ptr, iovecs_len)?;
    memory.check_range(&ctx, result_ptr as u32 as usize, 4)?;
    let data = memory.gather(&ctx, &iovecs)?;
    let descriptor = state_mut(&mut ctx)?.descriptor_mut(fd)?;
    descriptor.require_rights(RIGHT_FD_WRITE | if offset.is_some() { RIGHT_FD_SEEK } else { 0 })?;
    let written = match &mut descriptor.resource {
        Resource::Stdout => std::io::stdout().lock().write(&data),
        Resource::Stderr => std::io::stderr().lock().write(&data),
        Resource::File(file) if let Some(offset) = offset => file.write_at(&data, checked_offset(offset, data.len())?),
        Resource::File(file) => file.write(&data),
        Resource::TcpStream(stream) => stream.write(&data),
        Resource::UdpSocket(socket) => socket.send(&data),
        Resource::Directory(_) => return Ok(ISDIR),
        _ => return Ok(BADF),
    }
    .map_err(|error| io_errno(&error))?;
    Ok(memory
        .write(ctx.store_mut(), result_ptr, &(written as u32).to_le_bytes())
        .map_or_else(|errno| errno, |()| SUCCESS))
}

fn checked_offset(offset: u64, len: usize) -> Result<u64, Errno> {
    offset.checked_add(len as u64).map(|_| offset).ok_or(OVERFLOW)
}

#[cfg(unix)]
fn set_file_status_flags(file: &cap_std::fs::File, flags: u16) -> Result<(), Errno> {
    let mut host_flags = rustix::fs::fcntl_getfl(file).map_err(Errno::from)?;
    host_flags.set(rustix::fs::OFlags::APPEND, flags & FDFLAG_APPEND != 0);
    host_flags.set(rustix::fs::OFlags::NONBLOCK, flags & FDFLAG_NONBLOCK != 0);
    rustix::fs::fcntl_setfl(file, host_flags).map_err(Errno::from)
}

#[cfg(not(unix))]
fn set_file_status_flags(_file: &cap_std::fs::File, _flags: u16) -> Result<(), Errno> {
    Err(NOTSUP)
}

pub(super) fn fd_seek(
    mut ctx: FuncContext<'_>,
    (fd, offset, whence, result_ptr): (i32, i64, i32, i32),
) -> WasiResult<Errno> {
    let memory = GuestMemory::new(&ctx)?;
    memory.check_range(&ctx, result_ptr as u32 as usize, 8)?;
    let position = match whence {
        0 if offset >= 0 => SeekFrom::Start(offset as u64),
        0 => return Ok(INVAL),
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => return Ok(INVAL),
    };
    let descriptor = state_mut(&mut ctx)?.descriptor_mut(fd)?;
    descriptor.require_rights(if whence == 1 && offset == 0 { RIGHT_FD_TELL } else { RIGHT_FD_SEEK })?;
    let Resource::File(file) = &mut descriptor.resource else { return Ok(SPIPE) };
    let offset = file.seek(position).map_err(|error| io_errno(&error))?;
    Ok(memory.write(ctx.store_mut(), result_ptr, &offset.to_le_bytes()).map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn fd_tell(mut ctx: FuncContext<'_>, (fd, result_ptr): (i32, i32)) -> WasiResult<Errno> {
    let descriptor = state_mut(&mut ctx)?.descriptor_mut(fd)?;
    descriptor.require_rights(RIGHT_FD_TELL)?;
    let Resource::File(file) = &mut descriptor.resource else { return Ok(SPIPE) };
    let offset = file.stream_position().map_err(|error| io_errno(&error))?;
    let memory = GuestMemory::new(&ctx)?;
    Ok(memory.write(ctx.store_mut(), result_ptr, &offset.to_le_bytes()).map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn fd_sync(ctx: FuncContext<'_>, fd: i32) -> WasiResult<Errno> {
    sync_fd(ctx, fd, false)
}

pub(super) fn fd_datasync(ctx: FuncContext<'_>, fd: i32) -> WasiResult<Errno> {
    sync_fd(ctx, fd, true)
}

fn sync_fd(mut ctx: FuncContext<'_>, fd: i32, data_only: bool) -> WasiResult<Errno> {
    let descriptor = state_mut(&mut ctx)?.descriptor_mut(fd)?;
    descriptor.require_rights(if data_only { RIGHT_FD_DATASYNC } else { RIGHT_FD_SYNC })?;
    let Resource::File(file) = &descriptor.resource else { return Ok(INVAL) };
    Ok((if data_only { file.sync_data() } else { file.sync_all() }).map_or_else(|error| io_errno(&error), |()| SUCCESS))
}

pub(super) fn fd_allocate(mut ctx: FuncContext<'_>, (fd, offset, len): (i32, i64, i64)) -> WasiResult<Errno> {
    let offset = offset as u64;
    let len = len as u64;
    if offset.checked_add(len).is_none() {
        return Ok(FBIG);
    }
    let descriptor = state_mut(&mut ctx)?.descriptor_mut(fd)?;
    descriptor.require_rights(RIGHT_FD_ALLOCATE)?;
    let Resource::File(_) = &descriptor.resource else { return Ok(BADF) };
    Ok(NOTSUP)
}

pub(super) fn fd_advise(
    mut ctx: FuncContext<'_>,
    (fd, offset, len, advice): (i32, i64, i64, i32),
) -> WasiResult<Errno> {
    if !(0..=5).contains(&advice) {
        return Ok(INVAL);
    }
    let descriptor = state_mut(&mut ctx)?.descriptor_mut(fd)?;
    descriptor.require_rights(RIGHT_FD_ADVISE)?;
    let Resource::File(file) = &descriptor.resource else { return Ok(BADF) };
    if (offset as u64).checked_add(len as u64).is_none() {
        return Ok(OVERFLOW);
    }
    #[cfg(any(target_os = "android", target_os = "linux"))]
    {
        let advice = match advice {
            0 => rustix::fs::Advice::Normal,
            1 => rustix::fs::Advice::Sequential,
            2 => rustix::fs::Advice::Random,
            3 => rustix::fs::Advice::WillNeed,
            4 => rustix::fs::Advice::DontNeed,
            5 => rustix::fs::Advice::NoReuse,
            _ => unreachable!(),
        };
        Ok(rustix::fs::fadvise(file, offset as u64, std::num::NonZeroU64::new(len as u64), advice)
            .map_or_else(Errno::from, |()| SUCCESS))
    }
    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    {
        let _ = (file, offset, len);
        Ok(SUCCESS)
    }
}

pub(super) fn fd_prestat_get(mut ctx: FuncContext<'_>, (fd, result_ptr): (i32, i32)) -> WasiResult<Errno> {
    let descriptor = state(&ctx)?.descriptor(fd)?;
    let Some(path) = &descriptor.preopen_path else { return Ok(BADF) };
    let Some(len) = u32::try_from(path.len()).ok() else { return Ok(OVERFLOW) };
    let mut bytes = [0; 8];
    put(&mut bytes, 4, len.to_le_bytes());
    let memory = GuestMemory::new(&ctx)?;
    Ok(memory.write(ctx.store_mut(), result_ptr, &bytes).map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn fd_prestat_dir_name(
    mut ctx: FuncContext<'_>,
    (fd, path_ptr, path_len): (i32, i32, i32),
) -> WasiResult<Errno> {
    let descriptor = state(&ctx)?.descriptor(fd)?;
    let Some(path) = descriptor.preopen_path.clone() else { return Ok(BADF) };
    if path.len() > path_len as u32 as usize {
        return Ok(NAMETOOLONG);
    }
    let memory = GuestMemory::new(&ctx)?;
    Ok(memory.write(ctx.store_mut(), path_ptr, &path).map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn fd_readdir(
    mut ctx: FuncContext<'_>,
    (fd, buffer_ptr, buffer_len, cookie, used_ptr): (i32, i32, i32, i64, i32),
) -> WasiResult<Errno> {
    let capacity = buffer_len as u32 as usize;
    let memory = GuestMemory::new(&ctx)?;
    memory.check_range(&ctx, buffer_ptr as u32 as usize, buffer_len as u32 as usize)?;
    memory.check_range(&ctx, used_ptr as u32 as usize, 4)?;
    let descriptor = state(&ctx)?.descriptor(fd)?;
    descriptor.require_rights(RIGHT_FD_READDIR)?;
    let Resource::Directory(dir) = &descriptor.resource else { return Ok(NOTDIR) };
    let entries = dir.entries().map_err(|error| io_errno(&error))?;
    let cookie = usize::try_from(cookie as u64).map_err(|_| OVERFLOW)?;
    let mut output = Vec::new();
    output.try_reserve_exact(capacity).map_err(|_| NOMEM)?;
    for (index, entry) in entries.enumerate().skip(cookie) {
        let entry = entry.map_err(|error| io_errno(&error))?;
        let name = os_bytes(&entry.file_name());
        let metadata = entry.full_metadata().map_err(|error| io_errno(&error))?;
        let mut header = [0; 24];
        let Some(next) = u64::try_from(index).ok().and_then(|index| index.checked_add(1)) else {
            return Ok(OVERFLOW);
        };
        put(&mut header, 0, next.to_le_bytes());
        put(&mut header, 8, metadata.ino().to_le_bytes());
        put(&mut header, 16, u32::try_from(name.len()).unwrap_or(u32::MAX).to_le_bytes());
        header[20] = file_type(&metadata);
        let remaining = capacity.saturating_sub(output.len());
        output.extend_from_slice(&header[..remaining.min(header.len())]);
        let remaining = capacity.saturating_sub(output.len());
        output.extend_from_slice(&name[..remaining.min(name.len())]);
        if output.len() == capacity {
            break;
        }
    }
    memory.write(ctx.store_mut(), buffer_ptr, &output)?;
    Ok(memory
        .write(ctx.store_mut(), used_ptr, &(output.len() as u32).to_le_bytes())
        .map_or_else(|errno| errno, |()| SUCCESS))
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn rustix_time(flags: u16, explicit: u16, now: u16, timestamp: u64) -> Result<rustix::fs::Timespec, Errno> {
    if flags & now != 0 {
        Ok(rustix::fs::Timespec { tv_sec: 0, tv_nsec: rustix::fs::UTIME_NOW })
    } else if flags & explicit != 0 {
        Ok(rustix::fs::Timespec {
            tv_sec: i64::try_from(timestamp / 1_000_000_000).map_err(|_| OVERFLOW)?,
            tv_nsec: (timestamp % 1_000_000_000) as _,
        })
    } else {
        Ok(rustix::fs::Timespec { tv_sec: 0, tv_nsec: rustix::fs::UTIME_OMIT })
    }
}

use std::path::PathBuf;

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsMaybeDirExt, OpenOptionsSyncExt};
use cap_std::fs::{Dir, OpenOptions};
use tinywasm::FuncContext;

use crate::p1::abi::*;
use crate::p1::ctx::{Descriptor, Resource, state, state_mut};
use crate::p1::memory::GuestMemory;

use super::WasiResult;

const MAX_PATH_BYTES: usize = 4096;

pub(super) fn path_create_directory(
    ctx: FuncContext<'_>,
    (fd, path_ptr, path_len): (i32, i32, i32),
) -> WasiResult<Errno> {
    let path = guest_path(&ctx, path_ptr, path_len)?;
    let dir = directory(&ctx, fd, RIGHT_PATH_CREATE_DIRECTORY)?;
    Ok(dir.create_dir(path).map_or_else(|error| path_errno(&error), |()| SUCCESS))
}

pub(super) fn path_remove_directory(
    ctx: FuncContext<'_>,
    (fd, path_ptr, path_len): (i32, i32, i32),
) -> WasiResult<Errno> {
    let path = guest_path(&ctx, path_ptr, path_len)?;
    let dir = directory(&ctx, fd, RIGHT_PATH_REMOVE_DIRECTORY)?;
    Ok(dir.remove_dir(path).map_or_else(|error| path_errno(&error), |()| SUCCESS))
}

pub(super) fn path_unlink_file(ctx: FuncContext<'_>, (fd, path_ptr, path_len): (i32, i32, i32)) -> WasiResult<Errno> {
    let path = guest_path(&ctx, path_ptr, path_len)?;
    let dir = directory(&ctx, fd, RIGHT_PATH_UNLINK_FILE)?;
    Ok(dir.remove_file_or_symlink(path).map_or_else(|error| path_errno(&error), |()| SUCCESS))
}

pub(super) fn path_open(
    mut ctx: FuncContext<'_>,
    (fd, lookup_flags, path_ptr, path_len, open_flags, rights, inheriting_rights, fd_flags, result_ptr): (
        i32,
        i32,
        i32,
        i32,
        i32,
        i64,
        i64,
        i32,
        i32,
    ),
) -> WasiResult<Errno> {
    let lookup_flags = lookup_flags as u32;
    if lookup_flags & !LOOKUP_SYMLINK_FOLLOW != 0
        || open_flags as u32 > OFLAGS_ALL.into()
        || fd_flags as u32 > FDFLAGS_ALL.into()
    {
        return Ok(INVAL);
    }
    let open_flags = open_flags as u16;
    let fd_flags = fd_flags as u16;
    if fd_flags & (FDFLAG_DSYNC | FDFLAG_RSYNC | FDFLAG_SYNC) != 0 {
        return Ok(NOTSUP);
    }
    let follow = lookup_flags & LOOKUP_SYMLINK_FOLLOW != 0;
    let create = open_flags & OFLAG_CREAT != 0;
    let exclusive = open_flags & OFLAG_EXCL != 0;
    let truncate = open_flags & OFLAG_TRUNC != 0;
    if exclusive && !create {
        return Ok(INVAL);
    }
    if open_flags & OFLAG_DIRECTORY != 0 && (create || truncate) {
        return Ok(ISDIR);
    }
    let path = guest_path(&ctx, path_ptr, path_len)?;
    let memory = GuestMemory::new(&ctx)?;
    memory.check_range(&ctx, result_ptr as u32 as usize, 4)?;
    let parent = state(&ctx)?.descriptor(fd)?;
    parent.require_rights(RIGHT_PATH_OPEN)?;
    let rights = rights as u64;
    let inheriting_rights = inheriting_rights as u64;
    if rights & !parent.inheriting_rights != 0 || inheriting_rights & !parent.inheriting_rights != 0 {
        return Ok(NOTCAPABLE);
    }
    if create && parent.rights & RIGHT_PATH_CREATE_FILE == 0 {
        return Ok(NOTCAPABLE);
    }
    if truncate && parent.rights & RIGHT_PATH_FILESTAT_SET_SIZE == 0 {
        return Ok(NOTCAPABLE);
    }
    let Resource::Directory(parent_dir) = &parent.resource else { return Ok(NOTDIR) };
    let parent_dir = parent_dir.try_clone().map_err(|error| path_errno(&error))?;
    let mut options = OpenOptions::new();
    let write = create || truncate || rights & (RIGHT_FD_WRITE | RIGHT_FD_ALLOCATE | RIGHT_FD_FILESTAT_SET_SIZE) != 0;
    options
        .read(rights & RIGHT_FD_READ != 0 || !write)
        .write(write)
        .append(write && fd_flags & FDFLAG_APPEND != 0)
        .create(create)
        .create_new(exclusive)
        .truncate(truncate)
        .follow(if follow { FollowSymlinks::Yes } else { FollowSymlinks::No })
        .nonblock(fd_flags & FDFLAG_NONBLOCK != 0)
        .maybe_dir(true);
    let file = parent_dir.open_with(&path, &options).map_err(|error| path_errno(&error))?;
    let descriptor = if file.metadata().map_err(|error| path_errno(&error))?.is_dir() {
        Descriptor {
            resource: Resource::Directory(Dir::reopen_dir(&file).map_err(|error| path_errno(&error))?),
            rights: rights & DIRECTORY_RIGHTS,
            inheriting_rights,
            flags: fd_flags,
            preopen_path: None,
        }
    } else {
        if open_flags & OFLAG_DIRECTORY != 0 {
            return Ok(NOTDIR);
        }
        Descriptor {
            resource: Resource::File(file),
            rights: rights & FILE_RIGHTS,
            inheriting_rights: 0,
            flags: fd_flags,
            preopen_path: None,
        }
    };
    let new_fd = state_mut(&mut ctx)?.insert_descriptor(descriptor)?;
    if let Err(errno) = memory.write(ctx.store_mut(), result_ptr, &new_fd.to_le_bytes()) {
        let _ = state_mut(&mut ctx)?.descriptors[new_fd as usize].take();
        return Ok(errno);
    }
    Ok(SUCCESS)
}

pub(super) fn path_filestat_get(
    mut ctx: FuncContext<'_>,
    (fd, lookup_flags, path_ptr, path_len, result_ptr): (i32, i32, i32, i32, i32),
) -> WasiResult<Errno> {
    let lookup_flags = lookup_flags as u32;
    if lookup_flags & !LOOKUP_SYMLINK_FOLLOW != 0 {
        return Ok(INVAL);
    }
    let follow = lookup_flags & LOOKUP_SYMLINK_FOLLOW != 0;
    let path = guest_path(&ctx, path_ptr, path_len)?;
    let dir = directory(&ctx, fd, RIGHT_PATH_FILESTAT_GET)?;
    let metadata = if follow { dir.metadata(&path) } else { dir.symlink_metadata(&path) };
    let metadata = metadata.map_err(|error| path_errno(&error))?;
    let bytes = filestat(&metadata);
    let memory = GuestMemory::new(&ctx)?;
    Ok(memory.write(ctx.store_mut(), result_ptr, &bytes).map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn path_filestat_set_times(
    ctx: FuncContext<'_>,
    (fd, lookup_flags, path_ptr, path_len, atim, mtim, flags): (i32, i32, i32, i32, i64, i64, i32),
) -> WasiResult<Errno> {
    let lookup_flags = lookup_flags as u32;
    let flags = flags as u32;
    if lookup_flags & !LOOKUP_SYMLINK_FOLLOW != 0
        || flags > FSTFLAGS_ALL.into()
        || flags & u32::from(FSTFLAG_ATIM | FSTFLAG_ATIM_NOW) == u32::from(FSTFLAG_ATIM | FSTFLAG_ATIM_NOW)
        || flags & u32::from(FSTFLAG_MTIM | FSTFLAG_MTIM_NOW) == u32::from(FSTFLAG_MTIM | FSTFLAG_MTIM_NOW)
    {
        return Ok(INVAL);
    }
    let follow = lookup_flags & LOOKUP_SYMLINK_FOLLOW != 0;
    let path = guest_path(&ctx, path_ptr, path_len)?;
    let dir = directory(&ctx, fd, RIGHT_PATH_FILESTAT_SET_TIMES)?;
    if flags == 0 {
        return Ok(SUCCESS);
    }
    let atime = time_spec(flags as u16, FSTFLAG_ATIM, FSTFLAG_ATIM_NOW, atim as u64);
    let mtime = time_spec(flags as u16, FSTFLAG_MTIM, FSTFLAG_MTIM_NOW, mtim as u64);
    let result = if follow { dir.set_times(&path, atime, mtime) } else { dir.set_symlink_times(&path, atime, mtime) };
    Ok(result.map_or_else(|error| path_errno(&error), |()| SUCCESS))
}

pub(super) fn path_link(
    ctx: FuncContext<'_>,
    (old_fd, flags, old_ptr, old_len, new_fd, new_ptr, new_len): (i32, i32, i32, i32, i32, i32, i32),
) -> WasiResult<Errno> {
    if flags != 0 {
        return Ok(INVAL);
    }
    let old_path = guest_path(&ctx, old_ptr, old_len)?;
    let new_path = guest_path(&ctx, new_ptr, new_len)?;
    let old_dir = directory(&ctx, old_fd, RIGHT_PATH_LINK_SOURCE)?;
    let new_dir = directory(&ctx, new_fd, RIGHT_PATH_LINK_TARGET)?;
    Ok(old_dir.hard_link(old_path, &new_dir, new_path).map_or_else(|error| path_errno(&error), |()| SUCCESS))
}

pub(super) fn path_rename(
    ctx: FuncContext<'_>,
    (old_fd, old_ptr, old_len, new_fd, new_ptr, new_len): (i32, i32, i32, i32, i32, i32),
) -> WasiResult<Errno> {
    let old_path = guest_path(&ctx, old_ptr, old_len)?;
    let new_path = guest_path(&ctx, new_ptr, new_len)?;
    let old_dir = directory(&ctx, old_fd, RIGHT_PATH_RENAME_SOURCE)?;
    let new_dir = directory(&ctx, new_fd, RIGHT_PATH_RENAME_TARGET)?;
    Ok(old_dir.rename(old_path, &new_dir, new_path).map_or_else(|error| path_errno(&error), |()| SUCCESS))
}

pub(super) fn path_readlink(
    mut ctx: FuncContext<'_>,
    (fd, path_ptr, path_len, buffer_ptr, buffer_len, used_ptr): (i32, i32, i32, i32, i32, i32),
) -> WasiResult<Errno> {
    let memory = GuestMemory::new(&ctx)?;
    memory.check_range(&ctx, buffer_ptr as u32 as usize, buffer_len as u32 as usize)?;
    memory.check_range(&ctx, used_ptr as u32 as usize, 4)?;
    let path = guest_path(&ctx, path_ptr, path_len)?;
    let dir = directory(&ctx, fd, RIGHT_PATH_READLINK)?;
    let target = dir.read_link_contents(path).map_err(|error| path_errno(&error))?;
    let bytes = os_bytes(target.as_os_str());
    let written = bytes.len().min(buffer_len as u32 as usize);
    memory.write(ctx.store_mut(), buffer_ptr, &bytes[..written])?;
    Ok(memory
        .write(ctx.store_mut(), used_ptr, &(written as u32).to_le_bytes())
        .map_or_else(|errno| errno, |()| SUCCESS))
}

pub(super) fn path_symlink(
    ctx: FuncContext<'_>,
    (old_ptr, old_len, fd, new_ptr, new_len): (i32, i32, i32, i32, i32),
) -> WasiResult<Errno> {
    let memory = GuestMemory::new(&ctx)?;
    if old_len as u32 as usize > MAX_PATH_BYTES {
        return Ok(NAMETOOLONG);
    }
    let target_bytes = memory.read(&ctx, old_ptr, old_len)?;
    if target_bytes.contains(&0) {
        return Ok(ILSEQ);
    }
    let target = PathBuf::from(std::str::from_utf8(&target_bytes).map_err(|_| ILSEQ)?);
    let new_path = guest_path(&ctx, new_ptr, new_len)?;
    let dir = directory(&ctx, fd, RIGHT_PATH_SYMLINK)?;
    Ok(DirExt::symlink(&dir, target, new_path).map_or_else(|error| path_errno(&error), |()| SUCCESS))
}

fn directory(ctx: &FuncContext<'_>, fd: i32, right: u64) -> Result<Dir, Errno> {
    let descriptor = state(ctx).map_err(|_| IO)?.descriptor(fd)?;
    descriptor.require_rights(right)?;
    let Resource::Directory(dir) = &descriptor.resource else { return Err(NOTDIR) };
    dir.try_clone().map_err(|error| path_errno(&error))
}

fn guest_path(ctx: &FuncContext<'_>, pointer: i32, len: i32) -> Result<PathBuf, Errno> {
    if len as u32 as usize > MAX_PATH_BYTES {
        return Err(NAMETOOLONG);
    }
    let memory = GuestMemory::new(ctx).map_err(|_| IO)?;
    let bytes = memory.read(ctx, pointer, len)?;
    if bytes.is_empty() {
        return Err(NOENT);
    }
    if bytes.contains(&0) {
        return Err(INVAL);
    }
    Ok(PathBuf::from(std::str::from_utf8(&bytes).map_err(|_| ILSEQ)?))
}

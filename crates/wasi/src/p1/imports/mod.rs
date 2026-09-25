mod clock;
mod command;
mod fd;
mod path;
mod poll;
mod process;
mod random;
#[cfg(unix)]
mod sock;
#[cfg(not(unix))]
#[path = "sock_windows.rs"]
mod sock;

use tinywasm::{HostFunction, Imports};

use super::abi::Errno;

const MODULE: &str = "wasi_snapshot_preview1";

type WasiResult<T> = Result<T, WasiError>;

enum WasiError {
    Errno(Errno),
    Host(tinywasm::Error),
}

impl From<Errno> for WasiError {
    fn from(errno: Errno) -> Self {
        Self::Errno(errno)
    }
}

impl From<tinywasm::Error> for WasiError {
    fn from(error: tinywasm::Error) -> Self {
        Self::Host(error)
    }
}

pub(super) fn register(imports: &mut Imports) {
    register_import(imports, "args_get", command::args_get);
    register_import(imports, "args_sizes_get", command::args_sizes_get);
    register_import(imports, "environ_get", command::environ_get);
    register_import(imports, "environ_sizes_get", command::environ_sizes_get);
    register_import(imports, "clock_res_get", clock::clock_res_get);
    register_import(imports, "clock_time_get", clock::clock_time_get);
    register_import(imports, "fd_advise", fd::fd_advise);
    register_import(imports, "fd_allocate", fd::fd_allocate);
    register_import(imports, "fd_close", fd::fd_close);
    register_import(imports, "fd_datasync", fd::fd_datasync);
    register_import(imports, "fd_fdstat_get", fd::fd_fdstat_get);
    register_import(imports, "fd_fdstat_set_flags", fd::fd_fdstat_set_flags);
    register_import(imports, "fd_fdstat_set_rights", fd::fd_fdstat_set_rights);
    register_import(imports, "fd_filestat_get", fd::fd_filestat_get);
    register_import(imports, "fd_filestat_set_size", fd::fd_filestat_set_size);
    register_import(imports, "fd_filestat_set_times", fd::fd_filestat_set_times);
    register_import(imports, "fd_pread", fd::fd_pread);
    register_import(imports, "fd_prestat_get", fd::fd_prestat_get);
    register_import(imports, "fd_prestat_dir_name", fd::fd_prestat_dir_name);
    register_import(imports, "fd_pwrite", fd::fd_pwrite);
    register_import(imports, "fd_read", fd::fd_read);
    register_import(imports, "fd_readdir", fd::fd_readdir);
    register_import(imports, "fd_renumber", fd::fd_renumber);
    register_import(imports, "fd_seek", fd::fd_seek);
    register_import(imports, "fd_sync", fd::fd_sync);
    register_import(imports, "fd_tell", fd::fd_tell);
    register_import(imports, "fd_write", fd::fd_write);
    register_import(imports, "path_create_directory", path::path_create_directory);
    register_import(imports, "path_filestat_get", path::path_filestat_get);
    register_import(imports, "path_filestat_set_times", path::path_filestat_set_times);
    register_import(imports, "path_link", path::path_link);
    register_import(imports, "path_open", path::path_open);
    register_import(imports, "path_readlink", path::path_readlink);
    register_import(imports, "path_remove_directory", path::path_remove_directory);
    register_import(imports, "path_rename", path::path_rename);
    register_import(imports, "path_symlink", path::path_symlink);
    register_import(imports, "path_unlink_file", path::path_unlink_file);
    register_import(imports, "poll_oneoff", poll::poll_oneoff);
    imports.define(MODULE, "proc_exit", HostFunction::from(process::proc_exit));
    register_import(imports, "proc_raise", process::proc_raise);
    register_import(imports, "sched_yield", process::sched_yield);
    register_import(imports, "random_get", random::random_get);
    register_import(imports, "sock_accept", sock::sock_accept);
    register_import(imports, "sock_recv", sock::sock_recv);
    register_import(imports, "sock_send", sock::sock_send);
    register_import(imports, "sock_shutdown", sock::sock_shutdown);
}

fn register_import<P>(
    imports: &mut Imports,
    name: &str,
    function: fn(tinywasm::FuncContext<'_>, P) -> WasiResult<Errno>,
) where
    P: tinywasm::FromWasmValues + 'static,
{
    imports.define(
        MODULE,
        name,
        HostFunction::from(move |ctx: tinywasm::FuncContext<'_>, args: P| -> tinywasm::Result<i32> {
            match function(ctx, args) {
                Ok(errno) | Err(WasiError::Errno(errno)) => Ok(errno.into()),
                Err(WasiError::Host(error)) => Err(error),
            }
        }),
    );
}

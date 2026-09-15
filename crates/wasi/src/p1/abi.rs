use std::ffi::OsStr;
use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cap_fs_ext::{MetadataExt, SystemTimeSpec};
use cap_std::fs::Metadata;

#[derive(Clone, Copy)]
pub(super) struct Errno(u16);

impl From<Errno> for i32 {
    fn from(errno: Errno) -> Self {
        i32::from(errno.0)
    }
}

pub(super) const SUCCESS: Errno = Errno(0);
pub(super) const ACCESS: Errno = Errno(2);
pub(super) const ADDRINUSE: Errno = Errno(3);
pub(super) const ADDRNOTAVAIL: Errno = Errno(4);
pub(super) const AGAIN: Errno = Errno(6);
pub(super) const BADF: Errno = Errno(8);
pub(super) const CONNABORTED: Errno = Errno(13);
pub(super) const CONNREFUSED: Errno = Errno(14);
pub(super) const CONNRESET: Errno = Errno(15);
pub(super) const EXIST: Errno = Errno(20);
pub(super) const FAULT: Errno = Errno(21);
pub(super) const FBIG: Errno = Errno(22);
pub(super) const ILSEQ: Errno = Errno(25);
pub(super) const INTR: Errno = Errno(27);
pub(super) const INVAL: Errno = Errno(28);
pub(super) const IO: Errno = Errno(29);
pub(super) const ISDIR: Errno = Errno(31);
pub(super) const LOOP: Errno = Errno(32);
pub(super) const MFILE: Errno = Errno(33);
pub(super) const MLINK: Errno = Errno(34);
pub(super) const NAMETOOLONG: Errno = Errno(37);
pub(super) const NOMEM: Errno = Errno(48);
pub(super) const NOENT: Errno = Errno(44);
pub(super) const NOSPC: Errno = Errno(51);
pub(super) const NOTCONN: Errno = Errno(53);
pub(super) const NOTDIR: Errno = Errno(54);
pub(super) const NOTEMPTY: Errno = Errno(55);
pub(super) const NOTSOCK: Errno = Errno(57);
pub(super) const NOTSUP: Errno = Errno(58);
pub(super) const OVERFLOW: Errno = Errno(61);
pub(super) const PIPE: Errno = Errno(64);
pub(super) const ROFS: Errno = Errno(69);
pub(super) const SPIPE: Errno = Errno(70);
pub(super) const TIMEDOUT: Errno = Errno(73);
pub(super) const XDEV: Errno = Errno(75);
pub(super) const NOTCAPABLE: Errno = Errno(76);

pub(super) const FILETYPE_UNKNOWN: u8 = 0;
pub(super) const FILETYPE_BLOCK_DEVICE: u8 = 1;
pub(super) const FILETYPE_CHARACTER_DEVICE: u8 = 2;
pub(super) const FILETYPE_DIRECTORY: u8 = 3;
pub(super) const FILETYPE_REGULAR_FILE: u8 = 4;
pub(super) const FILETYPE_SOCKET_DGRAM: u8 = 5;
pub(super) const FILETYPE_SOCKET_STREAM: u8 = 6;
pub(super) const FILETYPE_SYMBOLIC_LINK: u8 = 7;

pub(super) const FDFLAG_APPEND: u16 = 1;
pub(super) const FDFLAG_DSYNC: u16 = 2;
pub(super) const FDFLAG_NONBLOCK: u16 = 4;
pub(super) const FDFLAG_RSYNC: u16 = 8;
pub(super) const FDFLAG_SYNC: u16 = 16;
pub(super) const FDFLAGS_ALL: u16 = 31;

pub(super) const OFLAG_CREAT: u16 = 1;
pub(super) const OFLAG_DIRECTORY: u16 = 2;
pub(super) const OFLAG_EXCL: u16 = 4;
pub(super) const OFLAG_TRUNC: u16 = 8;
pub(super) const OFLAGS_ALL: u16 = 15;
pub(super) const LOOKUP_SYMLINK_FOLLOW: u32 = 1;

pub(super) const RIGHT_FD_DATASYNC: u64 = 1 << 0;
pub(super) const RIGHT_FD_READ: u64 = 1 << 1;
pub(super) const RIGHT_FD_SEEK: u64 = 1 << 2;
pub(super) const RIGHT_FD_FDSTAT_SET_FLAGS: u64 = 1 << 3;
pub(super) const RIGHT_FD_SYNC: u64 = 1 << 4;
pub(super) const RIGHT_FD_TELL: u64 = 1 << 5;
pub(super) const RIGHT_FD_WRITE: u64 = 1 << 6;
pub(super) const RIGHT_FD_ADVISE: u64 = 1 << 7;
pub(super) const RIGHT_FD_ALLOCATE: u64 = 1 << 8;
pub(super) const RIGHT_PATH_CREATE_DIRECTORY: u64 = 1 << 9;
pub(super) const RIGHT_PATH_CREATE_FILE: u64 = 1 << 10;
pub(super) const RIGHT_PATH_LINK_SOURCE: u64 = 1 << 11;
pub(super) const RIGHT_PATH_LINK_TARGET: u64 = 1 << 12;
pub(super) const RIGHT_PATH_OPEN: u64 = 1 << 13;
pub(super) const RIGHT_FD_READDIR: u64 = 1 << 14;
pub(super) const RIGHT_PATH_READLINK: u64 = 1 << 15;
pub(super) const RIGHT_PATH_RENAME_SOURCE: u64 = 1 << 16;
pub(super) const RIGHT_PATH_RENAME_TARGET: u64 = 1 << 17;
pub(super) const RIGHT_PATH_FILESTAT_GET: u64 = 1 << 18;
pub(super) const RIGHT_PATH_FILESTAT_SET_SIZE: u64 = 1 << 19;
pub(super) const RIGHT_PATH_FILESTAT_SET_TIMES: u64 = 1 << 20;
pub(super) const RIGHT_FD_FILESTAT_GET: u64 = 1 << 21;
pub(super) const RIGHT_FD_FILESTAT_SET_SIZE: u64 = 1 << 22;
pub(super) const RIGHT_FD_FILESTAT_SET_TIMES: u64 = 1 << 23;
pub(super) const RIGHT_PATH_SYMLINK: u64 = 1 << 24;
pub(super) const RIGHT_PATH_REMOVE_DIRECTORY: u64 = 1 << 25;
pub(super) const RIGHT_PATH_UNLINK_FILE: u64 = 1 << 26;
pub(super) const RIGHT_POLL_FD_READWRITE: u64 = 1 << 27;
pub(super) const RIGHT_SOCK_SHUTDOWN: u64 = 1 << 28;
pub(super) const RIGHT_SOCK_ACCEPT: u64 = 1 << 29;
pub(super) const FILE_RIGHTS: u64 = RIGHT_FD_DATASYNC
    | RIGHT_FD_READ
    | RIGHT_FD_SEEK
    | RIGHT_FD_FDSTAT_SET_FLAGS
    | RIGHT_FD_SYNC
    | RIGHT_FD_TELL
    | RIGHT_FD_WRITE
    | RIGHT_FD_ADVISE
    | RIGHT_FD_ALLOCATE
    | RIGHT_FD_FILESTAT_GET
    | RIGHT_FD_FILESTAT_SET_SIZE
    | RIGHT_FD_FILESTAT_SET_TIMES
    | RIGHT_POLL_FD_READWRITE;
pub(super) const DIRECTORY_RIGHTS: u64 = RIGHT_PATH_CREATE_DIRECTORY
    | RIGHT_PATH_CREATE_FILE
    | RIGHT_PATH_LINK_SOURCE
    | RIGHT_PATH_LINK_TARGET
    | RIGHT_PATH_OPEN
    | RIGHT_FD_READDIR
    | RIGHT_PATH_READLINK
    | RIGHT_PATH_RENAME_SOURCE
    | RIGHT_PATH_RENAME_TARGET
    | RIGHT_PATH_FILESTAT_GET
    | RIGHT_PATH_FILESTAT_SET_SIZE
    | RIGHT_PATH_FILESTAT_SET_TIMES
    | RIGHT_FD_FILESTAT_GET
    | RIGHT_FD_FILESTAT_SET_TIMES
    | RIGHT_PATH_SYMLINK
    | RIGHT_PATH_REMOVE_DIRECTORY
    | RIGHT_PATH_UNLINK_FILE;

pub(super) const FSTFLAG_ATIM: u16 = 1;
pub(super) const FSTFLAG_ATIM_NOW: u16 = 2;
pub(super) const FSTFLAG_MTIM: u16 = 4;
pub(super) const FSTFLAG_MTIM_NOW: u16 = 8;
pub(super) const FSTFLAGS_ALL: u16 = 15;

pub(super) fn io_errno(error: &io::Error) -> Errno {
    use io::ErrorKind;
    match error.kind() {
        ErrorKind::NotFound => NOENT,
        ErrorKind::PermissionDenied => ACCESS,
        ErrorKind::AddrInUse => ADDRINUSE,
        ErrorKind::AddrNotAvailable => ADDRNOTAVAIL,
        ErrorKind::AlreadyExists => EXIST,
        ErrorKind::ConnectionAborted => CONNABORTED,
        ErrorKind::ConnectionRefused => CONNREFUSED,
        ErrorKind::ConnectionReset => CONNRESET,
        ErrorKind::InvalidInput | ErrorKind::InvalidData => INVAL,
        ErrorKind::WouldBlock => AGAIN,
        ErrorKind::WriteZero | ErrorKind::UnexpectedEof => IO,
        ErrorKind::BrokenPipe => PIPE,
        ErrorKind::StorageFull => NOSPC,
        ErrorKind::NotADirectory => NOTDIR,
        ErrorKind::NotConnected => NOTCONN,
        ErrorKind::IsADirectory => ISDIR,
        ErrorKind::DirectoryNotEmpty => NOTEMPTY,
        ErrorKind::FileTooLarge => FBIG,
        ErrorKind::CrossesDevices => XDEV,
        ErrorKind::Interrupted => INTR,
        ErrorKind::OutOfMemory => NOMEM,
        ErrorKind::ReadOnlyFilesystem => ROFS,
        ErrorKind::TooManyLinks => MLINK,
        ErrorKind::TimedOut => TIMEDOUT,
        ErrorKind::Unsupported => NOTSUP,
        _ => platform_io_errno(error),
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn platform_io_errno(error: &io::Error) -> Errno {
    match error.raw_os_error() {
        Some(36) => NAMETOOLONG,
        Some(40) => LOOP,
        _ => IO,
    }
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
fn platform_io_errno(_error: &io::Error) -> Errno {
    IO
}

impl From<rustix::io::Errno> for Errno {
    fn from(error: rustix::io::Errno) -> Self {
        use rustix::io::Errno as E;
        match error {
            E::INTR => INTR,
            E::ACCESS | E::PERM => ACCESS,
            E::BADF => BADF,
            E::INVAL => INVAL,
            E::MFILE => MFILE,
            E::NOMEM => NOMEM,
            E::NOSPC => NOSPC,
            E::NOSYS | E::NOTSUP => NOTSUP,
            E::XDEV => XDEV,
            E::LOOP => LOOP,
            _ => io_errno(&io::Error::from(error)),
        }
    }
}

pub(super) fn path_errno(error: &io::Error) -> Errno {
    if error.kind() == io::ErrorKind::PermissionDenied { NOTCAPABLE } else { io_errno(error) }
}

pub(super) fn file_type(metadata: &Metadata) -> u8 {
    use cap_fs_ext::FileTypeExt;

    let ty = metadata.file_type();
    if ty.is_dir() {
        FILETYPE_DIRECTORY
    } else if ty.is_file() {
        FILETYPE_REGULAR_FILE
    } else if ty.is_symlink() {
        FILETYPE_SYMBOLIC_LINK
    } else if ty.is_block_device() {
        FILETYPE_BLOCK_DEVICE
    } else if ty.is_char_device() {
        FILETYPE_CHARACTER_DEVICE
    } else if ty.is_socket() {
        FILETYPE_SOCKET_STREAM
    } else {
        FILETYPE_UNKNOWN
    }
}

fn timestamp(time: io::Result<cap_std::time::SystemTime>) -> u64 {
    time.ok()
        .and_then(|time| time.into_std().duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
        .unwrap_or(0)
}

pub(super) fn realtime_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
        .unwrap_or(0)
}

pub(super) fn filestat(metadata: &Metadata) -> [u8; 64] {
    let mut bytes = [0; 64];
    put(&mut bytes, 0, metadata.dev().to_le_bytes());
    put(&mut bytes, 8, metadata.ino().to_le_bytes());
    bytes[16] = file_type(metadata);
    put(&mut bytes, 24, metadata.nlink().to_le_bytes());
    put(&mut bytes, 32, metadata.len().to_le_bytes());
    put(&mut bytes, 40, timestamp(metadata.accessed()).to_le_bytes());
    put(&mut bytes, 48, timestamp(metadata.modified()).to_le_bytes());
    put(&mut bytes, 56, status_change_timestamp(metadata).to_le_bytes());
    bytes
}

fn status_change_timestamp(metadata: &Metadata) -> u64 {
    #[cfg(unix)]
    {
        let seconds = cap_std::fs::MetadataExt::ctime(metadata);
        let nanoseconds = cap_std::fs::MetadataExt::ctime_nsec(metadata);
        if seconds < 0 || nanoseconds < 0 {
            return 0;
        }
        (seconds as u64).saturating_mul(1_000_000_000).saturating_add(nanoseconds as u64)
    }
    #[cfg(not(unix))]
    timestamp(metadata.created())
}

pub(super) fn time_spec(flags: u16, explicit: u16, now: u16, timestamp: u64) -> Option<SystemTimeSpec> {
    if flags & now != 0 {
        Some(SystemTimeSpec::SymbolicNow)
    } else if flags & explicit != 0 {
        Some(SystemTimeSpec::Absolute(cap_std::time::SystemTime::from_std(
            UNIX_EPOCH + Duration::from_nanos(timestamp),
        )))
    } else {
        None
    }
}

pub(super) fn os_bytes(value: &OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        value.as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    value.to_string_lossy().into_owned().into_bytes()
}

pub(super) fn put<const N: usize>(buffer: &mut [u8], offset: usize, bytes: [u8; N]) {
    buffer[offset..offset + N].copy_from_slice(&bytes);
}

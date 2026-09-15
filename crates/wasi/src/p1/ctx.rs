use std::fmt::{Display, Formatter};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::Path;

use cap_std::ambient_authority;
use cap_std::fs::{Dir, File};
use tinywasm::{Error, FuncContext};

use super::abi::*;

pub(super) const MAX_DESCRIPTORS: usize = 4096;
const SOCKET_RIGHTS: u64 = RIGHT_FD_READ
    | RIGHT_FD_WRITE
    | RIGHT_FD_FDSTAT_SET_FLAGS
    | RIGHT_FD_FILESTAT_GET
    | RIGHT_POLL_FD_READWRITE
    | RIGHT_SOCK_SHUTDOWN;

pub(super) enum Resource {
    Stdin,
    Stdout,
    Stderr,
    File(File),
    Directory(Dir),
    TcpListener(TcpListener),
    TcpStream(TcpStream),
    UdpSocket(UdpSocket),
}

pub(super) struct Descriptor {
    pub(super) resource: Resource,
    pub(super) rights: u64,
    pub(super) inheriting_rights: u64,
    pub(super) flags: u16,
    pub(super) preopen_path: Option<Vec<u8>>,
}

impl Descriptor {
    pub(super) fn file_type(&self) -> u8 {
        match &self.resource {
            Resource::Stdin | Resource::Stdout | Resource::Stderr => FILETYPE_CHARACTER_DEVICE,
            Resource::File(_) => FILETYPE_REGULAR_FILE,
            Resource::Directory(_) => FILETYPE_DIRECTORY,
            Resource::TcpListener(_) | Resource::TcpStream(_) => FILETYPE_SOCKET_STREAM,
            Resource::UdpSocket(_) => FILETYPE_SOCKET_DGRAM,
        }
    }

    pub(super) fn require_rights(&self, rights: u64) -> Result<(), Errno> {
        if self.rights & rights == rights { Ok(()) } else { Err(super::abi::NOTCAPABLE) }
    }
}

/// Configures the host resources available to a WASI Preview 1 command.
pub struct WasiCtx {
    pub(super) args: Vec<Vec<u8>>,
    pub(super) env: Vec<Vec<u8>>,
    pub(super) descriptors: Vec<Option<Descriptor>>,
    pub(super) exit_status: Option<u32>,
    pub(super) monotonic_start: std::time::Instant,
}

impl Default for WasiCtx {
    fn default() -> Self {
        Self {
            args: Vec::new(),
            env: Vec::new(),
            descriptors: vec![None, None, None],
            exit_status: None,
            monotonic_start: std::time::Instant::now(),
        }
    }
}

impl WasiCtx {
    /// Creates an empty context without access to host standard I/O or files.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets UTF-8 command-line arguments visible to the guest.
    pub fn with_args(self, args: impl IntoIterator<Item = impl Into<String>>) -> Result<Self, WasiCtxError> {
        self.with_args_bytes(args.into_iter().map(|value| value.into().into_bytes()))
    }

    /// Sets byte command-line arguments visible to the guest.
    pub fn with_args_bytes(mut self, args: impl IntoIterator<Item = impl Into<Vec<u8>>>) -> Result<Self, WasiCtxError> {
        self.args = args.into_iter().map(|value| validate_bytes(value.into())).collect::<Result<_, _>>()?;
        Ok(self)
    }

    /// Sets UTF-8 environment variables visible to the guest.
    pub fn with_env(
        self,
        env: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Result<Self, WasiCtxError> {
        self.with_env_bytes(env.into_iter().map(|(key, value)| (key.into().into_bytes(), value.into().into_bytes())))
    }

    /// Sets byte environment variables visible to the guest.
    pub fn with_env_bytes(
        mut self,
        env: impl IntoIterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
    ) -> Result<Self, WasiCtxError> {
        self.env = env
            .into_iter()
            .map(|(key, value)| {
                let mut entry = key.into();
                entry.push(b'=');
                entry.extend(value.into());
                validate_bytes(entry)
            })
            .collect::<Result<_, _>>()?;
        Ok(self)
    }

    /// Allows the guest to read inherited standard input.
    pub fn inherit_stdin(self) -> Self {
        self.inherit_fd(0, Resource::Stdin, RIGHT_FD_READ)
    }

    /// Allows the guest to write inherited standard output.
    pub fn inherit_stdout(self) -> Self {
        self.inherit_fd(1, Resource::Stdout, RIGHT_FD_WRITE)
    }

    /// Allows the guest to write inherited standard error.
    pub fn inherit_stderr(self) -> Self {
        self.inherit_fd(2, Resource::Stderr, RIGHT_FD_WRITE)
    }

    /// Allows the guest to use inherited standard input, output, and error.
    pub fn inherit_stdio(self) -> Self {
        self.inherit_stdin().inherit_stdout().inherit_stderr()
    }

    /// Adds a capability directory and returns it as a guest preopen at fd 3 or above.
    pub fn preopen_dir(
        mut self,
        host_path: impl AsRef<Path>,
        guest_path: impl AsRef<[u8]>,
    ) -> Result<Self, WasiCtxError> {
        let guest_path = validate_bytes(guest_path.as_ref().to_vec())?;
        if guest_path.is_empty() {
            return Err(WasiCtxError::InvalidPreopenPath);
        }
        let dir = Dir::open_ambient_dir(host_path, ambient_authority()).map_err(WasiCtxError::Io)?;
        self.insert_descriptor(Descriptor {
            resource: Resource::Directory(dir),
            rights: DIRECTORY_RIGHTS,
            inheriting_rights: DIRECTORY_RIGHTS | FILE_RIGHTS,
            flags: 0,
            preopen_path: Some(guest_path),
        })
        .map_err(|_| WasiCtxError::TooManyDescriptors)?;
        Ok(self)
    }

    /// Inserts a TCP listener capability and returns its guest file descriptor.
    pub fn insert_tcp_listener(&mut self, listener: TcpListener) -> Result<u32, WasiCtxError> {
        listener.set_nonblocking(false).map_err(WasiCtxError::Io)?;
        self.insert_descriptor(Descriptor {
            resource: Resource::TcpListener(listener),
            rights: RIGHT_FD_READ
                | RIGHT_FD_FDSTAT_SET_FLAGS
                | RIGHT_FD_FILESTAT_GET
                | RIGHT_POLL_FD_READWRITE
                | RIGHT_SOCK_ACCEPT,
            inheriting_rights: SOCKET_RIGHTS,
            flags: 0,
            preopen_path: None,
        })
        .map_err(|_| WasiCtxError::TooManyDescriptors)
    }

    /// Inserts a connected TCP stream capability and returns its guest file descriptor.
    pub fn insert_tcp_stream(&mut self, stream: TcpStream) -> Result<u32, WasiCtxError> {
        self.insert_socket(Resource::TcpStream(stream))
    }

    /// Inserts a UDP socket capability and returns its guest file descriptor.
    ///
    /// `sock_send` requires the supplied socket to be connected. Receiving is
    /// supported for connected and unconnected sockets.
    pub fn insert_udp_socket(&mut self, socket: UdpSocket) -> Result<u32, WasiCtxError> {
        self.insert_socket(Resource::UdpSocket(socket))
    }

    /// Returns the status supplied to `proc_exit`, if it was called.
    pub fn exit_status(&self) -> Option<u32> {
        self.exit_status
    }

    /// Takes and clears the status supplied to `proc_exit`.
    pub fn take_exit_status(&mut self) -> Option<u32> {
        self.exit_status.take()
    }

    pub(super) fn descriptor(&self, fd: i32) -> Result<&Descriptor, Errno> {
        self.descriptors.get(fd as u32 as usize).and_then(Option::as_ref).ok_or(super::abi::BADF)
    }

    pub(super) fn descriptor_mut(&mut self, fd: i32) -> Result<&mut Descriptor, Errno> {
        self.descriptors.get_mut(fd as u32 as usize).and_then(Option::as_mut).ok_or(super::abi::BADF)
    }

    pub(super) fn insert_descriptor(&mut self, descriptor: Descriptor) -> Result<u32, Errno> {
        if let Some((index, slot)) = self.descriptors.iter_mut().enumerate().skip(3).find(|(_, slot)| slot.is_none()) {
            *slot = Some(descriptor);
            Ok(index as u32)
        } else {
            if self.descriptors.len() >= MAX_DESCRIPTORS {
                return Err(super::abi::MFILE);
            }
            let fd = self.descriptors.len() as u32;
            self.descriptors.push(Some(descriptor));
            Ok(fd)
        }
    }

    fn inherit_fd(mut self, fd: usize, resource: Resource, rights: u64) -> Self {
        self.descriptors[fd] = Some(Descriptor {
            resource,
            rights: rights | RIGHT_FD_FILESTAT_GET | RIGHT_POLL_FD_READWRITE,
            inheriting_rights: 0,
            flags: 0,
            preopen_path: None,
        });
        self
    }

    fn insert_socket(&mut self, resource: Resource) -> Result<u32, WasiCtxError> {
        match &resource {
            Resource::TcpStream(stream) => stream.set_nonblocking(false),
            Resource::UdpSocket(socket) => socket.set_nonblocking(false),
            _ => unreachable!(),
        }
        .map_err(WasiCtxError::Io)?;
        self.insert_descriptor(Descriptor {
            resource,
            rights: SOCKET_RIGHTS,
            inheriting_rights: 0,
            flags: 0,
            preopen_path: None,
        })
        .map_err(|_| WasiCtxError::TooManyDescriptors)
    }
}

/// An invalid WASI context configuration.
#[derive(Debug)]
pub enum WasiCtxError {
    /// An argument, environment variable, or guest path contains a nul byte.
    NulByte,
    /// A preopen guest path is empty.
    InvalidPreopenPath,
    /// A host preopen directory could not be opened.
    Io(std::io::Error),
    /// The context has reached its descriptor limit.
    TooManyDescriptors,
}

impl Display for WasiCtxError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NulByte => f.write_str("WASI strings cannot contain nul bytes"),
            Self::InvalidPreopenPath => f.write_str("WASI preopen guest paths cannot be empty"),
            Self::Io(error) => write!(f, "failed to configure WASI host resource: {error}"),
            Self::TooManyDescriptors => f.write_str("WASI descriptor limit reached"),
        }
    }
}

impl std::error::Error for WasiCtxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

fn validate_bytes(value: Vec<u8>) -> Result<Vec<u8>, WasiCtxError> {
    if value.contains(&0) { Err(WasiCtxError::NulByte) } else { Ok(value) }
}

pub(super) fn state<'a>(ctx: &'a FuncContext<'_>) -> tinywasm::Result<&'a WasiCtx> {
    ctx.state::<WasiCtx>().ok_or_else(|| Error::Other("WasiCtx is missing from the store".into()))
}

pub(super) fn state_mut<'a>(ctx: &'a mut FuncContext<'_>) -> tinywasm::Result<&'a mut WasiCtx> {
    ctx.state_mut::<WasiCtx>().ok_or_else(|| Error::Other("WasiCtx is missing from the store".into()))
}

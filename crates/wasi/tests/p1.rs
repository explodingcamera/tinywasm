use tinywasm::{Imports, ModuleInstance, Store};
use tinywasm_wasi::p1::{WasiCtx, imports, register};

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "tinywasm-wasi-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn module(source: &str) -> tinywasm::Module {
    tinywasm::parse_bytes(&wat::parse_str(source).unwrap()).unwrap()
}

#[test]
fn links_the_frozen_preview1_surface() -> tinywasm::Result<()> {
    let module = module(
        r#"(module
            (import "wasi_snapshot_preview1" "clock_res_get" (func (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_advise" (func (param i32 i64 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_allocate" (func (param i32 i64 i64) (result i32)))
            (import "wasi_snapshot_preview1" "fd_close" (func (param i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_datasync" (func (param i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_fdstat_set_flags" (func (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_fdstat_set_rights" (func (param i32 i64 i64) (result i32)))
            (import "wasi_snapshot_preview1" "fd_filestat_set_size" (func (param i32 i64) (result i32)))
            (import "wasi_snapshot_preview1" "fd_filestat_set_times" (func (param i32 i64 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_pread" (func (param i32 i32 i32 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_pwrite" (func (param i32 i32 i32 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_readdir" (func (param i32 i32 i32 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_renumber" (func (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_seek" (func (param i32 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_sync" (func (param i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_tell" (func (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_create_directory" (func (param i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_filestat_get" (func (param i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_filestat_set_times" (func (param i32 i32 i32 i32 i64 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_link" (func (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_readlink" (func (param i32 i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_remove_directory" (func (param i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_rename" (func (param i32 i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_symlink" (func (param i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_unlink_file" (func (param i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "poll_oneoff" (func (param i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "proc_raise" (func (param i32) (result i32)))
            (import "wasi_snapshot_preview1" "sched_yield" (func (result i32)))
            (import "wasi_snapshot_preview1" "sock_accept" (func (param i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "sock_recv" (func (param i32 i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "sock_send" (func (param i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "sock_shutdown" (func (param i32 i32) (result i32))))"#,
    );
    let mut store = Store::default().with_state(WasiCtx::new());
    let mut imports = Imports::new();
    register(&mut imports);
    ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    Ok(())
}

#[test]
fn writes_arguments_and_environment() -> tinywasm::Result<()> {
    let module = module(
        r#"(module
            (import "wasi_snapshot_preview1" "args_sizes_get" (func $args_sizes_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "args_get" (func $args_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "environ_sizes_get" (func $environ_sizes_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "environ_get" (func $environ_get (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "run") (result i32)
                i32.const 0 i32.const 4 call $args_sizes_get drop
                i32.const 8 i32.const 32 call $args_get drop
                i32.const 16 i32.const 20 call $environ_sizes_get drop
                i32.const 24 i32.const 48 call $environ_get))"#,
    );
    let wasi = WasiCtx::new().with_args(["app.wasm", "one"]).unwrap().with_env([("MODE", "test")]).unwrap();
    let mut store = Store::default().with_state(wasi);
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports()))?;
    assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, 0);

    let memory = instance.memory("memory")?;
    let mut sizes = [0; 24];
    memory.read_exact(&store, 0, &mut sizes)?;
    assert_eq!(u32::from_le_bytes(sizes[0..4].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(sizes[4..8].try_into().unwrap()), 13);
    assert_eq!(u32::from_le_bytes(sizes[16..20].try_into().unwrap()), 1);
    assert_eq!(u32::from_le_bytes(sizes[20..24].try_into().unwrap()), 10);
    assert_eq!(memory.read_vec(&store, 32, 13)?, b"app.wasm\0one\0");
    assert_eq!(memory.read_vec(&store, 48, 10)?, b"MODE=test\0");
    Ok(())
}

#[test]
fn handles_process_exit_and_unsupported_raise() -> tinywasm::Result<()> {
    let exit_module = module(
        r#"(module
            (import "wasi_snapshot_preview1" "proc_exit" (func $proc_exit (param i32)))
            (func (export "_start")
                i32.const 7
                call $proc_exit
                unreachable))"#,
    );
    let mut store = Store::default().with_state(WasiCtx::new());
    let instance = ModuleInstance::instantiate(&mut store, &exit_module, Some(&imports()))?;
    assert!(instance.func::<(), ()>(&store, "_start")?.call(&mut store, ()).is_err());
    let wasi = store.state_mut::<WasiCtx>().unwrap();
    assert_eq!(wasi.exit_status(), Some(7));
    assert_eq!(wasi.take_exit_status(), Some(7));
    assert_eq!(wasi.exit_status(), None);

    let raise_module = module(
        r#"(module
            (import "wasi_snapshot_preview1" "proc_raise" (func $proc_raise (param i32) (result i32)))
            (func (export "run") (result i32)
                i32.const 15 call $proc_raise))"#,
    );
    let mut store = Store::default().with_state(WasiCtx::new());
    let instance = ModuleInstance::instantiate(&mut store, &raise_module, Some(&imports()))?;
    assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, 58);
    Ok(())
}

#[test]
fn uses_preopened_files_and_enforces_descriptor_rights() -> tinywasm::Result<()> {
    let temp = TempDir::new("preopen-rights");
    std::fs::write(temp.0.join("hello.txt"), b"hello").unwrap();
    let read_module = module(
        r#"(module
            (import "wasi_snapshot_preview1" "fd_prestat_get" (func $prestat (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_prestat_dir_name" (func $prestat_name (param i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_filestat_get" (func $stat (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "\c8\00\00\00\05\00\00\00")
            (data (i32.const 100) "hello.txt")
            (func (export "run") (result i32)
                i32.const 3 i32.const 20 call $prestat drop
                i32.const 3 i32.const 32 i32.const 16 call $prestat_name drop
                i32.const 3 i32.const 1 i32.const 100 i32.const 9 i32.const 0
                i64.const 2097154 i64.const 0 i32.const 0 i32.const 12 call $open drop
                i32.const 12 i32.load i32.const 0 i32.const 1 i32.const 16 call $read drop
                i32.const 12 i32.load i32.const 256 call $stat))"#,
    );
    let wasi = WasiCtx::new().preopen_dir(&temp.0, "/sandbox").unwrap();
    let mut store = Store::default().with_state(wasi);
    let instance = ModuleInstance::instantiate(&mut store, &read_module, Some(&imports()))?;
    assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, 0);
    let memory = instance.memory("memory")?;
    assert_eq!(memory.read_vec(&store, 32, 8)?, b"/sandbox");
    assert_eq!(memory.read_vec(&store, 200, 5)?, b"hello");
    assert_eq!(memory.read_vec(&store, 256 + 16, 1)?, [4]);
    let stat = memory.read_vec(&store, 256, 64)?;
    assert_eq!(u64::from_le_bytes(stat[32..40].try_into().unwrap()), 5);

    let rights_module = module(
        r#"(module
            (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_fdstat_set_rights" (func $rights (param i32 i64 i64) (result i32)))
            (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "\80\00\00\00\01\00\00\00")
            (data (i32.const 32) "hello.txt")
            (func (export "run") (result i32)
                i32.const 3 i32.const 1 i32.const 32 i32.const 9 i32.const 0
                i64.const 2 i64.const 0 i32.const 0 i32.const 64 call $open drop
                i32.const 64 i32.load i64.const 0 i64.const 0 call $rights drop
                i32.const 64 i32.load i32.const 0 i32.const 1 i32.const 68 call $read))"#,
    );
    let wasi = WasiCtx::new().preopen_dir(&temp.0, "/").unwrap();
    let mut store = Store::default().with_state(wasi);
    let instance = ModuleInstance::instantiate(&mut store, &rights_module, Some(&imports()))?;
    assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, 76);
    Ok(())
}

#[test]
fn prevents_preopen_capability_escape() -> tinywasm::Result<()> {
    let parent = TempDir::new("escape");
    let sandbox = parent.0.join("sandbox");
    std::fs::create_dir(&sandbox).unwrap();
    std::fs::write(parent.0.join("outside.txt"), b"secret").unwrap();
    let module = module(
        r#"(module
            (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 32) "../outside.txt")
            (func (export "run") (result i32)
                i32.const 3 i32.const 1 i32.const 32 i32.const 14 i32.const 0
                i64.const 2 i64.const 0 i32.const 0 i32.const 0 call $open))"#,
    );
    let mut store = Store::default().with_state(WasiCtx::new().preopen_dir(&sandbox, "/").unwrap());
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports()))?;
    assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, 76);
    assert_eq!(std::fs::read(parent.0.join("outside.txt")).unwrap(), b"secret");
    Ok(())
}

#[test]
fn provides_clocks_randomness_and_timer_polling() -> tinywasm::Result<()> {
    let module = module(
        r#"(module
            (import "wasi_snapshot_preview1" "clock_time_get" (func $clock (param i32 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "random_get" (func $random (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "poll_oneoff" (func $poll (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "\2a\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00\00\00\00\00\40\42\0f\00\00\00\00\00\01\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00")
            (func (export "run") (result i32)
                i32.const 0 i32.const 64 i32.const 1 i32.const 128 call $poll drop
                i32.const 0 i64.const 1 i32.const 160 call $clock drop
                i32.const 1 i64.const 1 i32.const 168 call $clock drop
                i32.const 192 i32.const 32 call $random))"#,
    );
    let mut store = Store::default().with_state(WasiCtx::new());
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports()))?;
    assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, 0);
    let memory = instance.memory("memory")?;
    let event = memory.read_vec(&store, 64, 32)?;
    assert_eq!(u64::from_le_bytes(event[0..8].try_into().unwrap()), 42);
    assert_eq!(u16::from_le_bytes(event[8..10].try_into().unwrap()), 0);
    assert_eq!(event[10], 0);
    assert_eq!(memory.read_vec(&store, 128, 4)?, 1_u32.to_le_bytes());
    assert_ne!(u64::from_le_bytes(memory.read_vec(&store, 160, 8)?.try_into().unwrap()), 0);
    assert!(memory.read_vec(&store, 192, 32)?.iter().any(|byte| *byte != 0));
    Ok(())
}

#[test]
fn polls_accepts_and_uses_tcp_streams() -> tinywasm::Result<()> {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = std::thread::spawn(move || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream.write_all(b"hello").unwrap();
        let mut reply = [0; 5];
        stream.read_exact(&mut reply).unwrap();
        reply
    });
    let module = module(
        r#"(module
            (import "wasi_snapshot_preview1" "poll_oneoff" (func $poll (param i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "sock_accept" (func $accept (param i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "sock_recv" (func $recv (param i32 i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "sock_send" (func $send (param i32 i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "\64\00\00\00\05\00\00\00\78\00\00\00\05\00\00\00")
            (data (i32.const 120) "world")
            (data (i32.const 160) "\2a\00\00\00\00\00\00\00\01\00\00\00\00\00\00\00\03\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00")
            (func (export "run") (result i32)
                i32.const 160 i32.const 224 i32.const 1 i32.const 256 call $poll drop
                i32.const 3 i32.const 0 i32.const 32 call $accept drop
                i32.const 32 i32.load i32.const 0 i32.const 1 i32.const 0 i32.const 16 i32.const 20 call $recv drop
                i32.const 32 i32.load i32.const 8 i32.const 1 i32.const 0 i32.const 24 call $send))"#,
    );
    let mut wasi = WasiCtx::new();
    assert_eq!(wasi.insert_tcp_listener(listener).unwrap(), 3);
    let mut store = Store::default().with_state(wasi);
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports()))?;
    assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, 0);
    let memory = instance.memory("memory")?;
    let event = memory.read_vec(&store, 224, 32)?;
    assert_eq!(u64::from_le_bytes(event[0..8].try_into().unwrap()), 42);
    assert_eq!(event[10], 1);
    assert_eq!(memory.read_vec(&store, 256, 4)?, 1_u32.to_le_bytes());
    assert_eq!(memory.read_vec(&store, 100, 5)?, b"hello");
    assert_eq!(memory.read_vec(&store, 16, 4)?, 5_u32.to_le_bytes());
    assert_eq!(memory.read_vec(&store, 24, 4)?, 5_u32.to_le_bytes());
    assert_eq!(peer.join().unwrap(), *b"world");
    Ok(())
}

#[test]
fn sends_and_receives_udp_datagrams() -> tinywasm::Result<()> {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let peer = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.connect(peer.local_addr().unwrap()).unwrap();
    peer.connect(socket.local_addr().unwrap()).unwrap();
    peer.send(b"hello!").unwrap();
    let module = module(
        r#"(module
            (import "wasi_snapshot_preview1" "fd_fdstat_set_flags" (func $set_flags (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "sock_recv" (func $recv (param i32 i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "sock_send" (func $send (param i32 i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "\64\00\00\00\05\00\00\00\78\00\00\00\05\00\00\00")
            (data (i32.const 120) "world")
            (func (export "run") (result i32)
                i32.const 3 i32.const 0 i32.const 1 i32.const 0 i32.const 16 i32.const 20 call $recv drop
                i32.const 3 i32.const 8 i32.const 1 i32.const 0 i32.const 24 call $send)
            (func (export "nonblocking_recv") (result i32)
                i32.const 3 i32.const 4 call $set_flags drop
                i32.const 3 i32.const 0 i32.const 1 i32.const 0 i32.const 16 i32.const 20 call $recv))"#,
    );
    let mut wasi = WasiCtx::new();
    assert_eq!(wasi.insert_udp_socket(socket).unwrap(), 3);
    let mut store = Store::default().with_state(wasi);
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports()))?;
    assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, 0);
    let memory = instance.memory("memory")?;
    assert_eq!(memory.read_vec(&store, 100, 5)?, b"hello");
    assert_eq!(memory.read_vec(&store, 20, 2)?, 1_u16.to_le_bytes());
    let mut reply = [0; 5];
    peer.recv(&mut reply).unwrap();
    assert_eq!(reply, *b"world");
    assert_eq!(instance.func::<(), i32>(&store, "nonblocking_recv")?.call(&mut store, ())?, 6);
    Ok(())
}

#[test]
fn performs_descriptor_io_and_lifecycle_operations() -> tinywasm::Result<()> {
    let temp = TempDir::new("descriptor-lifecycle");
    let module = module(
        r#"(module
            (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_tell" (func $tell (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_seek" (func $seek (param i32 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_renumber" (func $renumber (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "\80\00\00\00\05\00\00\00\85\00\00\00\01\00\00\00")
            (data (i32.const 32) "data")
            (data (i32.const 128) "hello!")
            (func (export "run") (result i32)
                i32.const 3 i32.const 1 i32.const 32 i32.const 4 i32.const 9
                i64.const 100 i64.const 0 i32.const 0 i32.const 64 call $open drop
                i32.const 64 i32.load i32.const 0 i32.const 1 i32.const 68 call $write drop
                i32.const 64 i32.load i32.const 72 call $tell drop
                i32.const 64 i32.load i64.const 1 i32.const 0 i32.const 80 call $seek drop
                i32.const 64 i32.load i32.const 10 call $renumber drop
                i32.const 10 i32.const 8 i32.const 1 i32.const 88 call $write drop
                i32.const 10 call $close))"#,
    );
    let mut store = Store::default().with_state(WasiCtx::new().preopen_dir(&temp.0, "/").unwrap());
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports()))?;
    assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, 0);
    let memory = instance.memory("memory")?;
    assert_eq!(memory.read_vec(&store, 68, 4)?, 5_u32.to_le_bytes());
    assert_eq!(memory.read_vec(&store, 72, 8)?, 5_u64.to_le_bytes());
    assert_eq!(memory.read_vec(&store, 80, 8)?, 1_u64.to_le_bytes());
    assert_eq!(memory.read_vec(&store, 88, 4)?, 1_u32.to_le_bytes());
    assert_eq!(std::fs::read(temp.0.join("data")).unwrap(), b"h!llo");
    Ok(())
}

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use eyre::Result;
use tinywasm::{FuncContext, HostFunction, Imports, ModuleInstance, Store};

const IMPORT_MODULE: &str = "env";
pub const SCREEN_WIDTH: usize = 320;
pub const SCREEN_HEIGHT: usize = 200;

pub struct Runtime {
    store: Store,
    update: tinywasm::FunctionTyped<(), ()>,
    framebuffer: tinywasm::FunctionTyped<(), i32>,
    key_down: tinywasm::FunctionTyped<i32, ()>,
    key_up: tinywasm::FunctionTyped<i32, ()>,
    memory: tinywasm::Memory,
    framebuffer_bytes: Vec<u8>,
    pub host_state: Rc<RefCell<HostState>>,
}

impl Runtime {
    pub fn new(wad_path: PathBuf, guest_path: PathBuf) -> Result<Self> {
        let module = tinywasm::parse_file(&guest_path)?;
        let mut store = Store::default();
        let host_state = Rc::new(RefCell::new(HostState::new(wad_path)));
        let imports = build_imports(&mut store, host_state.clone());
        let instance = ModuleInstance::instantiate(&mut store, &module, Some(imports))?;

        let wad_path_buf = instance.func::<(), i32>(&store, "tinywasm_doom_wad_path_buf")?;
        let init = instance.func::<(), ()>(&store, "tinywasm_doom_init")?;
        let update = instance.func::<(), ()>(&store, "tinywasm_doom_update")?;
        let framebuffer = instance.func::<(), i32>(&store, "tinywasm_doom_framebuffer")?;
        let key_down = instance.func::<i32, ()>(&store, "tinywasm_doom_key_down")?;
        let key_up = instance.func::<i32, ()>(&store, "tinywasm_doom_key_up")?;
        let memory = instance.memory("memory")?;

        let buf_ptr = wad_path_buf.call(&mut store, ())? as usize;
        let wad_path_string = host_state.borrow().wad_path.to_string_lossy().into_owned();
        memory.write_cstring_bytes(&mut store, buf_ptr, &wad_path_string)?;
        init.call(&mut store, ())?;

        let width = SCREEN_WIDTH;
        let height = SCREEN_HEIGHT;

        Ok(Self {
            store,
            update,
            framebuffer,
            key_down,
            key_up,
            memory,
            framebuffer_bytes: vec![0; width * height * 4],
            host_state,
        })
    }

    pub fn tick(&mut self) -> Result<()> {
        self.update.call(&mut self.store, ())?;
        Ok(())
    }

    pub fn write_framebuffer(&mut self, dst: &mut [u32]) -> Result<()> {
        let ptr = self.framebuffer.call(&mut self.store, ())? as usize;
        self.memory.read_exact(&self.store, ptr, &mut self.framebuffer_bytes)?;
        for (index, pixel) in dst.iter_mut().enumerate() {
            let byte_index = index * 4;
            let chunk = &self.framebuffer_bytes[byte_index..byte_index + 4];
            *pixel = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
        }
        Ok(())
    }

    pub fn key_down(&mut self, key: i32) -> Result<()> {
        self.key_down.call(&mut self.store, key)?;
        Ok(())
    }

    pub fn key_up(&mut self, key: i32) -> Result<()> {
        self.key_up.call(&mut self.store, key)?;
        Ok(())
    }
}

pub struct HostState {
    pub wad_path: PathBuf,
    runtime_dir: PathBuf,
    start: Instant,
    files: BTreeMap<i32, File>,
    next_file: i32,
    pub exit_code: Option<i32>,
}

impl HostState {
    fn new(wad_path: PathBuf) -> Self {
        let runtime_dir = PathBuf::from("examples/doom/out/runtime");
        let _ = create_dir_all(&runtime_dir);
        Self { wad_path, runtime_dir, start: Instant::now(), files: BTreeMap::new(), next_file: 3, exit_code: None }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let candidate = Path::new(path);
        if candidate.is_absolute() { candidate.to_path_buf() } else { self.runtime_dir.join(candidate) }
    }

    fn should_redirect_to_wad(&self, requested: &str) -> bool {
        let Some(file_name) = Path::new(requested).file_name().and_then(|name| name.to_str()) else {
            return false;
        };

        let requested = file_name.to_ascii_lowercase();
        let provided = self.wad_path.file_name().and_then(|name| name.to_str()).map(|name| name.to_ascii_lowercase());

        match provided.as_deref() {
            Some("doom1.wad") => requested == "doom1.wad",
            Some("doom.wad") => requested == "doom.wad",
            Some("doomu.wad") => requested == "doomu.wad",
            Some("doom2.wad") => requested == "doom2.wad",
            Some("doom2f.wad") => requested == "doom2f.wad",
            Some("plutonia.wad") => requested == "plutonia.wad",
            Some("tnt.wad") => requested == "tnt.wad",
            Some(provided_name) => requested == provided_name,
            None => false,
        }
    }

    fn open_mode_options(mode: &str) -> OpenOptions {
        let mut options = OpenOptions::new();
        let plus = mode.as_bytes().contains(&b'+');

        match mode.as_bytes().first().copied() {
            Some(b'r') => {
                options.read(true);
                if plus {
                    options.write(true);
                }
            }
            Some(b'w') => {
                options.write(true).create(true).truncate(true);
                if plus {
                    options.read(true);
                }
            }
            Some(b'a') => {
                options.write(true).create(true).append(true);
                if plus {
                    options.read(true);
                }
            }
            _ => {
                options.read(true);
            }
        }

        options
    }
}

fn build_imports(store: &mut Store, state: Rc<RefCell<HostState>>) -> Imports {
    let mut imports = Imports::new();

    {
        let state = state.clone();
        imports.define(
            IMPORT_MODULE,
            "host_open",
            HostFunction::from(store, move |ctx: FuncContext<'_>, (filename_ptr, mode_ptr): (i32, i32)| {
                let memory = ctx.memory("memory")?;
                let filename = memory.read_cstring_until_null(ctx.store(), filename_ptr as usize, 1024)?;
                let mode = memory.read_cstring_until_null(ctx.store(), mode_ptr as usize, 16)?;
                let filename = filename.to_string_lossy();
                let mode = mode.to_string_lossy();
                let mut state = state.borrow_mut();
                let path = if filename == state.wad_path.to_string_lossy() || state.should_redirect_to_wad(&filename) {
                    state.wad_path.clone()
                } else {
                    state.resolve_path(&filename)
                };

                if path.is_dir() {
                    log::debug!("guest open rejected directory: path={} mode={}", path.display(), mode);
                    return Ok(-1);
                }

                let file = match HostState::open_mode_options(&mode).open(&path) {
                    Ok(file) => file,
                    Err(err) => {
                        log::debug!("guest open failed: path={} mode={} err={err}", path.display(), mode);
                        return Ok(-1);
                    }
                };

                let handle = state.next_file;
                state.next_file += 1;
                state.files.insert(handle, file);
                Ok(handle)
            }),
        );
    }

    {
        let state = state.clone();
        imports.define(
            IMPORT_MODULE,
            "host_close",
            HostFunction::from(store, move |_ctx: FuncContext<'_>, handle: i32| {
                state.borrow_mut().files.remove(&handle);
                Ok(())
            }),
        );
    }

    {
        let state = state.clone();
        imports.define(
            IMPORT_MODULE,
            "host_read",
            HostFunction::from(store, move |mut ctx: FuncContext<'_>, (handle, buf_ptr, count): (i32, i32, i32)| {
                let mut state = state.borrow_mut();
                let Some(file) = state.files.get_mut(&handle) else {
                    return Ok(0);
                };
                let mut buffer = vec![0; count.max(0) as usize];
                let read = file.read(&mut buffer).map_err(|err| tinywasm::Error::Other(err.to_string()))?;
                ctx.memory("memory")?.copy_from_slice(ctx.store_mut(), buf_ptr as usize, &buffer[..read])?;
                Ok(read as i32)
            }),
        );
    }

    {
        let state = state.clone();
        imports.define(
            IMPORT_MODULE,
            "host_write",
            HostFunction::from(store, move |ctx: FuncContext<'_>, (handle, buf_ptr, count): (i32, i32, i32)| {
                let data = ctx.memory("memory")?.read_vec(ctx.store(), buf_ptr as usize, count.max(0) as usize)?;
                let mut state = state.borrow_mut();
                let Some(file) = state.files.get_mut(&handle) else {
                    return Ok(-1);
                };
                let written = file.write(&data).map_err(|err| tinywasm::Error::Other(err.to_string()))?;
                Ok(written as i32)
            }),
        );
    }

    {
        let state = state.clone();
        imports.define(
            IMPORT_MODULE,
            "host_seek",
            HostFunction::from(store, move |_ctx: FuncContext<'_>, (handle, offset, origin): (i32, i32, i32)| {
                let seek_from = match origin {
                    0 => SeekFrom::Start(offset.max(0) as u64),
                    1 => SeekFrom::Current(offset as i64),
                    2 => SeekFrom::End(offset as i64),
                    _ => return Err(tinywasm::Error::Other(format!("invalid seek origin: {origin}"))),
                };
                let mut state = state.borrow_mut();
                let Some(file) = state.files.get_mut(&handle) else {
                    return Ok(-1);
                };
                let pos = file.seek(seek_from).map_err(|err| tinywasm::Error::Other(err.to_string()))?;
                Ok(pos.min(i32::MAX as u64) as i32)
            }),
        );
    }

    {
        let state = state.clone();
        imports.define(
            IMPORT_MODULE,
            "host_tell",
            HostFunction::from(store, move |_ctx: FuncContext<'_>, handle: i32| {
                let mut state = state.borrow_mut();
                let Some(file) = state.files.get_mut(&handle) else {
                    return Ok(-1);
                };
                let pos = file.stream_position().map_err(|err| tinywasm::Error::Other(err.to_string()))?;
                Ok(pos.min(i32::MAX as u64) as i32)
            }),
        );
    }

    {
        let state = state.clone();
        imports.define(
            IMPORT_MODULE,
            "host_eof",
            HostFunction::from(store, move |_ctx: FuncContext<'_>, handle: i32| {
                let mut state = state.borrow_mut();
                let Some(file) = state.files.get_mut(&handle) else {
                    return Ok(1);
                };
                let pos = file.stream_position().map_err(|err| tinywasm::Error::Other(err.to_string()))?;
                let len = file.metadata().map_err(|err| tinywasm::Error::Other(err.to_string()))?.len();
                Ok((pos >= len) as i32)
            }),
        );
    }

    {
        let state = state.clone();
        imports.define(
            IMPORT_MODULE,
            "host_gettime",
            HostFunction::from(store, move |mut ctx: FuncContext<'_>, (sec_ptr, usec_ptr): (i32, i32)| {
                let elapsed = state.borrow().start.elapsed();
                let sec = elapsed.as_secs().min(i32::MAX as u64) as i32;
                let usec = elapsed.subsec_micros() as i32;
                let memory = ctx.memory("memory")?;
                memory.copy_from_slice(ctx.store_mut(), sec_ptr as usize, &sec.to_le_bytes())?;
                memory.copy_from_slice(ctx.store_mut(), usec_ptr as usize, &usec.to_le_bytes())?;
                Ok(())
            }),
        );
    }

    {
        let state = state.clone();
        imports.define(
            IMPORT_MODULE,
            "host_exit",
            HostFunction::from(store, move |_ctx: FuncContext<'_>, code: i32| {
                state.borrow_mut().exit_code = Some(code);
                Ok(())
            }),
        );
    }

    imports.define(
        IMPORT_MODULE,
        "host_print",
        HostFunction::from(store, move |ctx: FuncContext<'_>, ptr: i32| {
            let text = ctx.memory("memory")?.read_cstring_until_null(ctx.store(), ptr as usize, 4096)?;
            log::info!("guest: {}", text.to_string_lossy());
            Ok(())
        }),
    );

    imports
}

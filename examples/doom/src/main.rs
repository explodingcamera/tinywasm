mod runtime;

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use eyre::{ContextCompat, Result, bail, eyre};
use runtime::{Runtime, SCREEN_HEIGHT, SCREEN_WIDTH};
use softbuffer::{Context as SoftbufferContext, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, Size};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowLevel};

const GUEST_MODULE: &str = "examples/doom/out/puredoom.wasm";
const DEFAULT_TITLE: &str = "tinywasm doom";

fn main() -> Result<()> {
    pretty_env_logger::init();

    let wad_path = std::env::args().nth(1).map(PathBuf::from).context("usage: cargo run -p tinywasm-doom -- <wad>")?;
    if !wad_path.exists() {
        bail!("WAD not found: {}", wad_path.display());
    }

    let guest_path = Path::new(GUEST_MODULE);
    if !guest_path.exists() {
        bail!("guest module missing: {}. Run ./examples/doom/build.sh first", guest_path.display());
    }

    let event_loop = EventLoop::new()?;
    let mut app = DoomApp::new(wad_path, guest_path.to_path_buf())?;
    event_loop.run_app(&mut app)?;
    Ok(())
}

struct DoomApp {
    runtime: Runtime,
    window: Option<Rc<Window>>,
    softbuffer_context: Option<SoftbufferContext<Rc<Window>>>,
    softbuffer_surface: Option<Surface<Rc<Window>, Rc<Window>>>,
}

impl DoomApp {
    fn new(wad_path: PathBuf, guest_path: PathBuf) -> Result<Self> {
        Ok(Self {
            runtime: Runtime::new(wad_path, guest_path)?,
            window: None,
            softbuffer_context: None,
            softbuffer_surface: None,
        })
    }

    fn present(&mut self) -> Result<()> {
        let Some(surface) = self.softbuffer_surface.as_mut() else {
            return Ok(());
        };

        surface
            .resize(NonZeroU32::new(SCREEN_WIDTH as u32).unwrap(), NonZeroU32::new(SCREEN_HEIGHT as u32).unwrap())
            .map_err(|err| eyre!(err.to_string()))?;
        let mut buffer = surface.buffer_mut().map_err(|err| eyre!(err.to_string()))?;
        self.runtime.write_framebuffer(&mut buffer)?;
        buffer.present().map_err(|err| eyre!(err.to_string()))?;
        Ok(())
    }
}

impl ApplicationHandler for DoomApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);

        let size = LogicalSize::new((SCREEN_WIDTH * 2) as f64, (SCREEN_HEIGHT * 2) as f64);

        let attributes = WindowAttributes::default()
            .with_title(DEFAULT_TITLE)
            .with_inner_size(Size::Logical(size))
            .with_resizable(false)
            .with_window_level(if cfg!(target_os = "linux") { WindowLevel::AlwaysOnTop } else { WindowLevel::Normal });

        let window = Rc::new(event_loop.create_window(attributes).expect("create window"));
        let context = SoftbufferContext::new(window.clone()).expect("create softbuffer context");
        let surface = Surface::new(&context, window.clone()).expect("create softbuffer surface");

        self.softbuffer_context = Some(context);
        self.softbuffer_surface = Some(surface);
        self.window = Some(window.clone());
        window.set_cursor_visible(false);
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: winit::window::WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if let Err(err) = self.present() {
                    log::error!("present failed: {err:?}");
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(key) = doom_key(&event.physical_key) {
                    let result = if matches!(event.state, ElementState::Pressed) {
                        self.runtime.key_down(i32::from(key))
                    } else {
                        self.runtime.key_up(i32::from(key))
                    };

                    if let Err(err) = result {
                        log::error!("keyboard event failed: {err:?}");
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(err) = self.runtime.tick() {
            log::error!("doom tick failed: {err:?}");
            event_loop.exit();
            return;
        }

        if let Some(window) = &self.window {
            window.request_redraw();
        }

        if self.runtime.host_state.borrow().exit_code.is_some() {
            event_loop.exit();
        }
    }
}

fn doom_key(key: &PhysicalKey) -> Option<u8> {
    let code = match key {
        PhysicalKey::Code(KeyCode::Enter) => 13,
        PhysicalKey::Code(KeyCode::Escape) => 27,
        PhysicalKey::Code(KeyCode::ArrowLeft) => 0xac,
        PhysicalKey::Code(KeyCode::ArrowRight) => 0xae,
        PhysicalKey::Code(KeyCode::ArrowUp) => 0xad,
        PhysicalKey::Code(KeyCode::ArrowDown) => 0xaf,
        PhysicalKey::Code(KeyCode::ControlLeft | KeyCode::ControlRight) => 0x80 + 0x1d,
        PhysicalKey::Code(KeyCode::ShiftLeft | KeyCode::ShiftRight) => 0xb6,
        PhysicalKey::Code(KeyCode::AltLeft | KeyCode::AltRight) => 0xb8,
        PhysicalKey::Code(KeyCode::Space) => b' ',
        PhysicalKey::Code(KeyCode::F2) => 0x80 + 0x3c,
        PhysicalKey::Code(KeyCode::F3) => 0x80 + 0x3d,
        PhysicalKey::Code(KeyCode::F4) => 0x80 + 0x3e,
        PhysicalKey::Code(KeyCode::F5) => 0x80 + 0x3f,
        PhysicalKey::Code(KeyCode::F6) => 0x80 + 0x40,
        PhysicalKey::Code(KeyCode::F7) => 0x80 + 0x41,
        PhysicalKey::Code(KeyCode::F8) => 0x80 + 0x42,
        PhysicalKey::Code(KeyCode::F9) => 0x80 + 0x43,
        PhysicalKey::Code(KeyCode::F10) => 0x80 + 0x44,
        PhysicalKey::Code(KeyCode::F11) => 0x80 + 0x57,
        PhysicalKey::Code(KeyCode::Equal) => b'=',
        PhysicalKey::Code(KeyCode::Minus) => b'-',
        PhysicalKey::Code(KeyCode::KeyA) => b'a',
        PhysicalKey::Code(KeyCode::KeyB) => b'b',
        PhysicalKey::Code(KeyCode::KeyC) => b'c',
        PhysicalKey::Code(KeyCode::KeyD) => b'd',
        PhysicalKey::Code(KeyCode::KeyE) => b'e',
        PhysicalKey::Code(KeyCode::KeyF) => b'f',
        PhysicalKey::Code(KeyCode::KeyG) => b'g',
        PhysicalKey::Code(KeyCode::KeyH) => b'h',
        PhysicalKey::Code(KeyCode::KeyI) => b'i',
        PhysicalKey::Code(KeyCode::KeyJ) => b'j',
        PhysicalKey::Code(KeyCode::KeyK) => b'k',
        PhysicalKey::Code(KeyCode::KeyL) => b'l',
        PhysicalKey::Code(KeyCode::KeyM) => b'm',
        PhysicalKey::Code(KeyCode::KeyN) => b'n',
        PhysicalKey::Code(KeyCode::KeyO) => b'o',
        PhysicalKey::Code(KeyCode::KeyP) => b'p',
        PhysicalKey::Code(KeyCode::KeyQ) => b'q',
        PhysicalKey::Code(KeyCode::KeyR) => b'r',
        PhysicalKey::Code(KeyCode::KeyS) => b's',
        PhysicalKey::Code(KeyCode::KeyT) => b't',
        PhysicalKey::Code(KeyCode::KeyU) => b'u',
        PhysicalKey::Code(KeyCode::KeyV) => b'v',
        PhysicalKey::Code(KeyCode::KeyW) => b'w',
        PhysicalKey::Code(KeyCode::KeyX) => b'x',
        PhysicalKey::Code(KeyCode::KeyY) => b'y',
        PhysicalKey::Code(KeyCode::KeyZ) => b'z',
        PhysicalKey::Code(KeyCode::Digit0) => b'0',
        PhysicalKey::Code(KeyCode::Digit1) => b'1',
        PhysicalKey::Code(KeyCode::Digit2) => b'2',
        PhysicalKey::Code(KeyCode::Digit3) => b'3',
        PhysicalKey::Code(KeyCode::Digit4) => b'4',
        PhysicalKey::Code(KeyCode::Digit5) => b'5',
        PhysicalKey::Code(KeyCode::Digit6) => b'6',
        PhysicalKey::Code(KeyCode::Digit7) => b'7',
        PhysicalKey::Code(KeyCode::Digit8) => b'8',
        PhysicalKey::Code(KeyCode::Digit9) => b'9',
        _ => return None,
    };

    Some(code)
}

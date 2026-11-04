//! Optional bounded logical guest frames. Never use the runtime from a fatal signal handler.
use crate::{
    Store,
    interpreter::stack::{CallFrame, CallStack},
    store::Code,
};

/// A logical frame with an original Wasm file offset (flags bit 0), not an IR offset.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct DebugFrame {
    /// Store-local instance ordinal; join using a consumer module manifest.
    pub instance: u32,
    /// Module-local function index.
    pub function: u32,
    /// Original Wasm file offset when mapped.
    pub offset: u32,
    /// Bit zero indicates a mapped offset (frame) or truncation (snapshot).
    pub flags: u32,
}

/// Versioned POD snapshot; youngest first, flags bit 0 means truncated.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct DebugSnapshot {
    /// Snapshot ABI version, currently one.
    pub version: u32,
    /// Byte size of this versioned POD record.
    pub size: u32,
    /// Number of initialized frames, at most 32.
    pub count: u32,
    /// Bit zero indicates a mapped offset (frame) or truncation (snapshot).
    pub flags: u32,
    /// Youngest-first logical guest frames; unused slots are zeroed.
    pub frames: [DebugFrame; 32],
}
impl Default for DebugSnapshot {
    fn default() -> Self {
        Self {
            version: 1,
            size: core::mem::size_of::<Self>() as u32,
            count: 0,
            flags: 0,
            frames: [DebugFrame::default(); 32],
        }
    }
}
impl DebugSnapshot {
    pub(crate) fn capture(&mut self, code: &Code, stack: &CallStack, current: CallFrame, ip: usize) {
        *self = Self::default();
        for (cf, ip) in
            core::iter::once((&current, ip)).chain(stack.debug_frames().map(|cf| (cf, cf.instr_ptr.saturating_sub(1))))
        {
            if self.count == 32 {
                self.flags = 1;
                break;
            }
            let func = code.funcs.wasm(cf.func_addr);
            let offset = func.func.data.source_offsets.get(ip).copied().unwrap_or(0);
            self.frames[self.count as usize] =
                DebugFrame { instance: func.owner, function: func.debug_index, offset, flags: u32::from(offset != 0) };
            self.count += 1;
        }
    }
}
impl Store {
    /// Copies the last safe-boundary snapshot; no references to guest memory or runtime internals.
    pub fn debug_snapshot(&self) -> DebugSnapshot {
        self.debug_snapshot
    }
}

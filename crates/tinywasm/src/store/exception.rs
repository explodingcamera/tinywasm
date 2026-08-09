use alloc::boxed::Box;
use tinywasm_types::TagAddr;

use crate::interpreter::TinyWasmValue;

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct ExceptionInstance {
    pub(crate) tag_addr: TagAddr,
    pub(crate) payload: Box<[TinyWasmValue]>,
}

use crate::{cold, unlikely, Error, Result};
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub(crate) struct BlockStack(Vec<BlockFrame>);

impl BlockStack {
    pub(crate) fn new() -> Self {
        Self(Vec::with_capacity(128))
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, block: BlockFrame) {
        self.0.push(block);
    }

    #[inline]
    /// get the label at the given index, where 0 is the top of the stack
    pub(crate) fn get_relative_to(&self, index: u32, offset: u32) -> Option<&BlockFrame> {
        let len = (self.0.len() as u32) - offset;

        // the vast majority of wasm functions don't use break to return
        if unlikely(index >= len) {
            return None;
        }

        Some(&self.0[self.0.len() - index as usize - 1])
    }

    #[inline(always)]
    pub(crate) fn pop(&mut self) -> Result<BlockFrame> {
        match self.0.pop() {
            Some(frame) => Ok(frame),
            None => {
                cold();
                Err(Error::BlockStackUnderflow)
            }
        }
    }

    /// keep the top `len` blocks and discard the rest
    #[inline(always)]
    pub(crate) fn truncate(&mut self, len: u32) {
        self.0.truncate(len as usize);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlockFrame {
    pub(crate) instr_ptr: usize, // position of the instruction pointer when the block was entered
    pub(crate) end_instr_offset: u32, // position of the end instruction of the block

    pub(crate) stack_ptr: u32, // position of the stack pointer when the block was entered

    #[cfg(all(nightly, feature = "simd"))]
    pub(crate) simd_stack_ptr: u32, // position of the large stack pointer when the block was entered

    pub(crate) results: u8,
    pub(crate) params: u8,
    pub(crate) ty: BlockType,
}

impl BlockFrame {}

#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub(crate) enum BlockType {
    Loop,
    If,
    Else,
    Block,
}

use crate::unlikely;
use alloc::vec::Vec;

use crate::interpreter::values::{StackHeight, StackLocation};

#[derive(Debug)]
pub(crate) struct BlockStack(Vec<BlockFrame>);

impl Default for BlockStack {
    fn default() -> Self {
        Self(Vec::with_capacity(128))
    }
}

impl BlockStack {
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
    pub(crate) fn pop(&mut self) -> BlockFrame {
        self.0.pop().expect("block stack underflow, this is a bug")
    }

    /// keep the top `len` blocks and discard the rest
    #[inline(always)]
    pub(crate) fn truncate(&mut self, len: u32) {
        self.0.truncate(len as usize);
    }
}

#[derive(Debug)]
pub(crate) struct BlockFrame {
    pub(crate) instr_ptr: usize, // position of the instruction pointer when the block was entered
    pub(crate) end_instr_offset: u32, // position of the end instruction of the block

    pub(crate) stack_ptr: StackLocation, // stack pointer when the block was entered
    pub(crate) results: StackHeight,
    pub(crate) params: StackHeight,

    pub(crate) ty: BlockType,
}

#[derive(Debug)]
pub(crate) enum BlockType {
    Loop,
    If,
    Else,
    Block,
}

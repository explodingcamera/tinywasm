use crate::engine::Config;
use alloc::vec::Vec;

use crate::interpreter::values::{StackHeight, StackLocation};
use crate::{Result, Trap};

#[derive(Debug)]
pub(crate) struct BlockStack(Vec<BlockFrame>);

impl BlockStack {
    pub(crate) fn new(config: &Config) -> Self {
        Self(Vec::with_capacity(config.block_stack_size))
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn push(&mut self, block: BlockFrame) -> Result<()> {
        if self.0.len() >= self.0.capacity() {
            return Err(Trap::BlockStackOverflow.into());
        }

        self.0.push(block);
        Ok(())
    }

    /// get the label at the given index, where 0 is the top of the stack
    pub(crate) fn get_relative_to(&self, index: u32, offset: u32) -> Option<&BlockFrame> {
        let len = (self.0.len() as u32) - offset;

        // the vast majority of wasm functions don't use break to return
        if index >= len {
            return None;
        }

        Some(&self.0[self.0.len() - index as usize - 1])
    }

    pub(crate) fn pop(&mut self) -> BlockFrame {
        match self.0.pop() {
            Some(frame) => frame,
            None => unreachable!("Block stack underflow, this is a bug"),
        }
    }

    /// keep the top `len` blocks and discard the rest
    pub(crate) fn truncate(&mut self, len: u32) {
        self.0.truncate(len as usize);
    }
}

#[derive(Debug)]
pub(crate) struct BlockFrame {
    pub(crate) instr_ptr: u32, // position of the instruction pointer when the block was entered
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

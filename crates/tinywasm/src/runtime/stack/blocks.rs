use alloc::vec::Vec;
use log::info;
use tinywasm_types::BlockArgs;

#[derive(Debug, Default, Clone)]
pub(crate) struct Blocks(Vec<BlockFrame>);

impl Blocks {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub(crate) fn push(&mut self, block: BlockFrame) {
        self.0.push(block);
    }

    #[inline]
    /// get the block at the given index, where 0 is the top of the stack
    pub(crate) fn get_relative_to_top(&self, index: usize) -> Option<&BlockFrame> {
        info!("get block: {}", index);
        info!("blocks: {:?}", self.0);
        self.0.get(self.0.len() - index - 1)
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<BlockFrame> {
        self.0.pop()
    }

    /// keep the top `len` blocks and discard the rest
    #[inline]
    pub(crate) fn trim(&mut self, len: usize) {
        self.0.truncate(len);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BlockFrame {
    // position of the instruction pointer when the block was entered
    pub(crate) instr_ptr: usize,
    // position of the end instruction of the block
    pub(crate) end_instr_ptr: usize,

    // position of the stack pointer when the block was entered
    pub(crate) stack_ptr: usize,
    pub(crate) args: BlockArgs,
    pub(crate) block: BlockFrameInner,
}

#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub(crate) enum BlockFrameInner {
    Loop,
    If,
    Else,
    Block,
}

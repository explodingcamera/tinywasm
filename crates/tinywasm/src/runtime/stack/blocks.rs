use alloc::vec::Vec;
use log::info;
use tinywasm_types::BlockArgs;

#[derive(Debug, Default, Clone)]
pub(crate) struct Blocks(Vec<BlockFrame>);

impl Blocks {
    #[inline]
    pub(crate) fn push(&mut self, block: BlockFrame) {
        self.0.push(block);
    }

    #[inline]
    /// get the block at the given index, where 0 is the top of the stack
    pub(crate) fn get(&self, index: usize) -> Option<&BlockFrame> {
        info!("get block: {}", index);
        info!("blocks: {:?}", self.0);
        self.0.get(self.0.len() - index - 1)
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<BlockFrame> {
        self.0.pop()
    }

    /// remove all blocks after the given index
    #[inline]
    pub(crate) fn trim(&mut self, index: usize) {
        self.0.truncate(index + 1);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BlockFrame {
    // where to resume execution when the block is broken
    pub(crate) instr_ptr: usize,
    // position of the stack pointer when the block was entered
    pub(crate) stack_ptr: usize,
    pub(crate) args: BlockArgs,
    pub(crate) ty: BlockFrameType,
}

#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub(crate) enum BlockFrameType {
    Loop,
    If,
    Else,
    Block,
}

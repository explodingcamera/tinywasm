use alloc::vec::Vec;
use log::info;
use tinywasm_types::BlockArgs;

use crate::{ModuleInstance, Result};

#[derive(Debug, Default, Clone)]
pub(crate) struct Labels(Vec<LabelFrame>);

impl Labels {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub(crate) fn push(&mut self, block: LabelFrame) {
        self.0.push(block);
    }

    #[inline]
    pub(crate) fn top(&self) -> Option<&LabelFrame> {
        self.0.last()
    }

    #[inline]
    /// get the block at the given index, where 0 is the top of the stack
    pub(crate) fn get_relative_to_top(&self, index: usize) -> Option<&LabelFrame> {
        info!("get block: {}", index);
        info!("blocks: {:?}", self.0);
        self.0.get(self.0.len() - index - 1)
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<LabelFrame> {
        self.0.pop()
    }

    /// keep the top `len` blocks and discard the rest
    #[inline]
    pub(crate) fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LabelFrame {
    // position of the instruction pointer when the block was entered
    pub(crate) instr_ptr: usize,
    // position of the end instruction of the block
    pub(crate) end_instr_ptr: usize,

    // position of the stack pointer when the block was entered
    pub(crate) stack_ptr: usize,
    pub(crate) args: LabelArgs,
    pub(crate) ty: BlockType,
}

#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub(crate) enum BlockType {
    Loop,
    If,
    Else,
    Block,
}

#[derive(Debug, Clone)]
pub(crate) struct LabelArgs {
    pub(crate) params: usize,
    pub(crate) results: usize,
}

pub(crate) fn get_label_args(args: BlockArgs, module: &ModuleInstance) -> Result<LabelArgs> {
    Ok(match args {
        BlockArgs::Empty => LabelArgs { params: 0, results: 0 },
        BlockArgs::Type(_) => LabelArgs { params: 0, results: 1 },
        BlockArgs::FuncType(t) => LabelArgs {
            params: module.func_ty(t).params.len(),
            results: module.func_ty(t).results.len(),
        },
    })
}

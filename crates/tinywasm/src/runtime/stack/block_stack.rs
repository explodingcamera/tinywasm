use crate::{unlikely, Error, ModuleInstance, Result};
use alloc::vec::Vec;
use tinywasm_types::BlockArgs;

#[derive(Debug, Clone, Default)]
pub(crate) struct BlockStack(Vec<BlockFrame>); // TODO: maybe Box<[LabelFrame]> by analyzing the label count when parsing the module?

impl BlockStack {
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub(crate) fn push(&mut self, block: BlockFrame) {
        self.0.push(block);
    }

    #[inline]
    /// get the label at the given index, where 0 is the top of the stack
    pub(crate) fn get_relative_to(&self, index: usize, offset: usize) -> Option<&BlockFrame> {
        let len = self.0.len() - offset;

        // the vast majority of wasm functions don't use break to return
        if unlikely(index >= len) {
            return None;
        }

        Some(&self.0[self.0.len() - index - 1])
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Result<BlockFrame> {
        self.0.pop().ok_or(Error::BlockStackUnderflow)
    }

    /// keep the top `len` blocks and discard the rest
    #[inline]
    pub(crate) fn truncate(&mut self, len: usize) {
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
    pub(crate) results: usize,
    pub(crate) params: usize,
    pub(crate) ty: BlockType,
}

impl BlockFrame {
    #[inline]
    pub(crate) fn new(
        instr_ptr: usize,
        end_instr_ptr: usize,
        stack_ptr: usize,
        ty: BlockType,
        args: &BlockArgs,
        module: &ModuleInstance,
    ) -> Self {
        let (params, results) = match args {
            BlockArgs::Empty => (0, 0),
            BlockArgs::Type(_) => (0, 1),
            BlockArgs::FuncType(t) => {
                let ty = module.func_ty(*t);
                (ty.params.len(), ty.results.len())
            }
        };

        Self { instr_ptr, end_instr_ptr, stack_ptr, results, params, ty }
    }
}

#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub(crate) enum BlockType {
    Loop,
    If,
    Else,
    Block,
}

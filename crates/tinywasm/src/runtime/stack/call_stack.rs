use crate::log;
use crate::{
    runtime::{BlockType, RawWasmValue},
    Error, FunctionInstance, Result, Trap,
};
use alloc::vec;
use alloc::{boxed::Box, rc::Rc, vec::Vec};
use tinywasm_types::{ValType, WasmValue};

use super::{blocks::Labels, LabelFrame};

// minimum call stack size
const CALL_STACK_SIZE: usize = 256;
const CALL_STACK_MAX_SIZE: usize = 1024;

#[derive(Debug)]
pub(crate) struct CallStack {
    stack: Vec<CallFrame>,
}

impl Default for CallStack {
    fn default() -> Self {
        Self { stack: Vec::with_capacity(CALL_STACK_SIZE) }
    }
}

impl CallStack {
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Result<CallFrame> {
        self.stack.pop().ok_or_else(|| Error::CallStackEmpty)
    }

    #[inline]
    pub(crate) fn push(&mut self, call_frame: CallFrame) -> Result<()> {
        log::debug!("stack size: {}", self.stack.len());
        if self.stack.len() >= CALL_STACK_MAX_SIZE {
            return Err(Trap::CallStackOverflow.into());
        }

        self.stack.push(call_frame);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CallFrame {
    pub(crate) instr_ptr: usize,
    // pub(crate) module: ModuleInstanceAddr,
    pub(crate) func_instance: Rc<FunctionInstance>,

    pub(crate) labels: Labels,
    pub(crate) locals: Box<[RawWasmValue]>,
    pub(crate) local_count: usize,
}

impl CallFrame {
    #[inline]
    /// Push a new label to the label stack and ensure the stack has the correct values
    pub(crate) fn enter_label(&mut self, label_frame: LabelFrame, stack: &mut super::ValueStack) {
        if label_frame.args.params > 0 {
            stack.extend_from_within((label_frame.stack_ptr - label_frame.args.params)..label_frame.stack_ptr);
        }

        self.labels.push(label_frame);
    }

    /// Break to a block at the given index (relative to the current frame)
    /// Returns `None` if there is no block at the given index (e.g. if we need to return, this is handled by the caller)
    pub(crate) fn break_to(&mut self, break_to_relative: u32, value_stack: &mut super::ValueStack) -> Option<()> {
        let break_to = self.labels.get_relative_to_top(break_to_relative as usize)?;

        // instr_ptr points to the label instruction, but the next step
        // will increment it by 1 since we're changing the "current" instr_ptr
        match break_to.ty {
            BlockType::Loop => {
                // this is a loop, so we want to jump back to the start of the loop
                // We also want to push the params to the stack
                value_stack.break_to(break_to.stack_ptr, break_to.args.params);

                self.instr_ptr = break_to.instr_ptr;

                // we also want to trim the label stack to the loop (but not including the loop)
                self.labels.truncate(self.labels.len() - break_to_relative as usize);
            }
            BlockType::Block | BlockType::If | BlockType::Else => {
                // this is a block, so we want to jump to the next instruction after the block ends
                // We also want to push the block's results to the stack
                value_stack.break_to(break_to.stack_ptr, break_to.args.results);

                // (the inst_ptr will be incremented by 1 before the next instruction is executed)
                self.instr_ptr = break_to.end_instr_ptr;

                // we also want to trim the label stack, including the block
                self.labels.truncate(self.labels.len() - (break_to_relative as usize + 1));
            }
        }

        Some(())
    }

    // TOOD: perf: this function is pretty hot
    // Especially the two `extend` calls
    pub(crate) fn new_raw(
        func_instance_ptr: Rc<FunctionInstance>,
        params: &[RawWasmValue],
        local_types: Vec<ValType>,
    ) -> Self {
        let mut locals = vec![RawWasmValue::default(); local_types.len() + params.len()];
        locals[..params.len()].copy_from_slice(params);

        Self {
            instr_ptr: 0,
            func_instance: func_instance_ptr,
            local_count: locals.len(),
            locals: locals.into_boxed_slice(),
            labels: Labels::default(),
        }
    }

    pub(crate) fn new(
        func_instance_ptr: Rc<FunctionInstance>,
        params: &[WasmValue],
        local_types: Vec<ValType>,
    ) -> Self {
        CallFrame::new_raw(
            func_instance_ptr,
            &params.iter().map(|v| RawWasmValue::from(*v)).collect::<Vec<_>>(),
            local_types,
        )
    }

    #[inline]
    pub(crate) fn set_local(&mut self, local_index: usize, value: RawWasmValue) {
        assert!(local_index < self.local_count, "Invalid local index");
        self.locals[local_index] = value;
    }

    #[inline]
    pub(crate) fn get_local(&self, local_index: usize) -> RawWasmValue {
        assert!(local_index < self.local_count, "Invalid local index");
        self.locals[local_index]
    }
}

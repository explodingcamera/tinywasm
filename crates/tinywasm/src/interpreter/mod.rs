pub(crate) mod executor;
pub(crate) mod num_helpers;
pub(crate) mod simd;
pub(crate) mod stack;
pub(crate) mod values;

#[cfg(not(feature = "std"))]
mod no_std_floats;

use crate::{Result, Store, interpreter::stack::CallFrame};
pub(crate) use simd::*;
pub(crate) use values::*;

#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) enum ExecState {
    Completed,
    Suspended(CallFrame),
}

/// The main `TinyWasm` runtime.
///
/// This is the default runtime used by `TinyWasm`.
#[derive(Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct InterpreterRuntime;

impl InterpreterRuntime {
    pub(crate) fn exec(store: &mut Store, mut cf: CallFrame, call_stack_base: u32) -> Result<()> {
        loop {
            let module = Self::frame_module(store, &cf);
            match executor::Executor::new(store, &module, cf, call_stack_base).run_to_completion()? {
                None => return Ok(()),
                Some(frame) => cf = frame,
            }
        }
    }

    pub(crate) fn exec_with_fuel(store: &mut Store, mut cf: CallFrame, fuel: u32) -> Result<ExecState> {
        store.execution_fuel = fuel;
        if fuel == 0 {
            return Ok(ExecState::Suspended(cf));
        }
        let mut chunk_left = executor::CHECKPOINT_INTERVAL;
        loop {
            let module = Self::frame_module(store, &cf);
            match executor::Executor::new(store, &module, cf, 0).run_with_fuel(chunk_left)? {
                executor::RunEnd::State(state) => return Ok(state),
                executor::RunEnd::Left(frame, left) => (cf, chunk_left) = (frame, left),
            }
        }
    }

    #[cfg(feature = "std")]
    pub(crate) fn exec_with_time_budget(
        store: &mut Store,
        mut cf: CallFrame,
        time_budget: core::time::Duration,
    ) -> Result<ExecState> {
        if time_budget.is_zero() {
            return Ok(ExecState::Suspended(cf));
        }
        let start = crate::std::time::Instant::now();
        let mut chunk_left = executor::CHECKPOINT_INTERVAL;
        loop {
            let module = Self::frame_module(store, &cf);
            match executor::Executor::new(store, &module, cf, 0).run_with_time_budget(start, time_budget, chunk_left)? {
                executor::RunEnd::State(state) => return Ok(state),
                executor::RunEnd::Left(frame, left) => (cf, chunk_left) = (frame, left),
            }
        }
    }

    /// The module instance that owns the function of `cf`.
    fn frame_module(store: &Store, cf: &CallFrame) -> crate::ModuleInstance {
        let owner = store.state.funcs.wasm(cf.func_addr).owner;
        store.get_module_instance(owner).expect("invalid module instance").clone()
    }
}

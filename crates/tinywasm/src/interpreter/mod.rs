pub(crate) mod executor;
pub(crate) mod num_helpers;
pub(crate) mod stack;
pub(crate) mod value128;
pub(crate) mod values;

#[cfg(not(feature = "std"))]
mod no_std_floats;

use crate::{Result, Store, interpreter::stack::CallFrame};
pub(crate) use value128::*;
pub(crate) use values::*;

#[derive(Clone, Copy)]
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
    pub(crate) fn exec(store: &mut Store, cf: CallFrame) -> Result<()> {
        executor::Executor::<false>::new(store, cf)?.run_to_completion()
    }

    pub(crate) fn exec_with_fuel(store: &mut Store, cf: CallFrame, fuel: u32) -> Result<ExecState> {
        executor::Executor::<true>::new(store, cf)?.run_with_fuel(fuel)
    }

    #[cfg(feature = "std")]
    pub(crate) fn exec_with_time_budget(
        store: &mut Store,
        cf: CallFrame,
        time_budget: core::time::Duration,
    ) -> Result<ExecState> {
        executor::Executor::<false>::new(store, cf)?.run_with_time_budget(time_budget)
    }
}

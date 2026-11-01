pub(crate) mod executor;
pub(crate) mod num;
pub(crate) mod simd;
pub(crate) mod stack;
pub(crate) mod values;

use crate::{Error, Result, Store, interpreter::stack::CallFrame};
use executor::{Budget, ExecOutcome, Executor};
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
    pub(crate) fn exec(store: &mut Store, cf: CallFrame, call_stack_base: u32) -> Result<()> {
        match Self::run(store, cf, call_stack_base, Budget::Unbudgeted)? {
            ExecState::Completed => Ok(()),
            ExecState::Suspended(_) => unreachable!("unbudgeted execution suspended"),
        }
    }

    pub(crate) fn exec_with_fuel(store: &mut Store, cf: CallFrame, fuel: u32) -> Result<ExecState> {
        store.execution_fuel = fuel;
        Self::run(store, cf, 0, Budget::Fuel)
    }

    fn run(store: &mut Store, cf: CallFrame, call_stack_base: u32, budget: Budget) -> Result<ExecState> {
        let mut outcome = Executor::new(store, cf, call_stack_base, budget).run()?;
        loop {
            let call = match &outcome {
                ExecOutcome::Complete => return Ok(ExecState::Completed),
                ExecOutcome::Suspended(frame) => return Ok(ExecState::Suspended(*frame)),
                ExecOutcome::Host(call) => call,
                ExecOutcome::Exception(reference) => {
                    // Its payload may have been popped and its frames unwound. Root it immediately,
                    // before any callback or operation that could collect the restored store.
                    return Err(Error::Exception(store.root_exception(*reference)?));
                }
            };
            let host = store.code.funcs.host(call.func);
            let type_addr = host.type_addr;
            let host_func = host.func.clone();
            host_func.call_stack(store, call.module, type_addr)?;
            outcome = {
                let mut executor = Executor::new(store, call.caller, call_stack_base, budget);
                if call.tail { executor.return_from_host() } else { executor.run() }
            }?;
        }
    }
}

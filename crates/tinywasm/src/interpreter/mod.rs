pub(crate) mod executor;
pub(crate) mod num_helpers;
pub(crate) mod simd;
pub(crate) mod stack;
mod values;

#[cfg(not(feature = "std"))]
mod no_std_floats;

use crate::coro;
use crate::{FuncContext, ModuleInstance, Result, Store};
use executor::{ExecOutcome, Executor, SuspendedHostCoroState};
use stack::{CallFrame, Stack};
use tinywasm_types::ResumeArgument;
pub use values::*;

/// The main `TinyWasm` runtime.
///
/// This is the default runtime used by `TinyWasm`.
#[derive(Debug, Default)]
pub struct InterpreterRuntime {}

#[derive(Debug)]
struct SuspendedInterpreterRuntimeBody {
    host_coro: Option<SuspendedHostCoroState>,
    executor_stack: Stack,
    executor_module: ModuleInstance,
    executor_frame: CallFrame,
}

impl SuspendedInterpreterRuntimeBody {
    fn new(
        host_coro: Option<SuspendedHostCoroState>,
        executor_stack: Stack,
        executor_module: ModuleInstance,
        executor_frame: CallFrame,
    ) -> Self {
        Self { host_coro, executor_stack, executor_module, executor_frame }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SuspendedInterpreterRuntime(Option<SuspendedInterpreterRuntimeBody>);

pub(crate) type InterpreterRuntimeExecOutcome = coro::PotentialCoroCallResult<Stack, SuspendedInterpreterRuntime>;
pub(crate) type InterpreterRuntimeResumeOutcome = coro::CoroStateResumeResult<Stack>;

impl<'_a> coro::CoroState<Stack, FuncContext<'_a>> for SuspendedInterpreterRuntime {
    fn resume(&mut self, ctx: FuncContext<'_a>, arg: ResumeArgument) -> Result<InterpreterRuntimeResumeOutcome> {
        let body = if let Some(body) = self.0.take() {
            body
        } else {
            // no suspended state to continue
            return Result::Err(crate::Error::InvalidResume);
        };

        let SuspendedInterpreterRuntimeBody { host_coro, executor_stack, executor_module, executor_frame } = body;

        let mut stack = executor_stack;
        let mut exec =
            executor::Executor { cf: executor_frame, module: executor_module, store: ctx.store, stack: &mut stack };
        let res = match exec.resume(arg, host_coro) {
            Ok(val) => val,
            Err(e) => {
                let Executor { cf, module, .. } = exec;
                // pack back in case host_coro isn't corrupted and can be continued
                self.0 = Some(SuspendedInterpreterRuntimeBody::new(e.1, stack, module, cf));
                return Err(e.0);
            }
        };

        return Ok(match res {
            ExecOutcome::Return(()) => {
                // we are finished
                InterpreterRuntimeResumeOutcome::Return(stack)
            }
            ExecOutcome::Suspended(suspend_reason, host_coro) => {
                // host_coro could be different host_coro than one we provided
                let Executor { cf, module, .. } = exec;
                self.0 = Some(SuspendedInterpreterRuntimeBody::new(host_coro, stack, module, cf));
                InterpreterRuntimeResumeOutcome::Suspended(suspend_reason)
            }
        });
    }
}

impl InterpreterRuntime {
    pub(crate) fn exec(&self, store: &mut Store, mut stack: Stack) -> Result<InterpreterRuntimeExecOutcome> {
        let mut executor = executor::Executor::new(store, &mut stack)?;
        let result = executor.run_to_suspension()?;
        Ok(match result {
            ExecOutcome::Return(()) => InterpreterRuntimeExecOutcome::Return(stack),
            ExecOutcome::Suspended(suspend_reason, host_coro) => {
                let Executor { cf, module, .. } = executor;
                InterpreterRuntimeExecOutcome::Suspended(
                    suspend_reason,
                    SuspendedInterpreterRuntime(Some(SuspendedInterpreterRuntimeBody::new(
                        host_coro, stack, module, cf,
                    ))),
                )
            }
        })
    }
}

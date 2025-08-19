pub(crate) mod executor;
pub(crate) mod num_helpers;
pub(crate) mod simd;
pub(crate) mod stack;
mod values;

#[cfg(not(feature = "std"))]
mod no_std_floats;

use crate::coro;
use crate::{FuncContext, ModuleInstance, Result, Store};
use executor::{Executor, SuspendedHostCoroState};
use stack::{CallFrame, Stack};
use tinywasm_types::ResumeArgument;
pub use values::*;

/// The main `TinyWasm` runtime.
///
/// This is the default runtime used by `TinyWasm`.
#[derive(Debug, Default)]
pub struct InterpreterRuntime {}

#[derive(Debug)]
pub(crate) struct SuspendedRuntimeBody {
    pub(crate) suspended_host_coro: Option<SuspendedHostCoroState>,
    pub(crate) module: ModuleInstance,
    pub(crate) frame: CallFrame,
}

#[derive(Debug)]
pub(crate) struct SuspendedRuntime {
    pub(crate) body: Option<(SuspendedRuntimeBody, Stack)>,
}
impl SuspendedRuntime {
    fn make_exec<'store, 'stack>(
        body: SuspendedRuntimeBody,
        stack: &'stack mut Stack,
        store: &'store mut Store,
    ) -> Executor<'store, 'stack> {
        Executor { cf: body.frame, suspended_host_coro: body.suspended_host_coro, module: body.module, store, stack }
    }
    fn unmake_exec(exec: Executor<'_, '_>) -> SuspendedRuntimeBody {
        SuspendedRuntimeBody { suspended_host_coro: exec.suspended_host_coro, module: exec.module, frame: exec.cf }
    }
}

impl<'a> coro::CoroState<stack::Stack, FuncContext<'a>> for SuspendedRuntime {
    fn resume(
        &mut self,
        ctx: FuncContext<'a>,
        arg: ResumeArgument,
    ) -> Result<coro::CoroStateResumeResult<stack::Stack>> {
        // should be put back into self.body unless we're finished
        let (body, mut stack) = if let Some(body_) = self.body.take() {
            body_
        } else {
            return Err(crate::error::Error::InvalidResume);
        };

        let mut exec = Self::make_exec(body, &mut stack, ctx.store);
        let resumed = match exec.resume(arg) {
            Ok(resumed) => resumed,
            Err(err) => {
                self.body = Some((Self::unmake_exec(exec), stack));
                return Err(err);
            }
        };
        match resumed {
            executor::ExecOutcome::Return(()) => Ok(coro::CoroStateResumeResult::Return(stack)),
            executor::ExecOutcome::Suspended(suspend) => {
                self.body = Some((Self::unmake_exec(exec), stack));
                Ok(coro::CoroStateResumeResult::Suspended(suspend))
            }
        }
    }
}

pub(crate) type RuntimeExecOutcome = coro::PotentialCoroCallResult<stack::Stack, SuspendedRuntime>;

impl InterpreterRuntime {
    pub(crate) fn exec(&self, store: &mut Store, stack: stack::Stack) -> Result<RuntimeExecOutcome> {
        let mut stack = stack;
        let mut executor = executor::Executor::new(store, &mut stack)?;
        match executor.run_to_suspension()? {
            coro::CoroStateResumeResult::Return(()) => Ok(RuntimeExecOutcome::Return(stack)),
            coro::CoroStateResumeResult::Suspended(suspend) => Ok(RuntimeExecOutcome::Suspended(
                suspend,
                SuspendedRuntime { body: Some((SuspendedRuntime::unmake_exec(executor), stack)) },
            )),
        }
    }
}

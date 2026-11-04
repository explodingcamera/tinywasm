use super::*;

/// Budget selected once for a run or resume, retained across internal stops and host callbacks.
#[derive(Clone, Copy)]
pub(crate) enum Budget {
    Unbudgeted,
    Fuel,
}

impl Budget {
    fn exhausted(self, fuel: u32) -> bool {
        matches!(self, Self::Fuel) && fuel == 0
    }
}

impl Executor<'_> {
    /// Charges a half-open executed range before its function or instruction position is lost.
    #[inline(always)]
    pub(super) fn charge_instructions(&mut self, end: usize) {
        if matches!(self.budget, Budget::Unbudgeted) {
            return;
        }
        let Some(work) = end.checked_sub(self.segment_start) else {
            crate::invariant_violated("invalid instruction accounting range");
        };
        self.segment_start = end;
        let fuel = u32::try_from(work).unwrap_or(u32::MAX);
        *self.execution_fuel = self.execution_fuel.saturating_sub(fuel);
    }

    /// Settles attempted instructions on an ordinary trap, using the dispatcher's existing IP.
    #[cold]
    pub(super) fn settle_error(&mut self, instr_ptr: usize, error: ExecError) -> ExecError {
        #[cfg(feature = "diagnostics")]
        self.debug_snapshot.capture(self.code, self.call_stack, self.cf, instr_ptr);
        self.charge_instructions(instr_ptr + 1);
        error
    }

    #[inline(always)]
    fn after_boundary(&mut self, next: usize) -> Option<usize> {
        if matches!(self.budget, Budget::Unbudgeted) {
            return Some(next);
        }
        self.segment_start = next;
        if self.budget.exhausted(*self.execution_fuel) {
            self.suspend(next);
            None
        } else {
            Some(next)
        }
    }

    #[inline(always)]
    fn suspend(&mut self, next: usize) {
        self.cf.instr_ptr = next;
        self.outcome = ExecOutcome::Suspended(self.cf);
    }

    /// Applies the same continuation and suspension rules in both dispatchers.
    #[inline(always)]
    pub(super) fn transition(&mut self, flow: ExecFlow, instr_ptr: usize) -> Option<usize> {
        match flow {
            ExecFlow::Next(next) => {
                debug_assert_eq!(next, instr_ptr + 1);
                Some(next)
            }
            ExecFlow::Jump(next) => self.after_boundary(next),
            ExecFlow::Checkpoint(next) => {
                self.charge_instructions(instr_ptr);
                self.after_boundary(next)
            }
            ExecFlow::Complete => {
                self.outcome = ExecOutcome::Complete;
                None
            }
            ExecFlow::Stop => None,
        }
    }

    /// Runs until completion or a boundary that needs the driver.
    pub(crate) fn run(&mut self) -> Result<ExecOutcome> {
        if self.budget.exhausted(*self.execution_fuel) {
            self.suspend(self.cf.instr_ptr);
            return Ok(self.outcome);
        }
        self.dispatch().map_err(|error| Error::Trap(*error.0))?;
        Ok(self.outcome)
    }

    /// Commits a tail host call's return before checking for suspension or dispatching its caller.
    pub(crate) fn return_from_host(&mut self) -> Result<ExecOutcome> {
        let flow = self.exec_return();
        if self.transition(flow, self.cf.instr_ptr).is_none() {
            return Ok(self.outcome);
        }
        self.run()
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::engine::Config;
    use crate::interpreter::{ExecState, InterpreterRuntime};
    use alloc::format;

    fn prepare(source: &str, policy: FuelPolicy) -> (Store, CallFrame) {
        let parser = tinywasm_parser::Parser::new(tinywasm_parser::ParserOptions::new().with_optimize(false));
        let module = parser.parse_module_bytes(wat::parse_str(source).unwrap()).unwrap();
        let mut store = Store::new(Engine::new(Config::new().with_fuel_policy(policy)));
        let instance = ModuleInstance::instantiate(&mut store, &module, None).unwrap();
        let addr = instance.func_untyped(&store, "run").unwrap().addr();
        let wasm = store.code.funcs.wasm(addr);
        let (params, locals, max_stack) = (wasm.func.params, wasm.func.locals, wasm.func.max_stack);
        let base = store.value_stack.enter_locals(&params, &locals, &max_stack).unwrap();
        (store, CallFrame::new(addr, base, locals))
    }

    #[test]
    fn suspension_restores_stack() {
        let (mut store, frame) =
            prepare(r#"(module (func (export "run") (local i32 i64 v128)))"#, FuelPolicy::PerInstruction);
        store.value_stack.stack_32.set(0, 37);
        store.value_stack.stack_64.set(0, 49);
        store.value_stack.stack_128.set(0, Value128([5; 16]));
        let ExecOutcome::Suspended(frame) = Executor::new(&mut store, frame, 0, Budget::Fuel).run().unwrap() else {
            panic!("expected suspension before the first instruction");
        };
        assert_eq!(frame.instr_ptr, 0);
        assert_eq!(*store.value_stack.stack_32.get(0), 37);
        assert_eq!(*store.value_stack.stack_64.get(0), 49);
        assert_eq!(*store.value_stack.stack_128.get(0), Value128([5; 16]));

        let outcome = Executor::new(&mut store, frame, 0, Budget::Unbudgeted).run().unwrap();
        assert!(matches!(outcome, ExecOutcome::Complete));
        let base = store.value_stack.base();
        assert_eq!([base.s32, base.s64, base.s128], [0; 3]);
    }

    #[cfg(feature = "std")]
    #[test]
    fn handler_unwind_restores_stack() {
        use crate::std::panic::{AssertUnwindSafe, catch_unwind};

        struct PanicOnGrowth;
        impl crate::ResourceLimiter for PanicOnGrowth {
            fn memory_growing(&self, _: usize, _: usize, _: Option<usize>) -> Result<bool, Trap> {
                panic!("unwind through an instruction handler");
            }
        }

        for budget in [Budget::Unbudgeted, Budget::Fuel] {
            let (mut store, frame) = prepare(
                r#"(module (memory 1)
                    (func (export "run") (local i32 i64 v128) i32.const 1 memory.grow drop))"#,
                FuelPolicy::PerInstruction,
            );
            store.engine = Engine::new(Config::new().with_resource_limiter(PanicOnGrowth));
            store.execution_fuel = 100;
            store.value_stack.stack_32.set(0, 37);
            store.value_stack.stack_64.set(0, 49);
            store.value_stack.stack_128.set(0, Value128([5; 16]));
            let result = catch_unwind(AssertUnwindSafe(|| Executor::new(&mut store, frame, 0, budget).run()));
            assert!(result.is_err());
            assert_eq!(*store.value_stack.stack_32.get(0), 37);
            assert_eq!(*store.value_stack.stack_64.get(0), 49);
            assert_eq!(*store.value_stack.stack_128.get(0), Value128([5; 16]));
        }
    }

    #[test]
    fn synthetic_checkpoints_cost_no_fuel() {
        let body = "local.get 0 i32.const 1 i32.add local.set 0\n".repeat(2048);
        let wasm =
            wat::parse_str(format!("(module (func (export \"run\") (result i32) (local i32) {body} local.get 0))"))
                .unwrap();
        for optimize in [false, true] {
            for interval in [1, 8, 128, 1024] {
                let options =
                    tinywasm_parser::ParserOptions::new().with_optimize(optimize).with_checkpoint_interval(interval);
                let module = tinywasm_parser::Parser::new(options).parse_module_bytes(&wasm).unwrap();
                let mut store = Store::default();
                let instance = ModuleInstance::instantiate(&mut store, &module, None).unwrap();
                let function = instance.func::<(), i32>(&store, "run").unwrap();
                let instructions = &store.code.funcs.wasm(function.func.addr()).func.instructions;
                let guest_work =
                    instructions.iter().filter(|instruction| !matches!(instruction, Instruction::Checkpoint)).count();
                let first_checkpoint = instructions
                    .iter()
                    .position(|instruction| matches!(instruction, Instruction::Checkpoint))
                    .expect("fixture needs synthetic checkpoints");
                assert!(
                    instructions
                        .split(|instruction| matches!(instruction, Instruction::Checkpoint))
                        .all(|segment| segment.len() <= interval + 3),
                    "checkpoint spacing exceeded for interval {interval}, optimize={optimize}"
                );
                let mut execution = function.call_resumable(&mut store, ()).unwrap();
                assert!(matches!(execution.resume_with_fuel(1).unwrap(), ExecProgress::Suspended));
                let remaining_work = guest_work - first_checkpoint;
                assert!(matches!(
                    execution.resume_with_fuel(remaining_work as u32 + 10).unwrap(),
                    ExecProgress::Completed(2048)
                ));
                drop(execution);
                assert_eq!(store.execution_fuel, 10);
            }
        }
    }

    #[test]
    fn untaken_jumps_keep_the_executed_range() {
        let source = r#"(module (func (export "run")
            block i32.const 0 br_if 0 i32.const 7 drop end))"#;
        let (mut store, frame) = prepare(source, FuelPolicy::PerInstruction);
        let count = store.code.funcs.wasm(frame.func_addr).func.instructions.len() as u32;
        let result = InterpreterRuntime::exec_with_fuel(&mut store, frame, count + 10).unwrap();
        assert!(matches!(result, ExecState::Completed));
        assert_eq!(store.execution_fuel, 10);
    }

    #[test]
    fn calls_are_charged_once() {
        let source = r#"(module (func $callee) (func (export "run") call $callee))"#;
        for (policy, cost) in [(FuelPolicy::PerInstruction, 3), (FuelPolicy::Weighted, 7)] {
            let (mut store, frame) = prepare(source, policy);
            let result = InterpreterRuntime::exec_with_fuel(&mut store, frame, 100).unwrap();
            assert!(matches!(result, ExecState::Completed));
            assert_eq!(store.execution_fuel, 100 - cost);
        }
    }

    #[test]
    fn failed_calls_are_not_charged_twice() {
        let source = r#"(module (type $void (func)) (table 1 funcref)
            (func (export "run") i32.const 0 call_indirect (type $void)))"#;
        for (policy, cost) in [(FuelPolicy::PerInstruction, 2), (FuelPolicy::Weighted, 6)] {
            let (mut store, frame) = prepare(source, policy);
            let result = InterpreterRuntime::exec_with_fuel(&mut store, frame, 100);
            assert!(matches!(result, Err(Error::Trap(Trap::UninitializedElement { .. }))));
            assert_eq!(store.execution_fuel, 100 - cost);
        }
    }

    #[test]
    fn traps_win_over_suspension() {
        let (mut store, frame) =
            prepare(r#"(module (func (export "run") i32.const 1 drop unreachable))"#, FuelPolicy::PerInstruction);
        let result = InterpreterRuntime::exec_with_fuel(&mut store, frame, 3);
        assert!(matches!(result, Err(Error::Trap(Trap::Unreachable))));
        assert_eq!(store.execution_fuel, 0);
    }

    #[test]
    fn completion_wins_over_suspension() {
        let (mut store, frame) = prepare(r#"(module (func (export "run")))"#, FuelPolicy::PerInstruction);
        let result = InterpreterRuntime::exec_with_fuel(&mut store, frame, 1).unwrap();
        assert!(matches!(result, ExecState::Completed));
        assert_eq!(store.execution_fuel, 0);
    }

    #[test]
    fn call_and_return_suspend_at_committed_continuations() {
        let (mut store, frame) =
            prepare(r#"(module (func $callee) (func (export "run") call $callee))"#, FuelPolicy::PerInstruction);
        let ExecState::Suspended(callee) = InterpreterRuntime::exec_with_fuel(&mut store, frame, 1).unwrap() else {
            panic!("expected suspension at callee entry");
        };
        assert_ne!(callee.func_addr, frame.func_addr);
        assert_eq!(callee.instr_ptr, 0);
        assert_eq!(store.call_stack.len(), 1);
        let ExecState::Suspended(caller) = InterpreterRuntime::exec_with_fuel(&mut store, callee, 1).unwrap() else {
            panic!("expected suspension at the caller continuation");
        };
        assert_eq!(caller.func_addr, frame.func_addr);
        assert_eq!(caller.instr_ptr, 1);
        assert_eq!(store.call_stack.len(), 0);
        let result = InterpreterRuntime::exec_with_fuel(&mut store, caller, 1).unwrap();
        assert!(matches!(result, ExecState::Completed));
        assert_eq!(store.execution_fuel, 0);
    }
}

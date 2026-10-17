use super::*;

struct Unbudgeted;
struct Bounded;

// The last argument of both handler types is the height of the 32-bit value stack, which most
// instructions push to or pop from. Each handler writes it back to the stack before its body runs
// and reads it again before dispatching, so no handler loads the height its predecessor stored.
// Most keep it in a register in between. A few load it back before dispatching: the 8- and 16-lane
// SIMD ops, and the fused `LoadLocal*` handlers, whose hot path joins a cold call.
//
// Between two handlers only what the calling convention passes in registers stays out of memory.
// On arm64_32 (watchOS) the Rust ABI passes an aggregate larger than a pointer, the 8-byte
// `Instruction`, by reference, so every dispatch would store it and the next handler load it back.
// The C convention passes it in a register; `C-unwind` still lets a host function's panic unwind.
// On Windows x64 the unbudgeted handlers use the System V convention (`sysv64-unwind`), which
// passes six integer arguments in registers where Windows' own passes four, so a dispatch no longer
// stores the height for the next handler to load back. The `Instruction` still goes through the
// stack there: the System V rules pass this enum in memory. The bounded handlers keep the Rust ABI
// on Windows x64 and do not pass the height, so their four arguments, the `Instruction` among them,
// stay in Windows' four argument registers. Elsewhere the handlers keep the Rust ABI, which
// passes eight integer arguments in registers on arm64 and six on x86-64 System V, with the
// `Instruction` in one of them.
macro_rules! handler_fn {
    ($(#[$meta:meta])* fn $($rest:tt)*) => {
        #[cfg(all(target_arch = "aarch64", target_pointer_width = "32"))]
        #[allow(improper_ctypes_definitions)]
        $(#[$meta])* extern "C-unwind" fn $($rest)*
        #[cfg(all(target_arch = "x86_64", windows))]
        #[allow(improper_ctypes_definitions)]
        $(#[$meta])* extern "sysv64-unwind" fn $($rest)*
        #[cfg(not(any(all(target_arch = "aarch64", target_pointer_width = "32"), all(target_arch = "x86_64", windows))))]
        $(#[$meta])* fn $($rest)*
    };
}

macro_rules! bounded_handler_fn {
    ($(#[$meta:meta])* fn $($rest:tt)*) => {
        #[cfg(all(target_arch = "aarch64", target_pointer_width = "32"))]
        #[allow(improper_ctypes_definitions)]
        $(#[$meta])* extern "C-unwind" fn $($rest)*
        #[cfg(not(all(target_arch = "aarch64", target_pointer_width = "32")))]
        $(#[$meta])* fn $($rest)*
    };
}

/// The height a bounded handler passes on: nothing on Windows x64.
#[cfg(not(all(target_arch = "x86_64", windows)))]
type BoundedHeight = usize;
#[cfg(all(target_arch = "x86_64", windows))]
type BoundedHeight = ();

#[inline(always)]
fn bounded_height(executor: &Executor<'_, '_>) -> BoundedHeight {
    #[cfg(not(all(target_arch = "x86_64", windows)))]
    return executor.value_stack.stack_32.len();
    #[cfg(all(target_arch = "x86_64", windows))]
    {
        let _ = executor;
    }
}

/// Writes a passed height back to the stack.
#[inline(always)]
fn enter_bounded_height(executor: &mut Executor<'_, '_>, height32: BoundedHeight) {
    #[cfg(not(all(target_arch = "x86_64", windows)))]
    executor.value_stack.stack_32.set_len(height32);
    #[cfg(all(target_arch = "x86_64", windows))]
    {
        let _ = (executor, height32);
    }
}

macro_rules! handler_types {
    ($($abi:literal)?) => {
        // Both sides are Rust, so the C convention's view of these types need not be FFI-safe.
        #[allow(improper_ctypes_definitions)]
        type UnbudgetedHandler =
            for<'store, 'module> $(extern $abi)? fn(&mut Executor<'store, 'module>, &[Instruction], usize, Instruction, usize) -> ExecResult<()>;
    };
}
#[cfg(all(target_arch = "aarch64", target_pointer_width = "32"))]
#[allow(improper_ctypes_definitions)]
type BoundedHandler = for<'store, 'module> extern "C-unwind" fn(&mut Executor<'store, 'module>, usize, Instruction, u32, BoundedHeight) -> ExecResult<()>;
#[cfg(not(all(target_arch = "aarch64", target_pointer_width = "32")))]
type BoundedHandler = for<'store, 'module> fn(&mut Executor<'store, 'module>, usize, Instruction, u32, BoundedHeight) -> ExecResult<()>;
#[cfg(all(target_arch = "aarch64", target_pointer_width = "32"))]
handler_types!("C-unwind");
#[cfg(all(target_arch = "x86_64", windows))]
handler_types!("sysv64-unwind");
#[cfg(not(any(all(target_arch = "aarch64", target_pointer_width = "32"), all(target_arch = "x86_64", windows))))]
handler_types!();

macro_rules! define_unbudgeted_tail_dispatch {
    ($executor:ident, $instr_ptr:ident, $dispatch_next:ident, $dispatch_flow:ident;
     $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
        #[inline(always)]
        fn handler_for(opcode: InstructionOpcode) -> UnbudgetedHandler {
            static HANDLERS: [UnbudgetedHandler; InstructionOpcode::COUNT] = {
                let mut handlers = [Unbudgeted::Unreachable as UnbudgetedHandler; InstructionOpcode::COUNT];
                $(handlers[InstructionOpcode::$variant as usize] = Unbudgeted::$variant;)*
                handlers
            };
            HANDLERS[opcode as usize]
        }

        $(handler_fn! {
            #[allow(non_snake_case, unreachable_code, unused_imports, unused_macros, unused_variables)]
            fn $variant(
                $executor: &mut Executor<'_, '_>,
                instructions: &[Instruction],
                $instr_ptr: usize,
                instruction: Instruction,
                height32: usize,
            ) -> ExecResult<()> {
                macro_rules! $dispatch_next {
                    ($next_instr_ptr:expr) => {{
                        let next_instr_ptr = $next_instr_ptr;
                        let height32 = $executor.value_stack.stack_32.len();
                        let Some(&next) = instructions.get(next_instr_ptr) else {
                            become Self::invalid_instr_ptr($executor, instructions, next_instr_ptr, instruction, height32);
                        };
                        let handler = Self::handler_for(next.opcode());
                        become handler($executor, instructions, next_instr_ptr, next, height32);
                    }};
                }
                macro_rules! $dispatch_flow {
                    ($flow:expr) => {{
                        match $flow {
                            ExecFlow::Next(next_instr_ptr) => $dispatch_next!(next_instr_ptr),
                            ExecFlow::Switch(next_instr_ptr) => {
                                $executor.cf.instr_ptr = next_instr_ptr;
                                return Ok(());
                            },
                            ExecFlow::Complete => return cold!({
                                if !$executor.left {
                                    $executor.completed = true;
                                }
                                Ok(())
                            }),
                        }
                    }};
                }
                use tinywasm_types::Instruction::*;
                $(let $variant($($arg),*) = &instruction else {
                    become Self::handler_mismatch($executor, instructions, $instr_ptr, instruction, height32);
                };)?
                $(let $variant { $($field),* } = &instruction else {
                    become Self::handler_mismatch($executor, instructions, $instr_ptr, instruction, height32);
                };)?
                $executor.value_stack.stack_32.set_len(height32);
                $body;
                $dispatch_next!($instr_ptr + 1)
            }
        })*
    };
}

macro_rules! define_bounded_tail_dispatch {
    ($executor:ident, $instr_ptr:ident, $dispatch_next:ident, $dispatch_flow:ident;
     $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
        #[inline(always)]
        fn handler_for(opcode: InstructionOpcode) -> BoundedHandler {
            static HANDLERS: [BoundedHandler; InstructionOpcode::COUNT] = {
                let mut handlers = [Bounded::Unreachable as BoundedHandler; InstructionOpcode::COUNT];
                $(handlers[InstructionOpcode::$variant as usize] = Bounded::$variant;)*
                handlers
            };
            HANDLERS[opcode as usize]
        }

        $(bounded_handler_fn! {
            #[allow(non_snake_case, unreachable_code, unused_imports, unused_macros, unused_variables)]
            fn $variant(
                $executor: &mut Executor<'_, '_>,
                $instr_ptr: usize,
                instruction: Instruction,
                instructions_until_checkpoint: u32,
                height32: BoundedHeight,
            ) -> ExecResult<()> {
                macro_rules! $dispatch_next {
                    ($next_instr_ptr:expr) => {{
                        let next_instr_ptr = $next_instr_ptr;
                        let height32 = bounded_height($executor);
                        if instructions_until_checkpoint == 0 {
                            return cold!({
                                $executor.cf.instr_ptr = next_instr_ptr;
                                Ok(())
                            });
                        }

                        let Some(&next) = $executor.func.instructions.get(next_instr_ptr) else {
                            become Self::invalid_instr_ptr($executor, next_instr_ptr, instruction, instructions_until_checkpoint, height32);
                        };
                        let handler = Self::handler_for(next.opcode());
                        become handler($executor, next_instr_ptr, next, instructions_until_checkpoint - 1, height32);
                    }};
                }
                macro_rules! $dispatch_flow {
                    ($flow:expr) => {{
                        match $flow.next_instr_ptr() {
                            Some(next_instr_ptr) => $dispatch_next!(next_instr_ptr),
                            None => return cold!({
                                if $executor.left {
                                    $executor.chunk_left = instructions_until_checkpoint;
                                } else {
                                    $executor.completed = true;
                                }
                                Ok(())
                            }),
                        }
                    }};
                }
                use tinywasm_types::Instruction::*;
                $(let $variant($($arg),*) = &instruction else {
                    become Self::handler_mismatch($executor, $instr_ptr, instruction, instructions_until_checkpoint, height32);
                };)?
                $(let $variant { $($field),* } = &instruction else {
                    become Self::handler_mismatch($executor, $instr_ptr, instruction, instructions_until_checkpoint, height32);
                };)?
                enter_bounded_height($executor, height32);
                $body;
                $dispatch_next!($instr_ptr + 1)
            }
        })*
    };
}

impl Unbudgeted {
    instruction_handlers!(define_unbudgeted_tail_dispatch);

    // The handlers tail-call these cold paths instead of calling them: a call would make every
    // handler save a stack frame.

    handler_fn! {
        #[cold]
        #[inline(never)]
        fn handler_mismatch(_: &mut Executor<'_, '_>, _: &[Instruction], _: usize, _: Instruction, _: usize) -> ExecResult<()> {
            unreachable!("instruction handler mismatch")
        }
    }

    handler_fn! {
        #[cold]
        #[inline(never)]
        fn invalid_instr_ptr(_: &mut Executor<'_, '_>, _: &[Instruction], instr_ptr: usize, _: Instruction, _: usize) -> ExecResult<()> {
            unreachable!("instruction pointer {instr_ptr} out of range, this is a bug")
        }
    }
}

impl Bounded {
    instruction_handlers!(define_bounded_tail_dispatch);

    // Tail-called like `Unbudgeted`'s.

    bounded_handler_fn! {
        #[cold]
        #[inline(never)]
        fn handler_mismatch(_: &mut Executor<'_, '_>, _: usize, _: Instruction, _: u32, _: BoundedHeight) -> ExecResult<()> {
            unreachable!("instruction handler mismatch")
        }
    }

    bounded_handler_fn! {
        #[cold]
        #[inline(never)]
        fn invalid_instr_ptr(_: &mut Executor<'_, '_>, instr_ptr: usize, _: Instruction, _: u32, _: BoundedHeight) -> ExecResult<()> {
            unreachable!("instruction pointer {instr_ptr} out of range, this is a bug")
        }
    }

    /// Runs up to `chunk_left` (at least 1) instructions from `executor.cf`.
    #[inline(always)]
    fn run(executor: &mut Executor<'_, '_>, chunk_left: u32) -> ExecResult<()> {
        let instr_ptr = executor.cf.instr_ptr;
        let instruction = executor.func.instructions[instr_ptr];
        let handler = Self::handler_for(instruction.opcode());
        let height32 = bounded_height(executor);
        handler(executor, instr_ptr, instruction, chunk_left - 1, height32)
    }
}

impl Executor<'_, '_> {
    /// Runs until the call completes (`None`) or continues in another module instance's frame.
    #[inline(always)]
    pub(crate) fn run_to_completion(mut self) -> Result<Option<CallFrame>> {
        loop {
            let func = self.func;
            let instructions = &func.instructions;
            let instr_ptr = self.cf.instr_ptr;
            let instruction = instructions[instr_ptr];
            let handler = Unbudgeted::handler_for(instruction.opcode());
            let height32 = self.value_stack.stack_32.len();
            handler(&mut self, instructions, instr_ptr, instruction, height32)?;
            if self.completed || self.left {
                return Ok(self.left());
            }
        }
    }

    /// Runs `chunk_left` instructions, then checkpoint by checkpoint until `time_budget` has
    /// elapsed since `start`.
    #[cfg(feature = "std")]
    #[inline(always)]
    pub(crate) fn run_with_time_budget(
        mut self,
        start: crate::std::time::Instant,
        time_budget: core::time::Duration,
        mut chunk_left: u32,
    ) -> Result<RunEnd> {
        loop {
            if chunk_left != 0 {
                Bounded::run(&mut self, chunk_left)?;
                if let Some(end) = self.run_end() {
                    return Ok(end);
                }
            }
            chunk_left = CHECKPOINT_INTERVAL;
            if start.elapsed() >= time_budget {
                return cold!(Ok(RunEnd::State(ExecState::Suspended(self.cf))));
            }
        }
    }

    /// Runs `chunk_left` instructions, then checkpoint by checkpoint until the store's fuel is out.
    #[inline(always)]
    pub(crate) fn run_with_fuel(mut self, mut chunk_left: u32) -> Result<RunEnd> {
        self.fuel_metered = true;
        loop {
            if chunk_left != 0 {
                Bounded::run(&mut self, chunk_left)?;
                if let Some(end) = self.run_end() {
                    return Ok(end);
                }
            }
            chunk_left = CHECKPOINT_INTERVAL;
            self.store.execution_fuel = self.store.execution_fuel.saturating_sub(CHECKPOINT_INTERVAL);
            if self.store.execution_fuel == 0 {
                return cold!(Ok(RunEnd::State(ExecState::Suspended(self.cf))));
            }
        }
    }

    /// How a bounded chain that stopped ended, unless it stopped at a checkpoint.
    #[inline(always)]
    fn run_end(&self) -> Option<RunEnd> {
        if self.completed {
            return cold!(Some(RunEnd::State(ExecState::Completed)));
        }
        self.left().map(|frame| RunEnd::Left(frame, self.chunk_left))
    }
}

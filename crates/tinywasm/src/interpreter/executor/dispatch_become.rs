use super::*;

struct Unbudgeted;
struct Bounded;

type UnbudgetedHandler = for<'store> fn(&mut Executor<'store>, usize, Instruction) -> ExecResult<()>;
type BoundedHandler =
  for<'store> fn(&mut Executor<'store>, usize, Instruction, u32) -> ExecResult<()>;

const INSTRUCTION_VARIANTS: usize = core::mem::variant_count::<Instruction>();

/// Variant index of an 8-byte `Instruction`. Used to index a single static
/// handler table instead of a `match` (which LLVM clones per inlined site).
#[inline(always)]
const fn opcode(instruction: Instruction) -> usize {
  core::intrinsics::discriminant_value(&instruction) as usize
}

/// Builds a dummy `Instruction` so we can read its discriminant in `const`.
/// Payloads are zeroed; only the tag is used.
macro_rules! dummy_instruction {
  ($variant:ident) => {
    Instruction::$variant
  };
  ($variant:ident ($($arg:pat),+)) => {
    Instruction::$variant($({
      let _ = stringify!($arg);
      #[allow(unsafe_code)]
      unsafe { ::core::mem::zeroed() }
    }),+)
  };
  ($variant:ident { $($field:ident),+ }) => {
    Instruction::$variant {
      $($field: {
        #[allow(unsafe_code)]
        unsafe { ::core::mem::zeroed() }
      },)+
    }
  };
}

#[cold]
#[inline(never)]
fn instruction_handler_mismatch() -> ! {
  unreachable!("instruction handler mismatch")
}

macro_rules! define_unbudgeted_tail_dispatch {
  ($executor:ident, $instr_ptr:ident, $dispatch_next:ident, $dispatch_flow:ident;
   $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
    #[allow(unsafe_code)]
    const HANDLERS: [UnbudgetedHandler; INSTRUCTION_VARIANTS] = {
      let mut table = [Self::Unreachable as UnbudgetedHandler; INSTRUCTION_VARIANTS];
      $(
        table[opcode(dummy_instruction!($variant $(($($arg),*))? $({ $($field),* })?))] =
          Self::$variant;
      )*
      table
    };

    $(
      #[allow(non_snake_case, unreachable_code, unused_imports, unused_macros, unused_variables)]
      #[inline(never)]
      fn $variant(
        $executor: &mut Executor<'_>,
        $instr_ptr: usize,
        instruction: Instruction,
      ) -> ExecResult<()> {
        macro_rules! $dispatch_next {
          ($next_instr_ptr:expr) => {{
            let next_instr_ptr = $next_instr_ptr;
            let instruction = $executor.func.instructions[next_instr_ptr];
            become Self::HANDLERS[opcode(instruction)]($executor, next_instr_ptr, instruction);
          }};
        }
        macro_rules! $dispatch_flow {
          ($flow:expr) => {{
            match $flow.next_instr_ptr() {
              Some(next_instr_ptr) => $dispatch_next!(next_instr_ptr),
              None => return cold!(Ok(())),
            }
          }};
        }
        use tinywasm_types::Instruction::*;
        $(let $variant($($arg),*) = &instruction else {
          cold!(instruction_handler_mismatch())
        };)?
        $(let $variant { $($field),* } = &instruction else {
          cold!(instruction_handler_mismatch())
        };)?
        $body;
        $dispatch_next!($instr_ptr + 1)
      }
    )*

    #[allow(dead_code)]
    fn assert_handlers_exhaustive(instruction: Instruction) {
      match instruction {
        $(Instruction::$variant { .. } => {},)*
      }
    }
  };
}

macro_rules! define_bounded_tail_dispatch {
  ($executor:ident, $instr_ptr:ident, $dispatch_next:ident, $dispatch_flow:ident;
   $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
    #[allow(unsafe_code)]
    const HANDLERS: [BoundedHandler; INSTRUCTION_VARIANTS] = {
      let mut table = [Self::Unreachable as BoundedHandler; INSTRUCTION_VARIANTS];
      $(
        table[opcode(dummy_instruction!($variant $(($($arg),*))? $({ $($field),* })?))] =
          Self::$variant;
      )*
      table
    };

    $(
      #[allow(non_snake_case, unreachable_code, unused_imports, unused_macros, unused_variables)]
      #[inline(never)]
      fn $variant(
        $executor: &mut Executor<'_>,
        $instr_ptr: usize,
        instruction: Instruction,
        instructions_until_checkpoint: u32,
      ) -> ExecResult<()> {
        macro_rules! $dispatch_next {
          ($next_instr_ptr:expr) => {{
            let next_instr_ptr = $next_instr_ptr;
            if instructions_until_checkpoint == 0 {
              return cold!({
                $executor.cf.instr_ptr = next_instr_ptr;
                Ok(())
              });
            }

            let instruction = $executor.func.instructions[next_instr_ptr];
            become Self::HANDLERS[opcode(instruction)](
              $executor,
              next_instr_ptr,
              instruction,
              instructions_until_checkpoint - 1,
            );
          }};
        }
        macro_rules! $dispatch_flow {
          ($flow:expr) => {{
            match $flow.next_instr_ptr() {
              Some(next_instr_ptr) => $dispatch_next!(next_instr_ptr),
              None => return cold!({
                $executor.completed = true;
                Ok(())
              }),
            }
          }};
        }
        use tinywasm_types::Instruction::*;
        $(let $variant($($arg),*) = &instruction else {
          cold!(instruction_handler_mismatch())
        };)?
        $(let $variant { $($field),* } = &instruction else {
          cold!(instruction_handler_mismatch())
        };)?
        $body;
        $dispatch_next!($instr_ptr + 1)
      }
    )*
  };
}

#[allow(unsafe_code)]
impl Unbudgeted {
  instruction_handlers!(define_unbudgeted_tail_dispatch);
}

#[allow(unsafe_code)]
impl Bounded {
  instruction_handlers!(define_bounded_tail_dispatch);

  #[inline(always)]
  fn run(executor: &mut Executor<'_>) -> ExecResult<()> {
    let instr_ptr = executor.cf.instr_ptr;
    let instruction = executor.func.instructions[instr_ptr];
    Self::HANDLERS[opcode(instruction)](executor, instr_ptr, instruction, CHECKPOINT_INTERVAL - 1)
  }
}

impl<'store> Executor<'store> {
  #[inline(always)]
  pub(crate) fn run_to_completion(mut self) -> Result<()> {
    let instr_ptr = self.cf.instr_ptr;
    let instruction = self.func.instructions[instr_ptr];
    Ok(Unbudgeted::HANDLERS[opcode(instruction)](&mut self, instr_ptr, instruction)?)
  }

  #[cfg(feature = "std")]
  #[inline(always)]
  pub(crate) fn run_with_time_budget(mut self, time_budget: core::time::Duration) -> Result<ExecState> {
    use crate::std::time::Instant;

    if time_budget.is_zero() {
      return Ok(ExecState::Suspended(self.cf));
    }
    let start = Instant::now();

    loop {
      Bounded::run(&mut self)?;
      if self.completed {
        return cold!(Ok(ExecState::Completed));
      }
      if start.elapsed() >= time_budget {
        return cold!(Ok(ExecState::Suspended(self.cf)));
      }
    }
  }

  #[inline(always)]
  pub(crate) fn run_with_fuel(mut self, fuel: u32) -> Result<ExecState> {
    self.fuel_metered = true;
    self.store.execution_fuel = fuel;
    if self.store.execution_fuel == 0 {
      return Ok(ExecState::Suspended(self.cf));
    }

    loop {
      Bounded::run(&mut self)?;
      if self.completed {
        return cold!(Ok(ExecState::Completed));
      }
      self.store.execution_fuel = self.store.execution_fuel.saturating_sub(CHECKPOINT_INTERVAL);
      if self.store.execution_fuel == 0 {
        return cold!(Ok(ExecState::Suspended(self.cf)));
      }
    }
  }
}

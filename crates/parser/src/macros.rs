// Format detailed diagnostics only when the debug feature is enabled.
macro_rules! error_message {
    ($compact:literal | $format:literal $(, $arg:expr)* $(,)?) => {{
        #[cfg(feature = "debug")]
        { alloc::format!($format $(, $arg)*).into_boxed_str() }
        #[cfg(not(feature = "debug"))]
        {
            let _ = || { $(let _ = &$arg;)* };
            alloc::boxed::Box::<str>::from($compact)
        }
    }};
}
pub(crate) use error_message;

pub(crate) mod visit {
    // Both visitors implement SIMD, so avoid the default visitor's dynamic SIMD fallback.
    macro_rules! visit_all_operators {
        ($(@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {
            fn visit_operator(&mut self, operator: &wasmparser::Operator<'a>) -> Self::Output {
                match operator {
                    $(wasmparser::Operator::$op $({ $($arg),* })? => self.$visit($($($arg.clone()),*)?),)*
                    _ => Err(crate::ParseError::UnsupportedOperator("unknown operator")),
                }
            }
        };
    }

    // Rule families are chosen at the call site, not by matching the emitted opcode.
    // Unary instruction operands are evaluated once and passed after family arguments.
    macro_rules! emit_selected {
        ($builder:ident, $inputs:expr, $outputs:expr, $op:ident($value:expr) [$family:ident($($arg:expr),* $(,)?)]) => {{
            let operand = $value;
            $builder.emit_with($inputs, $outputs, tinywasm_types::Instruction::$op(operand),
                |tail, data| crate::selection::$family(tail, data $(, $arg)*, operand))
        }};
        ($builder:ident, $inputs:expr, $outputs:expr, $op:ident [$family:ident($($arg:expr),* $(,)?)]) => {
            $builder.emit_with($inputs, $outputs, tinywasm_types::Instruction::$op,
                |tail, data| crate::selection::$family(tail, data $(, $arg)*))
        };
        ($builder:ident, $inputs:expr, $outputs:expr, $op:ident $(($($value:expr),*))?) => {
            $builder.emit($inputs, $outputs, tinywasm_types::Instruction::$op $(($($value),*))?)
        };
    }
    #[cfg(feature = "validate")]
    macro_rules! validate_then_visit {
        (@methods $visitor:ident; $( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {$(
            fn $visit(&mut self $($(,$arg: $argty)*)?) -> Self::Output {
                if let Err(e) = self.validator.$visitor(self.position).$visit($($($arg.clone()),*)?) {
                    core::hint::cold_path();
                    return Err(e.into());
                }
                self.builder.$visit($($($arg),*)?)
            }
        )*};
        ($($operators:tt)*) => { validate_then_visit!(@methods visitor; $($operators)*); };
    }

    #[cfg(feature = "validate")]
    macro_rules! validate_then_visit_simd {
        ($($operators:tt)*) => { validate_then_visit!(@methods simd_visitor; $($operators)*); };
    }

    // Optional `[family(args)]` selects rules alongside the visitor's stack effect
    // and fallback opcode. Instructions without a family use plain emission.
    macro_rules! lowering_ops {
        () => {};
        (atomic $op:ident $inputs:tt => $outputs:tt {
            $($visit:ident => $width:literal),* $(,)?
        } $($rest:tt)*) => {
            $(lowering_ops!(@atomic $op $inputs => $outputs $visit $width);)*
            lowering_ops!($($rest)*);
        };
        (indexed $resolver:tt $inputs:tt => $outputs:tt {
            $($visit:ident($index:ident: $ty:ty) => $instr:ident $([$family:ident($($rule_arg:expr),*)])?),* $(,)?
        } $($rest:tt)*) => {
            $(lowering_ops!(@indexed $resolver $inputs => $outputs $visit($index: $ty) => $instr $([$family($($rule_arg),*)])?);)*
            lowering_ops!($($rest)*);
        };
        ($kind:ident $inputs:tt => $outputs:tt {
            $($visit:ident $(($($arg:ident: $ty:ty),+))? => $instr:ident $([$family:ident($($rule_arg:expr),*)])?),* $(,)?
        } $($rest:tt)*) => {
            $(lowering_ops!(@$kind $inputs => $outputs $visit $(($($arg: $ty),+))? => $instr $([$family($($rule_arg),*)])?);)*
            lowering_ops!($($rest)*);
        };
        (effect $inputs:tt => $outputs:tt { $($visit:ident),* $(,)? } $($rest:tt)*) => {
            $(lowering_ops!(@effect $inputs => $outputs $visit);)*
            lowering_ops!($($rest)*);
        };
        (heap $nullable:literal $inputs:tt => $outputs:tt {
            $($visit:ident => $instr:ident),* $(,)?
        } $($rest:tt)*) => {
            $(
                fn $visit(&mut self, heap_type: wasmparser::HeapType) -> Self::Output {
                    let ty = convert_heap_type(heap_type, $nullable)?;
                    lowering_ops!(@emit self fixed $inputs => $outputs $instr(ty))
                }
            )*
            lowering_ops!($($rest)*);
        };

        (@fixed [$($input:ident),*] => [$($output:ident),*]
            $visit:ident $(($($arg:ident: $ty:ty),+))? => $instr:ident $([$family:ident($($rule_arg:expr),*)])?
        ) => {
            fn $visit(&mut self $(, $($arg: $ty),+)?) -> Self::Output {
                lowering_ops!(@emit self fixed [$($input),*] => [$($output),*]
                    $instr $(($($arg),+))? $([$family($($rule_arg),*)])?)
            }
        };
        (@memory [$($input:ident),*] => [$($output:ident),*]
            $visit:ident $(($lane:ident: $ty:ty))? => $instr:ident $([$family:ident($($rule_arg:expr),*)])?
        ) => {
            fn $visit(&mut self, memarg: wasmparser::MemArg $(, $lane: $ty)?) -> Self::Output {
                let (address, memory_arg_idx) = self.memory_operand(memarg)?;
                let operand = lowering_ops!(@memory_arg memory_arg_idx, address $(, $lane)?);
                lowering_ops!(@emit self (address) [$($input),*] => [$($output),*]
                    $instr(operand) $([$family($($rule_arg),*)])?)
            }
        };
        (@memory_arg $index:ident, $address:ident) => {
            tinywasm_types::MemoryArg { memory_arg_idx: $index, address64: $address == ValueLane::S64 }
        };
        (@memory_arg $index:ident, $address:ident, $lane:ident) => {
            tinywasm_types::MemoryLaneArg { memory_arg_idx: $index, lane: $lane }
        };
        (@atomic $op:ident [$($input:ident),*] => [$($output:ident),*] $visit:ident $width:literal) => {
            fn $visit(&mut self, memarg: wasmparser::MemArg) -> Self::Output {
                if memarg.align != ($width as u32).trailing_zeros() as u8 {
                    return Err(crate::ParseError::Other("invalid atomic alignment".into()));
                }
                let (address, memory) = self.memory_operand(memarg)?;
                let is_64 = lowering_ops!(@atomic_is64 [$($input),*] [$($output),*]);
                let arg = tinywasm_types::AtomicArg::new(
                    memory, tinywasm_types::AtomicWidth::from_bytes($width), is_64, tinywasm_types::AtomicOp::$op,
                );
                self.emit(
                    &[$(lowering_ops!(@size $input (address))),*],
                    &[$(lowering_ops!(@size $output (address))),*],
                    tinywasm_types::Instruction::Atomic(arg),
                )
            }
        };
        (@atomic_is64 [Addr, S64 $(, S64)*] $outputs:tt) => { true };
        (@atomic_is64 [Addr] [S64]) => { true };
        (@atomic_is64 $inputs:tt $outputs:tt) => { false };
        (@indexed ($($resolver:ident).+) [$($input:ident),*] => [$($output:ident),*]
            $visit:ident($index:ident: $ty:ty) => $instr:ident $([$family:ident($($rule_arg:expr),*)])?
        ) => {
            fn $visit(&mut self, $index: $ty) -> Self::Output {
                let lane = self.$($resolver).+($index)?;
                lowering_ops!(@emit self (lane) [$($input),*] => [$($output),*]
                    $instr($index) $([$family($($rule_arg),*)])?)
            }
        };
        (@effect [$($input:ident),*] => [$($output:ident),*] $visit:ident) => {
            fn $visit(&mut self) -> Self::Output {
                self.apply_effect(&[$(lowering_ops!(@size $input)),*], &[$(lowering_ops!(@size $output)),*])
            }
        };
        (@terminating [$($input:ident),*] => [$($output:ident),*] $visit:ident => $instr:ident) => {
            fn $visit(&mut self) -> Self::Output {
                self.mark_unreachable();
                self.emit_boundary(
                    &[$(lowering_ops!(@size $input)),*],
                    &[$(lowering_ops!(@size $output)),*],
                    Instruction::$instr,
                )
            }
        };

        (@emit $self:ident $context:tt [$($input:ident),*] => [$($output:ident),*] $($instruction:tt)+) => {
            emit_selected!($self,
                &[$(lowering_ops!(@size $input $context)),*],
                &[$(lowering_ops!(@size $output $context)),*],
                $($instruction)+
            )
        };
        (@size Addr ($address:ident)) => { $address };
        (@size Field ($field:ident)) => { $field };
        (@size $size:ident $($context:tt)?) => { ValueLane::$size };
    }

    macro_rules! impl_visit_operator {
        ($(@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {
            $(impl_visit_operator!(@@$proposal $op $({ $($arg: $argty),* })? => $visit ($($ann:tt)*));)*
        };

        (@@mvp $($rest:tt)* ) => {};
        (@@reference_types $($rest:tt)* ) => {};
        (@@sign_extension $($rest:tt)* ) => {};
        (@@saturating_float_to_int $($rest:tt)* ) => {};
        (@@bulk_memory $($rest:tt)* ) => {};
        (@@simd $($rest:tt)* ) => {};
        (@@wide_arithmetic $($rest:tt)* ) => {};
        (@@relaxed_simd $($rest:tt)* ) => {};
        (@@tail_call $($rest:tt)* ) => {};
        (@@function_references $($rest:tt)* ) => {};
        (@@gc $($rest:tt)* ) => {};
        (@@exceptions $($rest:tt)* ) => {};
        (@@threads $($rest:tt)* ) => {};

        (@@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*)) => {
            fn $visit(&mut self $($(,_: $argty)*)?) -> Self::Output {
                Err(crate::ParseError::UnsupportedOperator(stringify!($visit)))
            }
        };
    }

    pub(crate) use {emit_selected, impl_visit_operator, lowering_ops, visit_all_operators};
    #[cfg(feature = "validate")]
    pub(crate) use {validate_then_visit, validate_then_visit_simd};
}

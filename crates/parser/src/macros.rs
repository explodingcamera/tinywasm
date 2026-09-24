pub(crate) mod visit {
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
        ($( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {$(
            fn $visit(&mut self $($(,$arg: $argty)*)?) -> Self::Output {
                if let Err(e) = self.validator.visitor(self.position).$visit($($($arg.clone()),*)?) {
                    core::hint::cold_path();
                    return Err(crate::ParseError::ParseError { message: e.to_string(), offset: self.position });
                }
                self.builder.$visit($($($arg),*)?)
            }
        )*};
    }

    #[cfg(feature = "validate")]
    macro_rules! validate_then_visit_simd {
        ($( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {$(
            fn $visit(&mut self $($(,$arg: $argty)*)?) -> Self::Output {
                if let Err(e) = self.validator.simd_visitor(self.position).$visit($($($arg.clone()),*)?) {
                    core::hint::cold_path();
                    return Err(crate::ParseError::ParseError { message: e.to_string(), offset: self.position });
                }
                self.builder.$visit($($($arg),*)?)
            }
        )*};
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
        (unsupported $args:tt { $($visit:ident),* $(,)? } $($rest:tt)*) => {
            $(lowering_ops!(@unsupported $args $visit);)*
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

        (@unsupported [$($argty:ty),*] $visit:ident) => {
            fn $visit(&mut self $(, _: $argty)*) -> Self::Output {
                Err(crate::ParseError::UnsupportedOperator(stringify!($visit).to_string()))
            }
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
                let address = self.metadata.memory_size(memarg.memory)?;
                self.mark_memory(memarg.memory);
                let memory_arg_idx = self.push128(tinywasm_types::Operand128::<
                    tinywasm_types::MemoryOperand,
                >::new(memarg.offset, memarg.memory))?;
                lowering_ops!(@emit self address(address) [$($input),*] => [$($output),*]
                    $instr(lowering_ops!(@memory_arg memory_arg_idx $(, $lane)?)) $([$family($($rule_arg),*)])?)
            }
        };
        (@memory_arg $memory_arg_idx:ident) => {
            $memory_arg_idx
        };
        (@memory_arg $memory_arg_idx:ident, $lane:ident) => {
            tinywasm_types::MemoryLaneArg { memory_arg_idx: $memory_arg_idx, lane: $lane }
        };
        (@atomic $op:ident [$($input:ident),*] => [$($output:ident),*] $visit:ident $width:literal) => {
            fn $visit(&mut self, memarg: wasmparser::MemArg) -> Self::Output {
                if memarg.align != ($width as u32).trailing_zeros() as u8 {
                    return Err(crate::ParseError::Other("invalid atomic alignment".into()));
                }
                let address = self.metadata.memory_size(memarg.memory)?;
                self.mark_memory(memarg.memory);
                let memory = self.push128(tinywasm_types::Operand128::<tinywasm_types::MemoryOperand>::new(
                    memarg.offset, memarg.memory,
                ))?;
                let is_64 = lowering_ops!(@atomic_is64 [$($input),*] [$($output),*]);
                let arg = tinywasm_types::AtomicArg::new(
                    memory, tinywasm_types::AtomicWidth::from_bytes($width), is_64, tinywasm_types::AtomicOp::$op,
                );
                self.emit(
                    &[$(lowering_ops!(@size $input, address)),*],
                    &[$(lowering_ops!(@size $output, address)),*],
                    tinywasm_types::Instruction::Atomic(arg),
                )
            }
        };
        (@atomic_is64 [Addr, S64 $(, S64)*] $outputs:tt) => { true };
        (@atomic_is64 [Addr] [S64]) => { true };
        (@atomic_is64 $inputs:tt $outputs:tt) => { false };
        (@global $inputs:tt => $outputs:tt $($operator:tt)*) => {
            lowering_ops!(@resolved global_size $inputs => $outputs $($operator)*);
        };
        (@memory_index $inputs:tt => $outputs:tt $($operator:tt)*) => {
            lowering_ops!(@memory_index_impl $inputs => $outputs $($operator)*);
        };
        (@memory_index_impl [$($input:ident),*] => [$($output:ident),*]
            $visit:ident($index:ident: $ty:ty) => $instr:ident $([$family:ident($($rule_arg:expr),*)])?
        ) => {
            fn $visit(&mut self, $index: $ty) -> Self::Output {
                let address = self.metadata.memory_size($index)?;
                self.mark_memory($index);
                lowering_ops!(@emit self address(address) [$($input),*] => [$($output),*]
                    $instr($index) $([$family($($rule_arg),*)])?)
            }
        };
        (@table $inputs:tt => $outputs:tt $($operator:tt)*) => {
            lowering_ops!(@resolved table_size $inputs => $outputs $($operator)*);
        };
        (@array_field [$($input:ident),*] => [$($output:ident),*]
            $visit:ident($type_index:ident: $type_ty:ty $(, $arg:ident: $arg_ty:ty)*) => $instr:ident
        ) => {
            fn $visit(&mut self, $type_index: $type_ty $(, $arg: $arg_ty)*) -> Self::Output {
                let size = self.metadata.array_field($type_index)?;
                lowering_ops!(@emit self address(size) [$($input),*] => [$($output),*]
                    $instr($type_index $(, $arg)*))
            }
        };
        (@resolved $resolver:ident [$($input:ident),*] => [$($output:ident),*]
            $visit:ident($index:ident: $ty:ty) => $instr:ident
        ) => {
            fn $visit(&mut self, $index: $ty) -> Self::Output {
                let address = self.metadata.$resolver($index)?;
                lowering_ops!(@emit self address(address) [$($input),*] => [$($output),*]
                    $instr($index))
            }
        };
        (@resolved $resolver:ident [$($input:ident),*] => [$($output:ident),*]
            $visit:ident($arg:ident: $arg_ty:ty, $index:ident: $index_ty:ty) => $instr:ident
        ) => {
            fn $visit(&mut self, $arg: $arg_ty, $index: $index_ty) -> Self::Output {
                let address = self.metadata.$resolver($index)?;
                lowering_ops!(@emit self address(address) [$($input),*] => [$($output),*]
                    $instr($arg, $index))
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

        (@emit $self:ident fixed [$($input:ident),*] => [$($output:ident),*] $($instruction:tt)+) => {
            emit_selected!($self,
                &[$(lowering_ops!(@size $input)),*],
                &[$(lowering_ops!(@size $output)),*],
                $($instruction)+
            )
        };
        (@emit $self:ident address($address:ident) [$($input:ident),*] => [$($output:ident),*] $($instruction:tt)+) => {
            emit_selected!($self,
                &[$(lowering_ops!(@size $input, $address)),*],
                &[$(lowering_ops!(@size $output, $address)),*],
                $($instruction)+
            )
        };

        (@size Addr, $address:ident) => { $address };
        (@size Field, $address:ident) => { $address };
        (@size $size:ident $(, $address:ident)?) => { ValueLane::$size };
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
                Err(crate::ParseError::UnsupportedOperator(stringify!($visit).to_string()))
            }
        };
    }

    pub(crate) use {emit_selected, impl_visit_operator, lowering_ops};
    #[cfg(feature = "validate")]
    pub(crate) use {validate_then_visit, validate_then_visit_simd};
}

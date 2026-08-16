pub(crate) mod visit {
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

    macro_rules! lowering_ops {
        () => {};
        ($kind:ident $inputs:tt => $outputs:tt {
            $($visit:ident $(($($arg:ident: $ty:ty),+))? => $instr:ident),* $(,)?
        } $($rest:tt)*) => {
            $(lowering_ops!(@$kind $inputs => $outputs $visit $(($($arg: $ty),+))? => $instr);)*
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
                    lowering_ops!(@emit self fixed $inputs => $outputs Instruction::$instr(ty))
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
            $visit:ident $(($($arg:ident: $ty:ty),+))? => $instr:ident
        ) => {
            fn $visit(&mut self $(, $($arg: $ty),+)?) -> Self::Output {
                lowering_ops!(@emit self fixed [$($input),*] => [$($output),*]
                    Instruction::$instr $(($($arg),+))?.into())
            }
        };
        (@memory [$($input:ident),*] => [$($output:ident),*]
            $visit:ident $(($lane:ident: $ty:ty))? => $instr:ident
        ) => {
            fn $visit(&mut self, memarg: wasmparser::MemArg $(, $lane: $ty)?) -> Self::Output {
                let address = self.metadata.memory_size(memarg.memory)?;
                let memory_arg_idx = self.push_operand(MemoryArg::new(memarg.offset, memarg.memory))?;
                lowering_ops!(@emit self address(address) [$($input),*] => [$($output),*]
                    lowering_ops!(@memory_instruction $instr memory_arg_idx $(, $lane)?))
            }
        };
        (@memory_instruction $instr:ident $memory_arg_idx:ident) => {
            Instruction::$instr($memory_arg_idx)
        };
        (@memory_instruction $instr:ident $memory_arg_idx:ident, $lane:ident) => {
            Instruction::$instr(tinywasm_types::MemoryLaneArg { memory_arg_idx: $memory_arg_idx, lane: $lane })
        };
        (@global $inputs:tt => $outputs:tt $($operator:tt)*) => {
            lowering_ops!(@resolved global_size $inputs => $outputs $($operator)*);
        };
        (@memory_index $inputs:tt => $outputs:tt $($operator:tt)*) => {
            lowering_ops!(@resolved memory_size $inputs => $outputs $($operator)*);
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
                    Instruction::$instr($type_index $(, $arg)*).into())
            }
        };
        (@resolved $resolver:ident [$($input:ident),*] => [$($output:ident),*]
            $visit:ident($index:ident: $ty:ty) => $instr:ident
        ) => {
            fn $visit(&mut self, $index: $ty) -> Self::Output {
                let address = self.metadata.$resolver($index)?;
                lowering_ops!(@emit self address(address) [$($input),*] => [$($output),*]
                    Instruction::$instr($index).into())
            }
        };
        (@resolved $resolver:ident [$($input:ident),*] => [$($output:ident),*]
            $visit:ident($arg:ident: $arg_ty:ty, $index:ident: $index_ty:ty) => $instr:ident
        ) => {
            fn $visit(&mut self, $arg: $arg_ty, $index: $index_ty) -> Self::Output {
                let address = self.metadata.$resolver($index)?;
                lowering_ops!(@emit self address(address) [$($input),*] => [$($output),*]
                    Instruction::$instr($arg, $index).into())
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
                lowering_ops!(@emit self fixed [$($input),*] => [$($output),*] Instruction::$instr)
            }
        };

        (@emit $self:ident fixed [$($input:ident),*] => [$($output:ident),*] $instruction:expr) => {
            $self.emit(
                &[$(lowering_ops!(@size $input)),*],
                &[$(lowering_ops!(@size $output)),*],
                $instruction,
            )
        };
        (@emit $self:ident address($address:ident) [$($input:ident),*] => [$($output:ident),*] $instruction:expr) => {
            $self.emit(
                &[$(lowering_ops!(@size $input, $address)),*],
                &[$(lowering_ops!(@size $output, $address)),*],
                $instruction,
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

        (@@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*)) => {
            fn $visit(&mut self $($(,_: $argty)*)?) -> Self::Output {
                Err(crate::ParseError::UnsupportedOperator(stringify!($visit).to_string()))
            }
        };
    }

    pub(crate) use {impl_visit_operator, lowering_ops};
    #[cfg(feature = "validate")]
    pub(crate) use {validate_then_visit, validate_then_visit_simd};
}

pub(crate) mod optimize {
    macro_rules! replace {
        ($instructions:ident, $read:ident, $consumed:expr => [$($out:expr),+ $(,)?]) => {{
            const {
                assert!($consumed >= 1 && $consumed <= 3);
                assert!([$(stringify!($out)),+].len() <= $consumed + 1);
            }
            let replacements = [$($out),+];
            let start = $read - $consumed;
            $instructions[start..start + replacements.len()].copy_from_slice(&replacements);
            $instructions.truncate(start + replacements.len());
            #[allow(unused_assignments)]
            { $read = $instructions.len() - 1; }
        }};
        ($instructions:ident, $read:ident, $consumed:expr => $out:expr) => {
            replace!($instructions, $read, $consumed => [$out]);
        };
        ($instructions:ident, *$read:ident, $consumed:expr => [$($out:expr),+ $(,)?]) => {{
            const {
                assert!($consumed >= 1 && $consumed <= 3);
                assert!([$(stringify!($out)),+].len() <= $consumed + 1);
            }
            let replacements = [$($out),+];
            let start = *$read - $consumed;
            $instructions[start..start + replacements.len()].copy_from_slice(&replacements);
            $instructions.truncate(start + replacements.len());
            *$read = $instructions.len() - 1;
        }};
        ($instructions:ident, *$read:ident, $consumed:expr => $out:expr) => {
            replace!($instructions, *$read, $consumed => [$out])
        };
    }

    macro_rules! rewrite {
        ($instructions:ident, $read:ident, [$($pattern:pat),+] $(if ($($guard:tt)+))? => [$($out:expr),+ $(,)?]) => {
            rewrite!($instructions, $read, [$($pattern),+] $(if ($($guard)+))? => {
                replace!($instructions, $read, [$(stringify!($pattern)),+].len() => [$($out),+]);
            })
        };
        ($instructions:ident, $read:ident, [$($pattern:pat),+] $(if ($($guard:tt)+))? => $body:block $(,)?) => {{
            const CONSUMED: usize = [$(stringify!($pattern)),+].len();
            if $read >= $instructions.block_start + CONSUMED {
                let previous: [Instruction; CONSUMED] = $instructions[$read - CONSUMED..$read].try_into().unwrap();
                if let [$($pattern),+] = previous $(
                    && $($guard)+
                )? {
                    $body
                    continue;
                }
            }
        }};
        ($instructions:ident, $read:ident, [$($pattern:pat),+] $(if ($($guard:tt)+))? => $out:expr $(,)?) => {
            rewrite!($instructions, $read, [$($pattern),+] $(if ($($guard)+))? => {
                replace!($instructions, $read, [$(stringify!($pattern)),+].len() => $out);
            })
        };
    }

    pub(crate) use {replace, rewrite};
}

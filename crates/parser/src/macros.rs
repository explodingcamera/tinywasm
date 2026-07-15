pub(crate) mod visit {
    macro_rules! validate_then_visit {
        ($( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {$(
            fn $visit(&mut self $($(,$arg: $argty)*)?) -> Self::Output {
                if let Some(validator) = self.validator.as_mut() {
                    if let Err(e) = validator.visitor(self.position).$visit($($($arg.clone()),*)?) {
                        core::hint::cold_path();
                        return Err(crate::ParseError::ParseError { message: e.to_string(), offset: self.position });
                    }
                }
                self.builder.$visit($($($arg),*)?)
            }
        )*};
    }

    macro_rules! validate_then_visit_simd {
        ($( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {$(
            fn $visit(&mut self $($(,$arg: $argty)*)?) -> Self::Output {
                if let Some(validator) = self.validator.as_mut() {
                    if let Err(e) = validator.simd_visitor(self.position).$visit($($($arg.clone()),*)?) {
                        core::hint::cold_path();
                        return Err(crate::ParseError::ParseError { message: e.to_string(), offset: self.position });
                    }
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
                lowering_ops!(@emit self address(address) [$($input),*] => [$($output),*]
                    Instruction::$instr(MemoryArg::new(memarg.offset, memarg.memory) $(, $lane)?).into())
            }
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
        (@size $size:ident $(, $address:ident)?) => { OperandSize::$size };
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

        (@@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*)) => {
            fn $visit(&mut self $($(,_: $argty)*)?) -> Self::Output {
                Err(crate::ParseError::UnsupportedOperator(stringify!($visit).to_string()))
            }
        };
    }

    pub(crate) use {impl_visit_operator, lowering_ops, validate_then_visit, validate_then_visit_simd};
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
    }

    macro_rules! rewrite {
        ($instructions:ident, $read:ident, [$($pattern:pat),+] $(if ($($guard:tt)+))? => [$($out:expr),+ $(,)?]) => {
            rewrite!($instructions, $read, [$($pattern),+] $(if ($($guard)+))? => {
                replace!($instructions, $read, [$(stringify!($pattern)),+].len() => [$($out),+]);
            })
        };
        ($instructions:ident, $read:ident, [$($pattern:pat),+] $(if ($($guard:tt)+))? => $body:block $(,)?) => {{
            const CONSUMED: usize = [$(stringify!($pattern)),+].len();
            if !$instructions.tail_rewritten
                && $read < $instructions.len()
                && $read >= $instructions.block_start + CONSUMED
            {
                let previous: [Instruction; CONSUMED] = $instructions[$read - CONSUMED..$read].try_into().unwrap();
                if let [$($pattern),+] = previous $(
                    && $($guard)+
                )? {
                    $instructions.tail_rewritten = true;
                    $body
                }
            }
        }};
        ($instructions:ident, $read:ident, [$($pattern:pat),+] $(if ($($guard:tt)+))? => $out:expr $(,)?) => {
            rewrite!($instructions, $read, [$($pattern),+] $(if ($($guard)+))? => {
                replace!($instructions, $read, [$(stringify!($pattern)),+].len() => $out);
            })
        };
    }

    macro_rules! define_local_source_resolver {
        (
            $name:ident,
            get = $get:ident,
            tee = $tee:ident,
            set = $set:ident,
            binop_local_local_tee = $lltee:ident,
            binop_local_local_set = $llset:ident,
            binop_local_const_tee = $lctee:ident,
            binop_local_const_set = $lcset:ident
            $(, load_local_tee = $loadtee:ident, load_local_set = $loadset:ident)?
        ) => {
            fn $name(instr: Instruction) -> Option<(Option<Instruction>, u16)> {
                Some(match instr {
                    Instruction::$get(local) => (None, local),
                    Instruction::$tee(local) => (Some(Instruction::$set(local)), local),
                    Instruction::$lltee(op, a, b, local) => (Some(Instruction::$llset(op, a, b, local)), local),
                    Instruction::$lctee(op, src, c, local) => (Some(Instruction::$lcset(op, src, c, local)), local),
                    $(Instruction::$loadtee(memarg, addr, local) => (Some(Instruction::$loadset(memarg, addr, local)), local.into()),)?
                    _ => return None,
                })
            }
        };
    }

    macro_rules! fold_local_binop {
        (
            $instrs:ident, $read:expr, $dst:expr,
            source = $source:ident,
            op = $op:ident,
            const = $const:ident,
            local_local = $local_local:ident,
            local_const = $local_const:expr
        ) => {{
            if !$instrs.tail_rewritten
                && $read < $instrs.len()
                && $read >= $instrs.block_start + 3
                && let [lhs_src, rhs_src, raw_op] = [$instrs[$read - 3], $instrs[$read - 2], $instrs[$read - 1]]
                && let Some((lhs_instr, lhs)) = $source(lhs_src)
                && let Some(op) = $op(raw_op)
            {
                if let Some((rhs_instr, rhs)) = $source(rhs_src) {
                    if rhs_instr.is_none() || rhs != lhs {
                        $instrs.tail_rewritten = true;
                        $instrs.truncate($read - 3);
                        $instrs.extend(lhs_instr);
                        $instrs.extend(rhs_instr);
                        $instrs.push(Instruction::$local_local(op, lhs, rhs, $dst));
                        $read = $instrs.len() - 1;
                    }
                } else if let Some(imm) = $const(rhs_src, raw_op) {
                    $instrs.tail_rewritten = true;
                    $instrs.truncate($read - 3);
                    $instrs.extend(lhs_instr);
                    $instrs.push($local_const($dst, lhs, op, imm));
                    $read = $instrs.len() - 1;
                }
            }
        }};
    }

    macro_rules! rewrite_local_set_direct {
        (
            $instrs:ident, $read:ident, $dst:expr,
            get = $get:ident,
            copy = $copy:ident,
            binop_local_local = $ll:ident,
            binop_local_local_set = $llset:ident,
            binop_local_const = $lc:ident,
            binop_local_const_set = $lcset:expr
            $(, const_instr = $const_instr:ident, set_local_const = $set_local_const:ident)?
        ) => {{
            rewrite!($instrs, $read, [$get(src)] if (src != $dst) => Instruction::$copy(src, $dst));
            if !$instrs.tail_rewritten
                && $read < $instrs.len()
                && $read > $instrs.block_start
                && let Instruction::$get(src) = $instrs[$read - 1]
                && src == $dst
            {
                $instrs.tail_rewritten = true;
                $instrs.truncate($read - 1);
                $read = $instrs.len();
            }
            $(rewrite!($instrs, $read, [$const_instr(c)] => Instruction::$set_local_const($dst, c));)?
            rewrite!($instrs, $read, [$ll(op, a, b)] => Instruction::$llset(op, a, b, $dst));
            rewrite!($instrs, $read, [$lc(op, src, c)] => { replace!($instrs, $read, 1 => $lcset($dst, src, op, c)); });
        }};
    }

    macro_rules! rewrite_local_tee_direct {
        (
            $instrs:ident, $read:ident, $dst:expr,
            get = $get:ident,
            binop_local_local = $ll:ident,
            binop_local_local_tee = $lltee:ident,
            binop_local_const = $lc:ident,
            binop_local_const_tee = $lctee:ident
        ) => {{
            rewrite!($instrs, $read, [$get(src)] if (src == $dst) => Instruction::$get(src));
            rewrite!($instrs, $read, [$ll(op, a, b)] => Instruction::$lltee(op, a, b, $dst));
            rewrite!($instrs, $read, [$lc(op, src, c)] => Instruction::$lctee(op, src, c, $dst));
        }};
    }

    macro_rules! rewrite_drop_tee_direct {
        (
            $instrs:ident, $read:ident,
            tee = $tee:ident,
            set = $set:ident,
            binop_local_local_tee = $lltee:ident,
            binop_local_local_set = $llset:ident,
            binop_local_const_tee = $lctee:ident,
            binop_local_const_set = $lcset:ident
        ) => {{
            rewrite!($instrs, $read, [$tee(local)] => Instruction::$set(local));
            rewrite!($instrs, $read, [$lltee(op, a, b, dst)] => Instruction::$llset(op, a, b, dst));
            rewrite!($instrs, $read, [$lctee(op, src, c, dst)] => Instruction::$lcset(op, src, c, dst));
        }};
    }

    pub(crate) use {
        define_local_source_resolver, fold_local_binop, replace, rewrite, rewrite_drop_tee_direct,
        rewrite_local_set_direct, rewrite_local_tee_direct,
    };
}

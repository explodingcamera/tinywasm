pub(crate) mod visit {
    macro_rules! validate_then_visit {
        ($( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {$(
            fn $visit(&mut self $($(,$arg: $argty)*)?) -> Self::Output {
                self.1.$visit($($($arg.clone()),*)?);
                self.1.validator_visitor(self.0).$visit($($($arg),*)?)?;
                Ok(())
            }
        )*};
    }

    macro_rules! define_operand {
        ($name:ident($instr:expr, $ty:ty)) => {
            fn $name(&mut self, arg: $ty) -> Self::Output {
                self.instructions.push($instr(arg).into());
            }
        };

        ($name:ident($instr:expr, $ty:ty, $ty2:ty)) => {
            fn $name(&mut self, arg: $ty, arg2: $ty2) -> Self::Output {
                self.instructions.push($instr(arg, arg2).into());
            }
        };

        ($name:ident($instr:expr)) => {
            fn $name(&mut self) -> Self::Output {
                self.instructions.push($instr.into());
            }
        };
    }

    macro_rules! define_operands {
        ($($name:ident($instr:ident $(,$ty:ty)*)),*) => {$(
            define_operand!($name(Instruction::$instr $(,$ty)*));
        )*};
    }

    macro_rules! define_mem_operands {
        ($($name:ident($instr:ident)),*) => {$(
            fn $name(&mut self, memarg: wasmparser::MemArg) -> Self::Output {
                self.instructions.push(Instruction::$instr(MemoryArg::new(memarg.offset, memarg.memory)));
            }
        )*};
    }

    macro_rules! define_mem_operands_simd {
        ($($name:ident($instr:ident)),*) => {$(
            fn $name(&mut self, memarg: wasmparser::MemArg) -> Self::Output {
                self.instructions.push(Instruction::$instr(MemoryArg::new(memarg.offset, memarg.memory)).into());
            }
        )*};
    }

    macro_rules! define_mem_operands_simd_lane {
        ($($name:ident($instr:ident)),*) => {$(
            fn $name(&mut self, memarg: wasmparser::MemArg, lane: u8) -> Self::Output {
                self.instructions.push(Instruction::$instr(MemoryArg::new(memarg.offset, memarg.memory), lane).into());
            }
        )*};
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
            fn $visit(&mut self $($(,_: $argty)*)?) {
                self.unsupported(stringify!($visit))
            }
        };
    }

    pub(crate) use {
        define_mem_operands, define_mem_operands_simd, define_mem_operands_simd_lane, define_operand, define_operands,
        impl_visit_operator, validate_then_visit,
    };
}

pub(crate) mod optimize {
    macro_rules! replace {
        ($instructions:ident, $read:ident, 1 => [$a:expr $(,)?]) => {{
            $instructions[$read - 1] = Instruction::Nop;
            $instructions[$read] = $a;
        }};
        ($instructions:ident, $read:ident, 1 => [$a:expr, $b:expr $(,)?]) => {{
            $instructions[$read - 1] = $a;
            $instructions[$read] = $b;
        }};
        ($instructions:ident, $read:ident, 2 => [$a:expr $(,)?]) => {{
            $instructions[$read - 2] = Instruction::Nop;
            $instructions[$read - 1] = Instruction::Nop;
            $instructions[$read] = $a;
        }};
        ($instructions:ident, $read:ident, 2 => [$a:expr, $b:expr $(,)?]) => {{
            $instructions[$read - 2] = Instruction::Nop;
            $instructions[$read - 1] = $a;
            $instructions[$read] = $b;
        }};
        ($instructions:ident, $read:ident, 2 => [$a:expr, $b:expr, $c:expr $(,)?]) => {{
            $instructions[$read - 2] = $a;
            $instructions[$read - 1] = $b;
            $instructions[$read] = $c;
        }};
        ($instructions:ident, $read:ident, 3 => [$a:expr $(,)?]) => {{
            $instructions[$read - 3] = Instruction::Nop;
            $instructions[$read - 2] = Instruction::Nop;
            $instructions[$read - 1] = Instruction::Nop;
            $instructions[$read] = $a;
        }};
        ($instructions:ident, $read:ident, 3 => [$a:expr, $b:expr $(,)?]) => {{
            $instructions[$read - 3] = Instruction::Nop;
            $instructions[$read - 2] = Instruction::Nop;
            $instructions[$read - 1] = $a;
            $instructions[$read] = $b;
        }};
        ($instructions:ident, $read:ident, 3 => [$a:expr, $b:expr, $c:expr $(,)?]) => {{
            $instructions[$read - 3] = Instruction::Nop;
            $instructions[$read - 2] = $a;
            $instructions[$read - 1] = $b;
            $instructions[$read] = $c;
        }};
        ($instructions:ident, $read:ident, 3 => [$a:expr, $b:expr, $c:expr, $d:expr $(,)?]) => {{
            $instructions[$read - 3] = $a;
            $instructions[$read - 2] = $b;
            $instructions[$read - 1] = $c;
            $instructions[$read] = $d;
        }};
        ($instructions:ident, $read:ident, 1 => $out:expr) => {
            replace!($instructions, $read, 1 => [$out]);
        };
        ($instructions:ident, $read:ident, 2 => $out:expr) => {
            replace!($instructions, $read, 2 => [$out]);
        };
        ($instructions:ident, $read:ident, 3 => $out:expr) => {
            replace!($instructions, $read, 3 => [$out]);
        };
    }

    macro_rules! rewrite {
        ($instructions:ident, $read:ident, [$a:pat] if ($($guard:tt)+) => [$($out:expr),+ $(,)?]) => {
            rewrite!($instructions, $read, [$a] if ($($guard)+) => { replace!($instructions, $read, 1 => [$($out),+]); })
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat] if ($($guard:tt)+) => [$($out:expr),+ $(,)?]) => {
            rewrite!($instructions, $read, [$a, $b] if ($($guard)+) => { replace!($instructions, $read, 2 => [$($out),+]); })
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat, $c:pat] if ($($guard:tt)+) => [$($out:expr),+ $(,)?]) => {
            rewrite!($instructions, $read, [$a, $b, $c] if ($($guard)+) => { replace!($instructions, $read, 3 => [$($out),+]); })
        };
        ($instructions:ident, $read:ident, [$a:pat] => [$($out:expr),+ $(,)?]) => {
            rewrite!($instructions, $read, [$a] => { replace!($instructions, $read, 1 => [$($out),+]); })
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat] => [$($out:expr),+ $(,)?]) => {
            rewrite!($instructions, $read, [$a, $b] => { replace!($instructions, $read, 2 => [$($out),+]); })
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat, $c:pat] => [$($out:expr),+ $(,)?]) => {
            rewrite!($instructions, $read, [$a, $b, $c] => { replace!($instructions, $read, 3 => [$($out),+]); })
        };
        ($instructions:ident, $read:ident, [$a:pat] if ($($guard:tt)+) => $body:block $(,)?) => {
            if $read > 0 && let $a = $instructions[$read - 1] && $($guard)+ {
                $body
            }
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat] if ($($guard:tt)+) => $body:block $(,)?) => {
            if $read > 1 && let ($a, $b) = ($instructions[$read - 2], $instructions[$read - 1]) && $($guard)+ {
                $body
            }
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat, $c:pat] if ($($guard:tt)+) => $body:block $(,)?) => {
            if $read > 2 && let ($a, $b, $c) = ($instructions[$read - 3], $instructions[$read - 2], $instructions[$read - 1]) && $($guard)+ {
                $body
            }
        };
        ($instructions:ident, $read:ident, [$a:pat] => $body:block $(,)?) => {
            if $read > 0 && let $a = $instructions[$read - 1] {
                $body
            }
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat] => $body:block $(,)?) => {
            if $read > 1 && let ($a, $b) = ($instructions[$read - 2], $instructions[$read - 1]) {
                $body
            }
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat, $c:pat] => $body:block $(,)?) => {
            if $read > 2 && let ($a, $b, $c) = ($instructions[$read - 3], $instructions[$read - 2], $instructions[$read - 1]) {
                $body
            }
        };
        ($instructions:ident, $read:ident, [$a:pat] if ($($guard:tt)+) => $out:expr $(,)?) => {
            rewrite!($instructions, $read, [$a] if ($($guard)+) => { replace!($instructions, $read, 1 => $out); })
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat] if ($($guard:tt)+) => $out:expr $(,)?) => {
            rewrite!($instructions, $read, [$a, $b] if ($($guard)+) => { replace!($instructions, $read, 2 => $out); })
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat, $c:pat] if ($($guard:tt)+) => $out:expr $(,)?) => {
            rewrite!($instructions, $read, [$a, $b, $c] if ($($guard)+) => { replace!($instructions, $read, 3 => $out); })
        };
        ($instructions:ident, $read:ident, [$a:pat] => $out:expr $(,)?) => {
            rewrite!($instructions, $read, [$a] => { replace!($instructions, $read, 1 => $out); })
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat] => $out:expr $(,)?) => {
            rewrite!($instructions, $read, [$a, $b] => { replace!($instructions, $read, 2 => $out); })
        };
        ($instructions:ident, $read:ident, [$a:pat, $b:pat, $c:pat] => $out:expr $(,)?) => {
            rewrite!($instructions, $read, [$a, $b, $c] => { replace!($instructions, $read, 3 => $out); })
        };
    }

    macro_rules! define_local_source_resolver {
        (
            $name:ident,
            get = $get:ident,
            tee = $tee:ident,
            set = $set:ident,
            copy = $copy:ident,
            binop_local_local_tee = $lltee:ident,
            binop_local_local_set = $llset:ident,
            binop_local_const_tee = $lctee:ident,
            binop_local_const_set = $lcset:ident
            $(, load_local_tee = $loadtee:ident, load_local_set = $loadset:ident)?
        ) => {
            fn $name(instrs: &mut [Instruction], read: usize, instr: Instruction) -> Option<(Instruction, u16)> {
                Some(match instr {
                    Instruction::$get(local) => (Instruction::Nop, local),
                    Instruction::$tee(local) => {
                        let replacement = if let Some([(prev_idx, Instruction::$get(src))]) = previous_non_nop::<1>(instrs, read) {
                            instrs[prev_idx] = Instruction::Nop;
                            if src == local { Instruction::Nop } else { Instruction::$copy(src, local) }
                        } else {
                            Instruction::$set(local)
                        };
                        (replacement, local)
                    }
                    Instruction::$lltee(op, a, b, local) => (Instruction::$llset(op, a, b, local), local),
                    Instruction::$lctee(op, src, c, local) => (Instruction::$lcset(op, src, c, local), local),
                    $(Instruction::$loadtee(memarg, addr, local) => (Instruction::$loadset(memarg, addr, local), local.into()),)?
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
            if let Some([(lhs_idx, lhs_src), (rhs_idx, rhs_src), (op_idx, raw_op)]) =
                previous_non_nop::<3>($instrs, $read)
                && let Some((lhs_instr, lhs)) = $source($instrs, lhs_idx, lhs_src)
                && let Some(op) = $op(raw_op)
            {
                if let Some((rhs_instr, rhs)) = $source($instrs, rhs_idx, rhs_src) {
                    $instrs[lhs_idx] = lhs_instr;
                    $instrs[rhs_idx] = rhs_instr;
                    $instrs[op_idx] = Instruction::Nop;
                    $instrs[$read] = Instruction::$local_local(op, lhs, rhs, $dst);
                } else if let Some(imm) = $const(rhs_src, raw_op) {
                    $instrs[lhs_idx] = lhs_instr;
                    $instrs[rhs_idx] = Instruction::Nop;
                    $instrs[op_idx] = Instruction::Nop;
                    $instrs[$read] = $local_const($dst, lhs, op, imm);
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
            rewrite!($instrs, $read, [$get(src)] => if src == $dst { Instruction::Nop } else { Instruction::$copy(src, $dst) });
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
            rewrite!($instrs, $read, [$get(src)] if (src == $dst) => [Instruction::$get(src), Instruction::Nop]);
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
            rewrite!($instrs, $read, [$tee(local)] => [Instruction::$set(local), Instruction::Nop]);
            rewrite!($instrs, $read, [$lltee(op, a, b, dst)] => Instruction::$llset(op, a, b, dst));
            rewrite!($instrs, $read, [$lctee(op, src, c, dst)] => Instruction::$lcset(op, src, c, dst));
        }};
    }

    pub(crate) use {
        define_local_source_resolver, fold_local_binop, replace, rewrite, rewrite_drop_tee_direct,
        rewrite_local_set_direct, rewrite_local_tee_direct,
    };
}

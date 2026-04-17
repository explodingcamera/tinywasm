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

    pub(crate) use {replace, rewrite};
}

#[rustfmt::skip]
macro_rules! execute_instruction {
    ($executor:ident, $instr_ptr:ident, $acc32:ident, $acc64:ident, $acc128:ident, $body:expr) => {{
        #[allow(unreachable_code, unused_macros, clippy::redundant_closure_call)]
        let result = (|| -> ExecResult<ExecFlow> {
            macro_rules! read_operand {
                (u32) => { *$acc32 };
                (i32) => { *$acc32 as i32 };
                (f32) => { f32::from_bits(*$acc32) };
                (u64) => { *$acc64 };
                (i64) => { *$acc64 as i64 };
                (f64) => { f64::from_bits(*$acc64) };
                (ValueRef) => { ValueRef::from_raw(*$acc32) };
                (Value128) => { *$acc128 };
            }
            macro_rules! write_result {
                (local $ty:ident, $local:expr, $expr:expr) => {{
                    let value: $ty = $expr;
                    <$ty>::local_set(&mut $executor.value_stack, &$executor.cf, $local, value);
                }};
                (tee $ty:ident, $local:expr, $value:expr) => {{
                    let value: $ty = $value;
                    write_result!(local $ty, $local, value);
                    write_result!($ty, value);
                }};
                ($ty:ident, $expr:expr) => {{
                    let value: $ty = $expr;
                    write_result!(@acc $ty, value);
                }};
                (@acc u32, $value:ident) => { *$acc32 = $value };
                (@acc i32, $value:ident) => { *$acc32 = $value as u32 };
                (@acc f32, $value:ident) => { *$acc32 = $value.to_bits() };
                (@acc u64, $value:ident) => { *$acc64 = $value };
                (@acc i64, $value:ident) => { *$acc64 = $value as u64 };
                (@acc f64, $value:ident) => { *$acc64 = $value.to_bits() };
                (@acc ValueRef, $value:ident) => { *$acc32 = $value.raw() };
                (@acc Value128, $value:ident) => { *$acc128 = $value };
            }
            macro_rules! exec_op {
                ($ex:ident; unary $from:ident => $to:ident, |$v:ident| $expr:expr) => {{
                    let $v = read_operand!($from);
                    write_result!($to, $expr);
                }};
                ($ex:ident; binary $from:ident => $to:ident, |$lhs:ident, $rhs:ident| $expr:expr) => {{
                    let $rhs = read_operand!($from);
                    let $lhs = <$from>::stack_pop(&mut $ex.value_stack);
                    write_result!($to, $expr);
                }};
                ($ex:ident; mixed $lhs_ty:ident, $rhs_ty:ident => $to:ident, |$lhs:ident, $rhs:ident| $expr:expr) => {{
                    let $rhs = read_operand!($rhs_ty);
                    let $lhs = read_operand!($lhs_ty);
                    write_result!($to, $expr);
                }};
                ($ex:ident; ternary $from:ident => $to:ident, |$a:ident, $b:ident, $c:ident| $expr:expr) => {{
                    let $c = read_operand!($from);
                    let $b = <$from>::stack_pop(&mut $ex.value_stack);
                    let $a = <$from>::stack_pop(&mut $ex.value_stack);
                    write_result!($to, $expr);
                }};
            }
            macro_rules! exec_load {
                ($memory:ident, $ty:ty, $bytes:literal => 32, $extended:ty) => {{
                    *$acc32 = $executor.exec_mem_load::<$ty, $bytes>(*$memory, *$acc32, *$acc64)? as $extended as u32;
                    if $memory.address64 {
                        *$acc64 = 0;
                    }
                }};
                ($memory:ident, $ty:ty, $bytes:literal => 64, $extended:ty) => {{
                    *$acc64 = $executor.exec_mem_load::<$ty, $bytes>(*$memory, *$acc32, *$acc64)? as $extended as u64;
                    if !$memory.address64 {
                        *$acc32 = 0;
                    }
                }};
            }
            macro_rules! exec_store {
                ($memory:ident, $value:expr, $wide:literal, $bytes:literal) => {{
                    $executor.exec_mem_store::<_, $bytes>(
                        *$memory,
                        $value,
                        if $wide { ValueLane::S64 } else { ValueLane::S32 },
                        *$acc32,
                        *$acc64,
                    )?;
                    // Stores consume their value lane and, when different, their address lane.
                    if $wide || $memory.address64 {
                        *$acc64 = 0;
                    }
                    if !$wide || !$memory.address64 {
                        *$acc32 = 0;
                    }
                }};
            }
            macro_rules! dispatch_flow {
                ($flow:expr) => {{
                    // Advance accounting before a call or throw can fail or switch functions.
                    $executor.charge_instructions($instr_ptr + 1);
                    return Ok($flow);
                }};
            }
            macro_rules! exec_branch {
                ($target:expr) => {{
                    // Dead cached bits may still be reusable copies of frame locals.
                    if let Some(target) = $target {
                        return Ok(ExecFlow::Branch(target));
                    }
                }};
            }
            $body;
            Ok(ExecFlow::Continue($instr_ptr + 1))
        })();
        result
    }};
}

// This list is the single source for stable instruction stepping and nightly tail handlers.
// Explicit loop forms run their own fuel-checked loop, ordinary branches need no self-edge check.
#[rustfmt::skip]
macro_rules! instruction_handlers {
    ($emit:ident $(, $abi:literal)?) => {
        $emit! { $($abi;)? executor, instr_ptr, acc32, acc64, acc128;
            Spill32 => Value32::stack_push(&mut executor.value_stack, *acc32),
            LocalGet32Preserve(local) => { Value32::stack_push(&mut executor.value_stack, *acc32); *acc32 = Value32::local_get(&executor.value_stack, &executor.cf, *local); },
            LocalGet64Preserve(local) => { Value64::stack_push(&mut executor.value_stack, *acc64); *acc64 = Value64::local_get(&executor.value_stack, &executor.cf, *local); },
            Const32Preserve(value) => { Value32::stack_push(&mut executor.value_stack, *acc32); *acc32 = *value as u32; },
            Const64ImmPreserve(value) => { Value64::stack_push(&mut executor.value_stack, *acc64); *acc64 = *value as i64 as u64; },
            Const64Preserve(index) => { Value64::stack_push(&mut executor.value_stack, *acc64); *acc64 = index.resolve(&executor.func.data).value() as u64; },
            Spill64 => Value64::stack_push(&mut executor.value_stack, *acc64),
            Reload32 => *acc32 = Value32::stack_pop(&mut executor.value_stack),
            Reload64 => *acc64 = Value64::stack_pop(&mut executor.value_stack),
            Spill128 => Value128::stack_push(&mut executor.value_stack, *acc128),
            Reload128 => *acc128 = Value128::stack_pop(&mut executor.value_stack),
            LocalGet128Preserve(local) => { Value128::stack_push(&mut executor.value_stack, *acc128); *acc128 = Value128::local_get(&executor.value_stack, &executor.cf, *local); },
            Const128ImmPreserve(value) => { Value128::stack_push(&mut executor.value_stack, *acc128); *acc128 = Value128(u128::from(*value).to_le_bytes()); },
            Const128Preserve(index) => { Value128::stack_push(&mut executor.value_stack, *acc128); *acc128 = Value128(index.resolve(&executor.func.data).value()); },
            I32Add3 => {
                let middle = u32::stack_pop(&mut executor.value_stack);
                let left = u32::stack_pop(&mut executor.value_stack);
                *acc32 = left.wrapping_add(middle).wrapping_add(*acc32);
            },
            I64Add3 => {
                let middle = u64::stack_pop(&mut executor.value_stack);
                let left = u64::stack_pop(&mut executor.value_stack);
                *acc64 = left.wrapping_add(middle).wrapping_add(*acc64);
            },
            I32MulAddLocal(local) => executor.exec_mul_add_local(*local, *acc32, u32::wrapping_mul, u32::wrapping_add),
            I64MulAddLocal(local) => executor.exec_mul_add_local(*local, *acc64, u64::wrapping_mul, u64::wrapping_add),
            F32MulAddLocal(local) => executor.exec_mul_add_local(*local, f32::from_bits(*acc32), |a, b| a * b, |a, b| a + b),
            F64MulAddLocal(local) => executor.exec_mul_add_local(*local, f64::from_bits(*acc64), |a, b| a * b, |a, b| a + b),
            F32MulAddStore(memory) => executor.exec_mul_add_store::<f32, 4>(*memory, f32::from_bits(*acc32), *acc32, *acc64)?,
            F64MulAddStore(memory) => executor.exec_mul_add_store::<f64, 8>(*memory, f64::from_bits(*acc64), *acc32, *acc64)?,
            Checkpoint => return Ok(ExecFlow::Checkpoint(instr_ptr + 1)),
            Unreachable => return cold!(Err(Trap::Unreachable)),
            Drop32 => {},
            Drop64 => {},
            Drop128 => {},
            Select32 => {
                let condition = *acc32 != 0;
                let rhs = Value32::stack_pop(&mut executor.value_stack);
                let lhs = Value32::stack_pop(&mut executor.value_stack);
                *acc32 = if condition { lhs } else { rhs };
            },
            Select64 => {
                let lhs = Value64::stack_pop(&mut executor.value_stack);
                if *acc32 != 0 { *acc64 = lhs; }
            },
            Select128 => {
                let lhs = Value128::stack_pop(&mut executor.value_stack);
                if *acc32 != 0 { *acc128 = lhs; }
            },
            SelectStore32(arg) => {
                let false_value = Value32::stack_pop(&mut executor.value_stack);
                let true_value = Value32::stack_pop(&mut executor.value_stack);
                let value = if *acc32 == 0 { false_value } else { true_value };
                executor.exec_mem_store::<_, 4>(*arg, value, ValueLane::S32, *acc32, *acc64)?;
            },
            SelectStore64(index) => {
                let true_value = Value64::stack_pop(&mut executor.value_stack);
                let value = if *acc32 == 0 { *acc64 } else { true_value };
                let memory = executor.operand128(*index);
                executor.exec_mem_store_value::<_, 8>(executor.mem_addr(memory.memory()), memory.offset(), value)?;
            },
            SelectMulti(counts) => executor.value_stack.select_multi(*counts),
            Call(v) => dispatch_flow!(executor.exec_call_direct(*v, instr_ptr + 1, ScalarValues { acc32: *acc32, acc64: *acc64 })?),
            CallSelf => dispatch_flow!({ executor.exec_call_self(instr_ptr + 1, ScalarValues { acc32: *acc32, acc64: *acc64 })?; ExecFlow::Jump(0) }),
            CallIndirect(idx) => dispatch_flow!(executor.exec_call_indirect::<false>(*idx, instr_ptr + 1, ScalarValues { acc32: *acc32, acc64: *acc64 })?),
            CallRef(ty) => dispatch_flow!(executor.exec_call_ref::<false>(*ty, instr_ptr + 1, ScalarValues { acc32: *acc32, acc64: *acc64 })?),
            ReturnCall(v) => dispatch_flow!(executor.exec_return_call_direct(*v, ScalarValues { acc32: *acc32, acc64: *acc64 })?),
            ReturnCallSelf => dispatch_flow!({ executor.exec_return_call_self(ScalarValues { acc32: *acc32, acc64: *acc64 })?; ExecFlow::Jump(0) }),
            ReturnCallIndirect(idx) => dispatch_flow!(executor.exec_call_indirect::<true>(*idx, instr_ptr + 1, ScalarValues { acc32: *acc32, acc64: *acc64 })?),
            ReturnCallRef(ty) => dispatch_flow!(executor.exec_call_ref::<true>(*ty, instr_ptr + 1, ScalarValues { acc32: *acc32, acc64: *acc64 })?),
            Throw(tag) => dispatch_flow!(executor.exec_throw(*tag, instr_ptr)?),
            ThrowRef => dispatch_flow!(executor.exec_throw_ref(instr_ptr)?),
            Jump(ip) => exec_branch!(Some(*ip as usize)),
            JumpSpill32(ip) => {
                Value32::stack_push(&mut executor.value_stack, *acc32);
                exec_branch!(Some(*ip as usize));
            },
            JumpKeep32(ip) => return Ok(ExecFlow::Continue(*ip as usize + 1)),
            JumpSpill64(ip) => {
                Value64::stack_push(&mut executor.value_stack, *acc64);
                exec_branch!(Some(*ip as usize));
            },
            JumpKeep64(ip) => return Ok(ExecFlow::Continue(*ip as usize + 1)),
            JumpSpill128(ip) => {
                Value128::stack_push(&mut executor.value_stack, *acc128);
                exec_branch!(Some(*ip as usize));
            },
            JumpKeep128(ip) => return Ok(ExecFlow::Continue(*ip as usize + 1)),
            JumpIfZero32(ip) => exec_branch!((*acc32 == 0).then_some(*ip as usize)),
            JumpIfNonZero32(ip) => exec_branch!((*acc32 != 0).then_some(*ip as usize)),
            JumpIfZero64(ip) => exec_branch!((*acc64 == 0).then_some(*ip as usize)),
            JumpIfNonZero64(ip) => exec_branch!((*acc64 != 0).then_some(*ip as usize)),
            BinOpLocalConstTestJump32(packed) => {
                let arg = executor.operand128(packed.index);
                let value = executor.exec_binop_local_const::<Value32>(packed.op, arg.local(), arg.value() as u32);
                exec_branch!(((value == 0) == arg.on_zero()).then_some(arg.target() as usize));
            },
            BinOpLocalLocalTestJump32(packed) => {
                let arg = executor.operand64(packed.index);
                let value = executor.exec_binop_local_local::<Value32>(packed.op.0, arg.b(), arg.c());
                exec_branch!(((value == 0) == packed.op.1).then_some(arg.target() as usize));
            },
            BinOpLocalConstJumpCmpConst32(packed) => {
                let arg = executor.operand128(packed.index);
                let value = executor.exec_binop_local_const::<Value32>(packed.op.0, arg.local(), arg.value());
                exec_branch!(packed.op.1.cmp(value as i32, arg.right()).then_some(arg.target() as usize));
            },
            JumpIfRefNull(ip) => exec_branch!(executor.exec_jump_if_ref::<true>().then_some(*ip as usize)),
            JumpIfRefNonNull(ip) => exec_branch!(executor.exec_jump_if_ref::<false>().then_some(*ip as usize)),
            BrOnCast(idx) => exec_branch!(executor.exec_br_on_cast::<false>(*idx)),
            BrOnCastFail(idx) => exec_branch!(executor.exec_br_on_cast::<true>(*idx)),
            JumpIfLocalZero32(arg) => exec_branch!(executor.exec_jump_if_local::<Value32, true>(arg.local).then_some(arg.target_ip as usize)),
            JumpIfLocalNonZero32(arg) => exec_branch!(executor.exec_jump_if_local::<Value32, false>(arg.local).then_some(arg.target_ip as usize)),
            JumpIfLocalZero64(arg) => exec_branch!(executor.exec_jump_if_local::<Value64, true>(arg.local).then_some(arg.target_ip as usize)),
            JumpIfLocalNonZero64(arg) => exec_branch!(executor.exec_jump_if_local::<Value64, false>(arg.local).then_some(arg.target_ip as usize)),
            JumpCmpAccConst32(packed) => exec_branch!(executor.exec_jump_cmp_acc_const32(*packed, *acc32 as i32)),
            JumpCmpAccConst64(packed) => exec_branch!(executor.exec_jump_cmp_acc_const64(*packed, *acc64 as i64)),
            JumpFCmpAccConst32(packed) => exec_branch!(executor.exec_jump_fcmp_acc_const32(*packed, f32::from_bits(*acc32))),
            JumpFCmpAccConst64(packed) => exec_branch!(executor.exec_jump_fcmp_acc_const64(*packed, f64::from_bits(*acc64))),
            JumpCmpAccLocal32(packed) => exec_branch!(executor.exec_jump_cmp_acc_local32(*packed, *acc32 as i32)),
            JumpCmpAccLocal64(packed) => exec_branch!(executor.exec_jump_cmp_acc_local64(*packed, *acc64 as i64)),
            BinOpLocalConstSetJump32(packed) => exec_branch!(executor.exec_binop_local_const_set_jump(*packed)),
            BinOpLocalConstSetJumpCmpLocal32(packed) => exec_branch!(executor.exec_binop_local_const_set_jump_cmp_local(*packed)),
            BinOpLocalConstSetJumpCmpLocal64(packed) => exec_branch!(executor.exec_binop_local_const_set_jump_cmp_local64(*packed)),
            BinOpLocalLocalSetJumpCmpLocal32(packed) => exec_branch!(executor.exec_binop_local_local_set_jump_cmp_local32(*packed)),
            BinOpLocalLocalSetJumpCmpLocal64(packed) => exec_branch!(executor.exec_binop_local_local_set_jump_cmp_local64(*packed)),
            BinOpStackConstTeeLocalJump32(packed) => exec_branch!(executor.exec_binop_stack_const_tee_local_jump(*packed)),
            BinOpGlobalConstSetJump32(packed) => exec_branch!(executor.exec_binop_global_const_set_jump(*packed)),
            I32AddLocalSetConstJump(idx) => exec_branch!(executor.exec_i32_add_local_set_const_jump(*idx)),
            I32AddStackConstTeeLocalJump(idx) => exec_branch!(executor.exec_i32_add_stack_const_tee_local_jump(*idx)),
            I32AddGlobalSetConstJump(idx) => exec_branch!(executor.exec_i32_add_global_set_const_jump(*idx)),
            I32AddLocalSetConstJumpCmpLocal(packed) => exec_branch!(executor.exec_i32_add_local_set_const_jump_cmp_local(*packed)),
            I32AddLocalSetConstLoop(idx) => return Ok(executor.exec_i32_add_local_set_const_loop(*idx, instr_ptr)),
            I32AddStackConstTeeLocalLoop(idx) => return Ok(executor.exec_i32_add_stack_const_tee_local_loop(*idx, instr_ptr)),
            I32AddGlobalSetConstLoop(idx) => return Ok(executor.exec_i32_add_global_set_const_loop(*idx, instr_ptr)),
            JumpCmpLocalConst32(packed) => exec_branch!(executor.exec_jump_cmp_local_const32(*packed)),
            JumpCmpLocalConst64(packed) => exec_branch!(executor.exec_jump_cmp_local_const64(*packed)),
            JumpCmpLocalLocal32(packed) => exec_branch!(executor.exec_jump_cmp_local_local32(*packed)),
            JumpCmpLocalLocal64(packed) => exec_branch!(executor.exec_jump_cmp_local_local64(*packed)),
            DropKeep32 { base, keep } => executor.value_stack.stack_32.truncate_keep((executor.cf.stack_base().s32 + u32::from(*base)) as usize, *keep as usize),
            DropKeep64 { base, keep } => executor.value_stack.stack_64.truncate_keep((executor.cf.stack_base().s64 + u32::from(*base)) as usize, *keep as usize),
            DropKeep128 { base, keep } => executor.value_stack.stack_128.truncate_keep((executor.cf.stack_base().s128 + u32::from(*base)) as usize, *keep as usize),
            BranchTable(idx) => exec_branch!(Some(executor.exec_branch_table(*idx, *acc32))),
            Return => dispatch_flow!(executor.exec_return(*acc32, *acc64)),
            ReturnVoid => dispatch_flow!(executor.exec_return_void()),
            Return32 => dispatch_flow!(executor.exec_return_32(*acc32)),
            Return64 => dispatch_flow!(executor.exec_return_64(*acc64)),
            Return128 => dispatch_flow!(executor.exec_return_128()),
            LocalGet32(local_index) => *acc32 = Value32::local_get(&executor.value_stack, &executor.cf, *local_index),
            LocalCopyPair32(first, target, second, destination) => {
                Value32::local_copy(&mut executor.value_stack, &executor.cf, u16::from(*first), u16::from(*target));
                Value32::local_copy(&mut executor.value_stack, &executor.cf, u16::from(*second), u16::from(*destination));
            },
            LocalCopyPair64(first, target, second, destination) => {
                Value64::local_copy(&mut executor.value_stack, &executor.cf, u16::from(*first), u16::from(*target));
                Value64::local_copy(&mut executor.value_stack, &executor.cf, u16::from(*second), u16::from(*destination));
            },
            LocalGet64(local_index) => *acc64 = Value64::local_get(&executor.value_stack, &executor.cf, *local_index),
            LocalGet128(local_index) => *acc128 = Value128::local_get(&executor.value_stack, &executor.cf, *local_index),
            LocalSet32(local_index) => Value32::local_set(&mut executor.value_stack, &executor.cf, *local_index, *acc32),
            LocalSet64(local_index) => Value64::local_set(&mut executor.value_stack, &executor.cf, *local_index, *acc64),
            LocalSet128(local_index) => Value128::local_set(&mut executor.value_stack, &executor.cf, *local_index, *acc128),
            LocalCopy32(from, to) => Value32::local_copy(&mut executor.value_stack, &executor.cf, *from, *to),
            LocalCopy64(from, to) => Value64::local_copy(&mut executor.value_stack, &executor.cf, *from, *to),
            LocalCopy128(from, to) => Value128::local_copy(&mut executor.value_stack, &executor.cf, *from, *to),
            I32AddConst(c) => *acc32 = acc32.wrapping_add(*c as u32),
            I32AddLocal(local) => *acc32 = acc32.wrapping_add(Value32::local_get(&executor.value_stack, &executor.cf, *local)),
            I32AddLocalShl(local, shift) => *acc32 = acc32.wrapping_add(
                Value32::local_get(&executor.value_stack, &executor.cf, *local).wrapping_shl(u32::from(*shift))
            ),
            I32AddLocalAndShl(packed) => {
                let arg = executor.operand64(packed.index);
                let value = Value32::local_get(&executor.value_stack, &executor.cf, arg.a()) & arg.b();
                *acc32 = acc32.wrapping_add(value.wrapping_shl(u32::from(packed.op)));
            },
            I64AddLocalAndShl(packed) => {
                let arg = executor.operand128(packed.index);
                let value = Value64::local_get(&executor.value_stack, &executor.cf, arg.a()) & arg.b();
                *acc64 = acc64.wrapping_add(value.wrapping_shl(u32::from(packed.op)));
            },
            I32AddLocalConst(arg) => {
                let value = Value32::local_get(&executor.value_stack, &executor.cf, arg.local);
                *acc32 = value.wrapping_add(arg.value as u32);
            },
            BinOpLocal32(op, local) => *acc32 = op.exec(*acc32, Value32::local_get(&executor.value_stack, &executor.cf, *local)),
            BinOpLocal64(op, local) => *acc64 = op.exec(*acc64, Value64::local_get(&executor.value_stack, &executor.cf, *local)),
            BinOpConst32(op, value) => *acc32 = op.exec(*acc32, *value as u32),
            BinOpConst64(packed) => *acc64 = packed.op.exec(*acc64, packed.index.resolve(&executor.func.data).value() as u64),
            BinOpLocalLocal32(op, left, right) => *acc32 = executor.exec_binop_local_local::<Value32>(*op, *left, *right),
            I32XorRotlConst(shift) => exec_op!(executor; binary u32 => u32, |a, b| (a ^ b).rotate_left(u32::from(*shift))),
            I32XorLocalRotlConst(local, shift) => {
                let rhs = Value32::local_get(&executor.value_stack, &executor.cf, *local);
                *acc32 = (*acc32 ^ rhs).rotate_left(u32::from(*shift));
            },
            I32XorLocalLocalRotlConst(left, right, shift) => {
                let lhs = Value32::local_get(&executor.value_stack, &executor.cf, *left);
                let rhs = Value32::local_get(&executor.value_stack, &executor.cf, *right);
                *acc32 = (lhs ^ rhs).rotate_left(u32::from(*shift));
            },
            BinOpLocalLocal64(op, left, right) => *acc64 = executor.exec_binop_local_local::<Value64>(*op, *left, *right),
            I64XorRotlConst(shift) => exec_op!(executor; binary u64 => u64, |a, b| (a ^ b).rotate_left(u32::from(*shift))),
            I64XorLocalRotlConst(local, shift) => {
                let rhs = Value64::local_get(&executor.value_stack, &executor.cf, *local);
                *acc64 = (*acc64 ^ rhs).rotate_left(u32::from(*shift));
            },
            I64XorLocalLocalRotlConst(left, right, shift) => {
                let lhs = Value64::local_get(&executor.value_stack, &executor.cf, *left);
                let rhs = Value64::local_get(&executor.value_stack, &executor.cf, *right);
                *acc64 = (lhs ^ rhs).rotate_left(u32::from(*shift));
            },
            BinOpLocalConstCompact32(arg) => {
                let value = Value32::local_get(&executor.value_stack, &executor.cf, u16::from(arg.local));
                *acc32 = arg.op.exec(value, arg.value);
            },
            BinOpLocalConstRhs32(packed) => {
                let arg = executor.operand64(packed.index);
                let rhs = executor.exec_binop_local_const::<Value32>(packed.op.1, arg.a(), arg.b());
                *acc32 = packed.op.0.exec(*acc32, rhs);
            },
            BinOpLocalConstRhs64(packed) => {
                let arg = executor.operand128(packed.index);
                let rhs = executor.exec_binop_local_const::<Value64>(packed.op.1, arg.a(), arg.b());
                *acc64 = packed.op.0.exec(*acc64, rhs);
            },
            BinOpLocalLocalRhs32(outer, inner, left, right) => {
                let rhs = executor.exec_binop_local_local::<Value32>(*inner, *left, *right);
                *acc32 = outer.exec(*acc32, rhs);
            },
            BinOpLocalLocalRhs64(outer, inner, left, right) => {
                let lhs = Value64::local_get(&executor.value_stack, &executor.cf, *left);
                let rhs = Value64::local_get(&executor.value_stack, &executor.cf, *right);
                *acc64 = match (*outer, *inner) {
                    (BinOp::IAdd, BinOp::IAdd) => acc64.wrapping_add(lhs.wrapping_add(rhs)),
                    (BinOp::IXor, BinOp::IXor) => *acc64 ^ (lhs ^ rhs),
                    _ => outer.exec(*acc64, inner.exec(lhs, rhs)),
                };
            },
            BinOpLocalConst32(packed) => { let arg = executor.operand64(packed.index); *acc32 = executor.exec_binop_local_const::<Value32>(packed.op, arg.a(), arg.b()); },
            BinOpLocalConst64(packed) => { let arg = executor.operand128(packed.index); *acc64 = executor.exec_binop_local_const::<Value64>(packed.op, arg.a(), arg.b()); },
            BinOpLocalSet32(op, local, dst) => write_result!(local u32, *dst, op.exec(*acc32, Value32::local_get(&executor.value_stack, &executor.cf, *local))),
            BinOpLocalSet64(op, local, dst) => write_result!(local u64, *dst, op.exec(*acc64, Value64::local_get(&executor.value_stack, &executor.cf, *local))),
            BinOpLocalTee32(op, local, dst) => write_result!(tee u32, *dst, op.exec(*acc32, Value32::local_get(&executor.value_stack, &executor.cf, *local))),
            BinOpLocalTee64(op, local, dst) => write_result!(tee u64, *dst, op.exec(*acc64, Value64::local_get(&executor.value_stack, &executor.cf, *local))),
            BinOpLocalLocalSet32(packed) => { let arg = executor.operand64(packed.index); write_result!(local u32, arg.c(), executor.exec_binop_local_local::<Value32>(packed.op, arg.a(), arg.b())); },
            BinOpLocalLocalSet64(packed) => { let arg = executor.operand64(packed.index); write_result!(local u64, arg.c(), executor.exec_binop_local_local::<Value64>(packed.op, arg.a(), arg.b())); },
            BinOpLocalLocalTee32(packed) => { let arg = executor.operand64(packed.index); write_result!(tee u32, arg.c(), executor.exec_binop_local_local::<Value32>(packed.op, arg.a(), arg.b())); },
            BinOpLocalLocalTee64(packed) => { let arg = executor.operand64(packed.index); write_result!(tee u64, arg.c(), executor.exec_binop_local_local::<Value64>(packed.op, arg.a(), arg.b())); },
            BinOpLocalConstSet32(packed) => { let arg = executor.operand64(packed.index); write_result!(local u32, arg.b(), executor.exec_binop_local_const::<Value32>(packed.op, arg.a(), arg.c())); },
            BinOpLocalConstSet64(packed) => { let arg = executor.operand128(packed.index); write_result!(local u64, arg.b(), executor.exec_binop_local_const::<Value64>(packed.op, arg.a(), arg.c())); },
            BinOpLocalConstTee32(packed) => { let arg = executor.operand64(packed.index); write_result!(tee u32, arg.b(), executor.exec_binop_local_const::<Value32>(packed.op, arg.a(), arg.c())); },
            BinOpLocalConstTee64(packed) => { let arg = executor.operand128(packed.index); write_result!(tee u64, arg.b(), executor.exec_binop_local_const::<Value64>(packed.op, arg.a(), arg.c())); },
            I64x2RotlConst(shift) => *acc128 = acc128.i64x2_rotl(u32::from(*shift)),
            I32AndConst(c) => *acc32 &= *c as u32,
            I32XorConst(c) => *acc32 ^= *c as u32,
            I32ShlConst(c) => *acc32 = acc32.wrapping_shl(*c as u32),
            I32ShrUConst(c) => *acc32 = acc32.wrapping_shr(*c as u32),
            I64AddConst(idx) => *acc64 = acc64.wrapping_add(idx.resolve(&executor.func.data).value() as u64),
            I32AddLocalSetConst(arg) => { i32::local_update(&mut executor.value_stack, &executor.cf, arg.local, |value| value.wrapping_add(arg.value)); },
            I64AddLocalSetConst(packed) => { let rhs = packed.index.resolve(&executor.func.data).value(); i64::local_update(&mut executor.value_stack, &executor.cf, packed.op, |value| value.wrapping_add(rhs)); },
            CmpLocalLocal32(op, a, b) => *acc32 = executor.exec_cmp_local_local::<i32>(*a, *b, *op) as u32,
            CmpLocalLocal64(op, a, b) => *acc32 = executor.exec_cmp_local_local::<i64>(*a, *b, *op) as u32,
            SetLocalConst32(arg) => i32::local_set(&mut executor.value_stack, &executor.cf, arg.local, arg.value),
            SetLocalConst64(packed) => { let v = packed.index.resolve(&executor.func.data).value(); i64::local_set(&mut executor.value_stack, &executor.cf, packed.op, v); },
            SetLocalConst128(packed) => Value128::local_set(&mut executor.value_stack, &executor.cf, packed.op, Value128(packed.index.resolve(&executor.func.data).value())),
            IncMemoryLocal32(arg) => executor.exec_inc_memory_local::<i32, 4>(arg.memory_arg_idx, arg.local1, #[inline(always)] |v| v.wrapping_add(1))?,
            UpdateMemoryLocal32(packed) => {
                let arg = executor.operand128(packed.index);
                executor.exec_update_memory_local32(arg, packed.op)?;
            },
            IncMemoryLocal64(arg) => executor.exec_inc_memory_local::<i64, 8>(arg.memory_arg_idx, arg.local1, #[inline(always)] |v| v.wrapping_add(1))?,
            I32StoreLocalLocal(arg) => executor.exec_store_local_value::<_, 4>(arg.memory_arg_idx, arg.local1, Value32::local_get(&executor.value_stack, &executor.cf, u16::from(arg.local2)))?,
            I64StoreLocalLocal(arg) => executor.exec_store_local_value::<_, 8>(arg.memory_arg_idx, arg.local1, Value64::local_get(&executor.value_stack, &executor.cf, u16::from(arg.local2)))?,
            I32StoreLocalCompact(offset, address, value) => executor.exec_store_local_compact::<u32, 4>(*offset, *address, *value)?,
            I64StoreLocalCompact(offset, address, value) => executor.exec_store_local_compact::<u64, 8>(*offset, *address, *value)?,
            V128StoreLocalLocal(arg) => executor.exec_store_local_value::<_, 16>(arg.memory_arg_idx, arg.local1, Value128::local_get(&executor.value_stack, &executor.cf, u16::from(arg.local2)))?,
            I32Store8LocalLocal(arg) => executor.exec_store_local_value::<_, 1>(arg.memory_arg_idx, arg.local1, Value32::local_get(&executor.value_stack, &executor.cf, u16::from(arg.local2)) as u8)?,
            I32Store16LocalLocal(arg) => executor.exec_store_local_value::<_, 2>(arg.memory_arg_idx, arg.local1, Value32::local_get(&executor.value_stack, &executor.cf, u16::from(arg.local2)) as u16)?,
            I64Store8LocalLocal(arg) => executor.exec_store_local_value::<_, 1>(arg.memory_arg_idx, arg.local1, Value64::local_get(&executor.value_stack, &executor.cf, u16::from(arg.local2)) as u8)?,
            I64Store16LocalLocal(arg) => executor.exec_store_local_value::<_, 2>(arg.memory_arg_idx, arg.local1, Value64::local_get(&executor.value_stack, &executor.cf, u16::from(arg.local2)) as u16)?,
            I64Store32LocalLocal(arg) => executor.exec_store_local_value::<_, 4>(arg.memory_arg_idx, arg.local1, Value64::local_get(&executor.value_stack, &executor.cf, u16::from(arg.local2)) as u32)?,
            I32LoadLocal(arg) => *acc32 = executor.exec_load_local_value::<u32, 4>(arg.memory_arg_idx, arg.local1)?,
            I32LoadLocalCompact(arg) => *acc32 = executor.exec_i32_load_local_compact(arg.local, arg.offset)?,
            LoadLocalBinOp32(packed) => {
                let arg = executor.operand64(packed.index);
                let index = Operand64Idx::new(arg.c());
                let value = executor.exec_load_local_kind32(packed.op.0, index, arg.a())?;
                let rhs = Value32::local_get(&executor.value_stack, &executor.cf, arg.b());
                *acc32 = packed.op.1.exec(value, rhs);
            },
            BinOpLoadLocalRhs32(packed) => {
                let arg = executor.operand64(packed.index);
                let index = Operand64Idx::new(arg.c());
                let rhs = executor.exec_load_local_kind32(packed.op.0, index, arg.a())?;
                *acc32 = packed.op.1.exec(*acc32, rhs);
            },
            I64LoadLocal(arg) => *acc64 = executor.exec_load_local_value::<u64, 8>(arg.memory_arg_idx, arg.local1)?,
            I64LoadLocalTee(arg) => write_result!(tee u64, u16::from(arg.local2), executor.exec_load_local_value::<u64, 8>(arg.memory_arg_idx, arg.local1)?),
            I64LoadLocalSet(arg) => write_result!(local u64, u16::from(arg.local2), executor.exec_load_local_value::<u64, 8>(arg.memory_arg_idx, arg.local1)?),
            I32LoadLocalAdd(arg) => *acc32 = executor.exec_load_local_add::<u32, 4>(*arg)?,
            I64LoadLocalAdd(arg) => *acc64 = executor.exec_load_local_add::<u64, 8>(*arg)?,
            I32Load8SLocalAdd(arg) => *acc32 = executor.exec_load_local_add::<i8, 1>(*arg)? as i32 as u32,
            I32Load8ULocalAdd(arg) => *acc32 = u32::from(executor.exec_load_local_add::<u8, 1>(*arg)?),
            I32Load8ULocalAddCompact(offset, left, right) => {
                let left = Value32::local_get(&executor.value_stack, &executor.cf, u16::from(*left));
                let right = Value32::local_get(&executor.value_stack, &executor.cf, u16::from(*right));
                let base = left.wrapping_add(right) as usize;
                let address = executor.mem_addr(0);
                *acc32 = crate::store::with_memory!(executor.state, address, |mem, _kind| {
                    let addr = cold_err!(mem.effective_addr::<1>(base, u64::from(*offset)))?;
                    Ok(u32::from(cold_err!(u8::load_at(mem, addr))?))
                })?;
            },
            I32Load16SLocalAdd(arg) => *acc32 = executor.exec_load_local_add::<i16, 2>(*arg)? as i32 as u32,
            CopyByteLocalAdd(dst, src, index) => {
                let index = Value32::local_get(&executor.value_stack, &executor.cf, u16::from(*index));
                let dst = Value32::local_get(&executor.value_stack, &executor.cf, u16::from(*dst)).wrapping_add(index);
                let src = Value32::local_get(&executor.value_stack, &executor.cf, u16::from(*src)).wrapping_add(index);
                let address = executor.mem_addr(0);
                let byte = crate::store::with_memory!(executor.state, address, |mem, _kind| {
                    cold_err!(u8::load_at(mem, src as usize))
                })?;
                crate::store::with_memory!(executor.state, address, |mem, _kind| {
                    cold_err!(byte.store_at(mem, dst as usize))
                })?;
            },
            I32Load16ULocalAdd(arg) => *acc32 = u32::from(executor.exec_load_local_add::<u16, 2>(*arg)?),
            F64AddLoadLocal(arg) => {
                let rhs = executor.exec_load_local_value::<u64, 8>(arg.memory_arg_idx, arg.local1)?;
                *acc64 = (f64::from_bits(*acc64) + f64::from_bits(rhs)).to_bits();
            },
            F64SubLoadLocal(arg) => {
                let rhs = executor.exec_load_local_value::<u64, 8>(arg.memory_arg_idx, arg.local1)?;
                *acc64 = (f64::from_bits(*acc64) - f64::from_bits(rhs)).to_bits();
            },
            I32Load8SLocal(arg) => *acc32 = executor.exec_load_local_value::<i8, 1>(arg.memory_arg_idx, arg.local1)? as i32 as u32,
            I32Load8ULocal(arg) => *acc32 = u32::from(executor.exec_load_local_value::<u8, 1>(arg.memory_arg_idx, arg.local1)?),
            I32Load16SLocal(arg) => *acc32 = executor.exec_load_local_value::<i16, 2>(arg.memory_arg_idx, arg.local1)? as i32 as u32,
            I32Load16ULocal(arg) => *acc32 = u32::from(executor.exec_load_local_value::<u16, 2>(arg.memory_arg_idx, arg.local1)?),
            I32LoadLocalTee(arg) => write_result!(tee u32, u16::from(arg.local2), executor.exec_load_local_value::<u32, 4>(arg.memory_arg_idx, arg.local1)?),
            I32LoadLocalTeeJump(packed) => {
                // Lowering materializes live values before this branch.
                *acc32 = 0;
                *acc64 = 0;
                let operand = executor.operand128(packed.index);
                let arg = operand.memory();
                let value = executor.exec_load_local_value::<u32, 4>(arg.memory_arg_idx, arg.local1)?;
                u32::local_set(&mut executor.value_stack, &executor.cf, u16::from(arg.local2), value);
                if (value == 0) == packed.op { return Ok(ExecFlow::Branch(operand.target() as usize)); }
            },
            I32LoadLocalSet(arg) => write_result!(local u32, u16::from(arg.local2), executor.exec_load_local_value::<u32, 4>(arg.memory_arg_idx, arg.local1)?),
            I32Load8SLocalTee(arg) => write_result!(tee i32, u16::from(arg.local2), i32::from(executor.exec_load_local_value::<i8, 1>(arg.memory_arg_idx, arg.local1)?)),
            I32Load8ULocalTee(arg) => write_result!(tee u32, u16::from(arg.local2), u32::from(executor.exec_load_local_value::<u8, 1>(arg.memory_arg_idx, arg.local1)?)),
            I32Load16SLocalTee(arg) => write_result!(tee i32, u16::from(arg.local2), i32::from(executor.exec_load_local_value::<i16, 2>(arg.memory_arg_idx, arg.local1)?)),
            I32Load16ULocalTee(arg) => write_result!(tee u32, u16::from(arg.local2), u32::from(executor.exec_load_local_value::<u16, 2>(arg.memory_arg_idx, arg.local1)?)),
            I32Load8SLocalSet(arg) => write_result!(local i32, u16::from(arg.local2), i32::from(executor.exec_load_local_value::<i8, 1>(arg.memory_arg_idx, arg.local1)?)),
            I32Load8ULocalSet(arg) => write_result!(local u32, u16::from(arg.local2), u32::from(executor.exec_load_local_value::<u8, 1>(arg.memory_arg_idx, arg.local1)?)),
            I32Load16SLocalSet(arg) => write_result!(local i32, u16::from(arg.local2), i32::from(executor.exec_load_local_value::<i16, 2>(arg.memory_arg_idx, arg.local1)?)),
            I32Load16ULocalSet(arg) => write_result!(local u32, u16::from(arg.local2), u32::from(executor.exec_load_local_value::<u16, 2>(arg.memory_arg_idx, arg.local1)?)),
            V128LoadLocalTee(arg) => write_result!(tee Value128, u16::from(arg.local2), executor.exec_load_local_value::<Value128, 16>(arg.memory_arg_idx, arg.local1)?),
            V128LoadLocalSet(arg) => write_result!(local Value128, u16::from(arg.local2), executor.exec_load_local_value::<Value128, 16>(arg.memory_arg_idx, arg.local1)?),
            I32AndConstTee(arg) => write_result!(tee u32, arg.local, *acc32 & arg.value as u32),
            I32AddConstTee(arg) => write_result!(tee u32, arg.local, acc32.wrapping_add(arg.value as u32)),
            I64AndConstTee(packed) => write_result!(tee u64, packed.op, *acc64 & packed.index.resolve(&executor.func.data).value() as u64),
            I64AddConstTee(packed) => write_result!(tee u64, packed.op, acc64.wrapping_add(packed.index.resolve(&executor.func.data).value() as u64)),
            LocalTee32(local_index) => Value32::local_set(&mut executor.value_stack, &executor.cf, *local_index, *acc32),
            LocalTee64(local_index) => Value64::local_set(&mut executor.value_stack, &executor.cf, *local_index, *acc64),
            LocalTee128(local_index) => Value128::local_set(&mut executor.value_stack, &executor.cf, *local_index, *acc128),
            GlobalGet32(global_index) => *acc32 = Value32::global_get(&executor.state.globals, executor.module.resolve_global_addr(*global_index)),
            GlobalGet64(global_index) => *acc64 = Value64::global_get(&executor.state.globals, executor.module.resolve_global_addr(*global_index)),
            GlobalGet128(global_index) => executor.exec_global_get::<Value128>(*global_index),
            GlobalSet32(global_index) => Value32::global_set(&mut executor.state.globals, executor.module.resolve_global_addr(*global_index), *acc32),
            GlobalSet64(global_index) => Value64::global_set(&mut executor.state.globals, executor.module.resolve_global_addr(*global_index), *acc64),
            GlobalSet128(global_index) => executor.exec_global_set::<Value128>(*global_index),
            GlobalTee32(global_index) => Value32::global_set(&mut executor.state.globals, executor.module.resolve_global_addr(*global_index), *acc32),
            GlobalTee64(global_index) => Value64::global_set(&mut executor.state.globals, executor.module.resolve_global_addr(*global_index), *acc64),
            GlobalTee128(global_index) => executor.exec_global_tee::<Value128>(*global_index),
            Const32(val) => *acc32 = *val as u32,
            Const64Imm(val) => *acc64 = i64::from(*val) as u64,
            Const64(idx) => *acc64 = idx.resolve(&executor.func.data).value() as u64,
            Const128Imm(val) => *acc128 = Value128(u128::from(*val).to_le_bytes()),
            I64Eqz => exec_op!(executor; unary i64 => i32, |v| i32::from(v == 0)),
            I32Eqz => exec_op!(executor; unary i32 => i32, |v| i32::from(v == 0)),
            I32Eq => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a == b)),
            I64Eq => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a == b)),
            F32Eq => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a == b)),
            F64Eq => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a == b)),
            I32Ne => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a != b)),
            I64Ne => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a != b)),
            F32Ne => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a != b)),
            F64Ne => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a != b)),
            I32LtS => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a < b)),
            I64LtS => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a < b)),
            I32LtU => exec_op!(executor; binary u32 => i32, |a, b| i32::from(a < b)),
            I64LtU => exec_op!(executor; binary u64 => i32, |a, b| i32::from(a < b)),
            F32Lt => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a < b)),
            F64Lt => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a < b)),
            I32LeS => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a <= b)),
            I64LeS => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a <= b)),
            I32LeU => exec_op!(executor; binary u32 => i32, |a, b| i32::from(a <= b)),
            I64LeU => exec_op!(executor; binary u64 => i32, |a, b| i32::from(a <= b)),
            F32Le => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a <= b)),
            F64Le => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a <= b)),
            I32GeS => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a >= b)),
            I64GeS => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a >= b)),
            I32GeU => exec_op!(executor; binary u32 => i32, |a, b| i32::from(a >= b)),
            I64GeU => exec_op!(executor; binary u64 => i32, |a, b| i32::from(a >= b)),
            F32Ge => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a >= b)),
            F64Ge => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a >= b)),
            I32GtS => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a > b)),
            I64GtS => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a > b)),
            I32GtU => exec_op!(executor; binary u32 => i32, |a, b| i32::from(a > b)),
            I64GtU => exec_op!(executor; binary u64 => i32, |a, b| i32::from(a > b)),
            F32Gt => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a > b)),
            F64Gt => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a > b)),
            I32Add => exec_op!(executor; binary i32 => i32, |a, b| a.wrapping_add(b)),
            I64Add => exec_op!(executor; binary i64 => i64, |a, b| a.wrapping_add(b)),
            F32Add => exec_op!(executor; binary f32 => f32, |a, b| a + b),
            F64Add => exec_op!(executor; binary f64 => f64, |a, b| a + b),
            I32Sub => exec_op!(executor; binary i32 => i32, |a, b| a.wrapping_sub(b)),
            I64Sub => exec_op!(executor; binary i64 => i64, |a, b| a.wrapping_sub(b)),
            F32Sub => exec_op!(executor; binary f32 => f32, |a, b| a - b),
            F64Sub => exec_op!(executor; binary f64 => f64, |a, b| a - b),
            F32Div => exec_op!(executor; binary f32 => f32, |a, b| a / b),
            F64Div => exec_op!(executor; binary f64 => f64, |a, b| a / b),
            I32Mul => exec_op!(executor; binary i32 => i32, |a, b| a.wrapping_mul(b)),
            I64Mul => exec_op!(executor; binary i64 => i64, |a, b| a.wrapping_mul(b)),
            F32Mul => exec_op!(executor; binary f32 => f32, |a, b| a * b),
            F64Mul => exec_op!(executor; binary f64 => f64, |a, b| a * b),
            I32DivS => exec_op!(executor; binary i32 => i32, |a, b| num::i32::div(a, b)?),
            I64DivS => exec_op!(executor; binary i64 => i64, |a, b| num::i64::div(a, b)?),
            I32DivU => exec_op!(executor; binary u32 => u32, |a, b| num::u32::div(a, b)?),
            I64DivU => exec_op!(executor; binary u64 => u64, |a, b| num::u64::div(a, b)?),
            I32RemS => exec_op!(executor; binary i32 => i32, |a, b| num::i32::rem(a, b)?),
            I64RemS => exec_op!(executor; binary i64 => i64, |a, b| num::i64::rem(a, b)?),
            I32RemU => exec_op!(executor; binary u32 => u32, |a, b| num::u32::rem(a, b)?),
            I64RemU => exec_op!(executor; binary u64 => u64, |a, b| num::u64::rem(a, b)?),
            I32And => exec_op!(executor; binary i32 => i32, |a, b| a & b),
            I64And => exec_op!(executor; binary i64 => i64, |a, b| a & b),
            I32Or => exec_op!(executor; binary i32 => i32, |a, b| a | b),
            I64Or => exec_op!(executor; binary i64 => i64, |a, b| a | b),
            I32Xor => exec_op!(executor; binary i32 => i32, |a, b| a ^ b),
            I64Xor => exec_op!(executor; binary i64 => i64, |a, b| a ^ b),
            I32Shl => exec_op!(executor; binary i32 => i32, |a, b| a.wrapping_shl(b as u32)),
            I64Shl => exec_op!(executor; binary i64 => i64, |a, b| a.wrapping_shl(b as u32)),
            I32ShrS => exec_op!(executor; binary i32 => i32, |a, b| a.wrapping_shr(b as u32)),
            I64ShrS => exec_op!(executor; binary i64 => i64, |a, b| a.wrapping_shr(b as u32)),
            I32ShrU => exec_op!(executor; binary u32 => u32, |a, b| a.wrapping_shr(b)),
            I64ShrU => exec_op!(executor; binary u64 => u64, |a, b| a.wrapping_shr(b as u32)),
            I32Rotl => exec_op!(executor; binary i32 => i32, |a, b| a.rotate_left(b as u32)),
            I64Rotl => exec_op!(executor; binary i64 => i64, |a, b| a.rotate_left(b as u32)),
            I32Rotr => exec_op!(executor; binary i32 => i32, |a, b| a.rotate_right(b as u32)),
            I64Rotr => exec_op!(executor; binary i64 => i64, |a, b| a.rotate_right(b as u32)),
            I64Add128 => *acc64 = executor.exec_i64_add128(*acc64 as i64) as u64,
            I64Sub128 => *acc64 = executor.exec_i64_sub128(*acc64 as i64) as u64,
            I64MulWideS => *acc64 = executor.exec_i64_mul_wide_s(*acc64 as i64) as u64,
            I64MulWideU => *acc64 = executor.exec_i64_mul_wide_u(*acc64 as i64) as u64,
            I32Clz => exec_op!(executor; unary i32 => i32, |v| v.leading_zeros() as i32),
            I64Clz => exec_op!(executor; unary i64 => i64, |v| i64::from(v.leading_zeros())),
            I32Ctz => exec_op!(executor; unary i32 => i32, |v| v.trailing_zeros() as i32),
            I64Ctz => exec_op!(executor; unary i64 => i64, |v| i64::from(v.trailing_zeros())),
            I32Popcnt => exec_op!(executor; unary i32 => i32, |v| v.count_ones() as i32),
            I64Popcnt => exec_op!(executor; unary i64 => i64, |v| i64::from(v.count_ones())),

            // Reference types
            RefFunc(func_idx) => ValueRef::stack_push(&mut executor.value_stack, ValueRef::from_category_addr(executor.module.resolve_func_addr(*func_idx))),
            RefNull(_) => ValueRef::stack_push(&mut executor.value_stack, ValueRef::NULL),
            RefIsNull => exec_op!(executor; unary ValueRef => i32, |v| i32::from(v.is_null())),
            RefAsNonNull => exec_op!(executor; unary ValueRef => ValueRef, |v| {
                if v.is_null() { cold!(Err(Trap::NullReference)) } else { Ok(v) }
            }?),
            RefI31 => exec_op!(executor; unary i32 => ValueRef, |v| ValueRef::from_i31(v)),
            I31GetS => exec_op!(executor; unary ValueRef => i32, |v| {
                if v.is_null() { cold!(Err(Trap::NullI31Reference)) }
                else { Ok(v.i31_s().expect("validated i31.get operand")) }
            }?),
            I31GetU => exec_op!(executor; unary ValueRef => u32, |v| {
                if v.is_null() { cold!(Err(Trap::NullI31Reference)) }
                else { Ok(v.i31_u().expect("validated i31.get operand")) }
            }?),
            RefEq => exec_op!(executor; binary ValueRef => i32, |a, b| i32::from(a == b)),
            RefTest(ty) => executor.exec_ref_test(*ty),
            RefCast(ty) => executor.exec_ref_cast(*ty)?,

            // GC objects
            StructNew(ty) => executor.exec_struct_new(*ty, false)?,
            StructNewDefault(ty) => executor.exec_struct_new(*ty, true)?,
            StructGet(idx) => executor.exec_struct_get(*idx, None)?,
            StructGetS(idx) => executor.exec_struct_get(*idx, Some(true))?,
            StructGetU(idx) => executor.exec_struct_get(*idx, Some(false))?,
            StructSet(idx) => executor.exec_struct_set(*idx)?,
            ArrayNew(ty) => executor.exec_array_new(*ty, false)?,
            ArrayNewDefault(ty) => executor.exec_array_new(*ty, true)?,
            ArrayNewFixed(idx) => executor.exec_array_new_fixed(*idx)?,
            ArrayNewData(idx) => executor.exec_array_new_data(*idx)?,
            ArrayNewElem(idx) => executor.exec_array_new_elem(*idx)?,
            ArrayGet(ty) => executor.exec_array_get(*ty, None)?,
            ArrayGetS(ty) => executor.exec_array_get(*ty, Some(true))?,
            ArrayGetU(ty) => executor.exec_array_get(*ty, Some(false))?,
            ArraySet(ty) => executor.exec_array_set(*ty)?,
            ArrayLen => executor.exec_array_len()?,
            ArrayFill(ty) => executor.exec_array_fill(*ty)?,
            ArrayCopy(idx) => executor.exec_array_copy(*idx)?,
            ArrayInitData(idx) => executor.exec_array_init_data(*idx)?,
            ArrayInitElem(idx) => executor.exec_array_init_elem(*idx)?,
            MemorySize(addr) => executor.exec_memory_size(*addr),
            MemoryGrow(addr) => executor.exec_memory_grow(*addr)?,
            Atomic(arg) => executor.exec_atomic(*arg)?,
            AtomicWait(memory, op) => executor.exec_atomic_wait(*memory, *op)?,
            AtomicFence => crate::shared::fence(crate::shared::Ordering::SeqCst),

            // Bulk memory operations
            MemoryCopy(idx) => executor.exec_memory_copy(*idx)?,
            MemoryFill(addr) => executor.exec_memory_fill(*addr)?,
            MemoryFillConst(idx) => executor.exec_memory_fill_const(*idx)?,
            MemoryInit(idx) => executor.exec_memory_init(*idx)?,
            DataDrop(data_index) => executor.state.get_data_mut(executor.module.resolve_data_addr(*data_index)).drop(),
            ElemDrop(elem_index) => executor.state.get_elem_mut(executor.module.resolve_elem_addr(*elem_index)).drop(),

            // Table instructions
            TableGet(table_idx) => executor.exec_table_get(*table_idx)?,
            TableSet(table_idx) => executor.exec_table_set(*table_idx)?,
            TableSize(table_idx) => executor.exec_table_size(*table_idx),
            TableInit(idx) => executor.exec_table_init(*idx)?,
            TableGrow(table_idx) => executor.exec_table_grow(*table_idx)?,
            TableFill(table_idx) => executor.exec_table_fill(*table_idx)?,
            TableCopy(idx) => executor.exec_table_copy(*idx)?,

            // Core memory load/store operations
            I32Store(arg) => exec_store!(arg, *acc32, false, 4),
            I64Store(arg) => exec_store!(arg, *acc64, true, 8),
            F32Store(arg) => exec_store!(arg, *acc32, false, 4),
            F64Store(arg) => exec_store!(arg, *acc64, true, 8),
            I32Store8(arg) => exec_store!(arg, *acc32 as u8, false, 1),
            I32Store16(arg) => exec_store!(arg, *acc32 as u16, false, 2),
            I64Store8(arg) => exec_store!(arg, *acc64 as u8, true, 1),
            I64Store16(arg) => exec_store!(arg, *acc64 as u16, true, 2),
            I64Store32(arg) => exec_store!(arg, *acc64 as u32, true, 4),
            I32Load(arg) => exec_load!(arg, u32, 4 => 32, u32),
            I64Load(arg) => exec_load!(arg, u64, 8 => 64, u64),
            F32Load(arg) => exec_load!(arg, u32, 4 => 32, u32),
            F64Load(arg) => exec_load!(arg, u64, 8 => 64, u64),
            I32Load8S(arg) => exec_load!(arg, i8, 1 => 32, i32),
            I32Load8U(arg) => exec_load!(arg, u8, 1 => 32, u32),
            I32Load16S(arg) => exec_load!(arg, i16, 2 => 32, i32),
            I32Load16U(arg) => exec_load!(arg, u16, 2 => 32, u32),
            I64Load8S(arg) => exec_load!(arg, i8, 1 => 64, i64),
            I64Load8U(arg) => exec_load!(arg, u8, 1 => 64, u64),
            I64Load16S(arg) => exec_load!(arg, i16, 2 => 64, i64),
            I64Load16U(arg) => exec_load!(arg, u16, 2 => 64, u64),
            I64Load32S(arg) => exec_load!(arg, i32, 4 => 64, i64),
            I64Load32U(arg) => exec_load!(arg, u32, 4 => 64, u64),

            // Numeric conversion operations
            F32ConvertI32S => exec_op!(executor; unary i32 => f32, |v| v as f32),
            F32ConvertI64S => exec_op!(executor; unary i64 => f32, |v| v as f32),
            F64ConvertI32S => exec_op!(executor; unary i32 => f64, |v| f64::from(v)),
            F64ConvertI64S => exec_op!(executor; unary i64 => f64, |v| v as f64),
            F32ConvertI32U => exec_op!(executor; unary u32 => f32, |v| v as f32),
            F32ConvertI64U => exec_op!(executor; unary u64 => f32, |v| v as f32),
            F64ConvertI32U => exec_op!(executor; unary u32 => f64, |v| f64::from(v)),
            F64ConvertI64U => exec_op!(executor; unary u64 => f64, |v| v as f64),

            // Sign-extension operations
            I32Extend8S => exec_op!(executor; unary i32 => i32, |v| i32::from(v as i8)),
            I32Extend16S => exec_op!(executor; unary i32 => i32, |v| i32::from(v as i16)),
            I64Extend8S => exec_op!(executor; unary i64 => i64, |v| i64::from(v as i8)),
            I64Extend16S => exec_op!(executor; unary i64 => i64, |v| i64::from(v as i16)),
            I64Extend32S => exec_op!(executor; unary i64 => i64, |v| i64::from(v as i32)),
            I64ExtendI32U => exec_op!(executor; unary u32 => i64, |v| i64::from(v)),
            I64ExtendI32S => exec_op!(executor; unary i32 => i64, |v| i64::from(v)),
            I32WrapI64 => exec_op!(executor; unary i64 => i32, |v| v as i32),
            F32DemoteF64 => exec_op!(executor; unary f64 => f32, |v| v as f32),
            F64PromoteF32 => exec_op!(executor; unary f32 => f64, |v| f64::from(v)),
            F32Abs => exec_op!(executor; unary f32 => f32, |v| v.abs()),
            F64Abs => exec_op!(executor; unary f64 => f64, |v| v.abs()),
            F32Neg => exec_op!(executor; unary f32 => f32, |v| -v),
            F64Neg => exec_op!(executor; unary f64 => f64, |v| -v),
            F32Ceil => exec_op!(executor; unary f32 => f32, |v| num::f32::ceil(v)),
            F64Ceil => exec_op!(executor; unary f64 => f64, |v| num::f64::ceil(v)),
            F32Floor => exec_op!(executor; unary f32 => f32, |v| num::f32::floor(v)),
            F64Floor => exec_op!(executor; unary f64 => f64, |v| num::f64::floor(v)),
            F32Trunc => exec_op!(executor; unary f32 => f32, |v| num::f32::trunc(v)),
            F64Trunc => exec_op!(executor; unary f64 => f64, |v| num::f64::trunc(v)),
            F32Nearest => exec_op!(executor; unary f32 => f32, |v| num::f32::nearest(v)),
            F64Nearest => exec_op!(executor; unary f64 => f64, |v| num::f64::nearest(v)),
            F32Sqrt => exec_op!(executor; unary f32 => f32, |v| num::f32::sqrt(v)),
            F64Sqrt => exec_op!(executor; unary f64 => f64, |v| num::f64::sqrt(v)),
            F32Min => exec_op!(executor; binary f32 => f32, |a, b| num::f32::minimum(a, b)),
            F64Min => exec_op!(executor; binary f64 => f64, |a, b| num::f64::minimum(a, b)),
            F32Max => exec_op!(executor; binary f32 => f32, |a, b| num::f32::maximum(a, b)),
            F64Max => exec_op!(executor; binary f64 => f64, |a, b| num::f64::maximum(a, b)),
            F32Copysign => exec_op!(executor; binary f32 => f32, |a, b| a.copysign(b)),
            F64Copysign => exec_op!(executor; binary f64 => f64, |a, b| a.copysign(b)),
            I32TruncF32S => exec_op!(executor; unary f32 => i32, |v| num::f32::trunc_to_i32(v)?),
            I32TruncF64S => exec_op!(executor; unary f64 => i32, |v| num::f64::trunc_to_i32(v)?),
            I32TruncF32U => exec_op!(executor; unary f32 => u32, |v| num::f32::trunc_to_u32(v)?),
            I32TruncF64U => exec_op!(executor; unary f64 => u32, |v| num::f64::trunc_to_u32(v)?),
            I64TruncF32S => exec_op!(executor; unary f32 => i64, |v| num::f32::trunc_to_i64(v)?),
            I64TruncF64S => exec_op!(executor; unary f64 => i64, |v| num::f64::trunc_to_i64(v)?),
            I64TruncF32U => exec_op!(executor; unary f32 => u64, |v| num::f32::trunc_to_u64(v)?),
            I64TruncF64U => exec_op!(executor; unary f64 => u64, |v| num::f64::trunc_to_u64(v)?),

            // Non-trapping float-to-int conversions
            I32TruncSatF32S => exec_op!(executor; unary f32 => i32, |v| v as i32),
            I32TruncSatF32U => exec_op!(executor; unary f32 => u32, |v| v as u32),
            I32TruncSatF64S => exec_op!(executor; unary f64 => i32, |v| v as i32),
            I32TruncSatF64U => exec_op!(executor; unary f64 => u32, |v| v as u32),
            I64TruncSatF32S => exec_op!(executor; unary f32 => i64, |v| v as i64),
            I64TruncSatF32U => exec_op!(executor; unary f32 => u64, |v| v as u64),
            I64TruncSatF64S => exec_op!(executor; unary f64 => i64, |v| v as i64),
            I64TruncSatF64U => exec_op!(executor; unary f64 => u64, |v| v as u64),

            // SIMD extension
            V128Not => exec_op!(executor; unary Value128 => Value128, |v| v.v128_not()),
            V128And => exec_op!(executor; binary Value128 => Value128, |a, b| a.v128_and(b)),
            V128AndNot => exec_op!(executor; binary Value128 => Value128, |a, b| a.v128_andnot(b)),
            V128Or => exec_op!(executor; binary Value128 => Value128, |a, b| a.v128_or(b)),
            V128Xor => exec_op!(executor; binary Value128 => Value128, |a, b| a.v128_xor(b)),
            V128Bitselect => exec_op!(executor; ternary Value128 => Value128, |a, b, c| Value128::v128_bitselect(a, b, c)),
            V128AnyTrue => exec_op!(executor; unary Value128 => i32, |v| v.v128_any_true() as i32),
            I8x16Swizzle => exec_op!(executor; binary Value128 => Value128, |a, s| a.i8x16_swizzle(s)),
            I8x16RelaxedSwizzle => exec_op!(executor; binary Value128 => Value128, |a, s| a.i8x16_relaxed_swizzle(s)),
            V128Load(arg) => *acc128 = executor.exec_mem_load::<Value128, 16>(*arg, *acc32, *acc64)?,
            V128Load8x8S(arg) => *acc128 = Value128::v128_load8x8_s(executor.exec_mem_load::<u64, 8>(*arg, *acc32, *acc64)?.to_le_bytes()),
            V128Load8x8U(arg) => *acc128 = Value128::v128_load8x8_u(executor.exec_mem_load::<u64, 8>(*arg, *acc32, *acc64)?.to_le_bytes()),
            V128Load16x4S(arg) => *acc128 = Value128::v128_load16x4_s(executor.exec_mem_load::<u64, 8>(*arg, *acc32, *acc64)?.to_le_bytes()),
            V128Load16x4U(arg) => *acc128 = Value128::v128_load16x4_u(executor.exec_mem_load::<u64, 8>(*arg, *acc32, *acc64)?.to_le_bytes()),
            V128Load32x2S(arg) => *acc128 = Value128::v128_load32x2_s(executor.exec_mem_load::<u64, 8>(*arg, *acc32, *acc64)?.to_le_bytes()),
            V128Load32x2U(arg) => *acc128 = Value128::v128_load32x2_u(executor.exec_mem_load::<u64, 8>(*arg, *acc32, *acc64)?.to_le_bytes()),
            V128Load8Splat(arg) => *acc128 = Value128::splat_i8(executor.exec_mem_load::<i8, 1>(*arg, *acc32, *acc64)?),
            V128Load16Splat(arg) => *acc128 = Value128::splat_i16(executor.exec_mem_load::<i16, 2>(*arg, *acc32, *acc64)?),
            V128Load32Splat(arg) => *acc128 = Value128::splat_i32(executor.exec_mem_load::<i32, 4>(*arg, *acc32, *acc64)?),
            V128Load64Splat(arg) => *acc128 = Value128::splat_i64(executor.exec_mem_load::<i64, 8>(*arg, *acc32, *acc64)?),
            V128Store(arg) => executor.exec_mem_store::<_, 16>(*arg, *acc128, ValueLane::S128, *acc32, *acc64)?,
            V128Store8Lane(arg) => executor.exec_mem_store_lane::<i8, 1>(*arg)?,
            V128Store16Lane(arg) => executor.exec_mem_store_lane::<i16, 2>(*arg)?,
            V128Store32Lane(arg) => executor.exec_mem_store_lane::<i32, 4>(*arg)?,
            V128Store64Lane(arg) => executor.exec_mem_store_lane::<i64, 8>(*arg)?,
            V128Load32Zero(arg) => *acc128 = Value128::from_i32x4([executor.exec_mem_load::<i32, 4>(*arg, *acc32, *acc64)?, 0, 0, 0]),
            V128Load64Zero(arg) => *acc128 = Value128::from_i64x2([executor.exec_mem_load::<i64, 8>(*arg, *acc32, *acc64)?, 0]),
            Const128(arg) => *acc128 = Value128(arg.resolve(&executor.func.data).value()),
            I8x16ExtractLaneS(lane) => *acc32 = acc128.extract_lane_i8(*lane) as i32 as u32,
            I8x16ExtractLaneU(lane) => *acc32 = acc128.extract_lane_u8(*lane) as u32,
            I16x8ExtractLaneS(lane) => *acc32 = acc128.extract_lane_i16(*lane) as i32 as u32,
            I16x8ExtractLaneU(lane) => *acc32 = acc128.extract_lane_u16(*lane) as u32,
            I32x4ExtractLane(lane) => *acc32 = acc128.extract_lane_i32(*lane) as u32,
            I64x2ExtractLane(lane) => *acc64 = acc128.extract_lane_i64(*lane) as u64,
            F32x4ExtractLane(lane) => *acc32 = acc128.extract_lane_f32(*lane).to_bits(),
            F64x2ExtractLane(lane) => *acc64 = acc128.extract_lane_f64(*lane).to_bits(),
            V128Load8Lane(arg) => executor.exec_mem_load_lane::<i8, 1>(*arg)?,
            V128Load16Lane(arg) => executor.exec_mem_load_lane::<i16, 2>(*arg)?,
            V128Load32Lane(arg) => executor.exec_mem_load_lane::<i32, 4>(*arg)?,
            V128Load64Lane(arg) => executor.exec_mem_load_lane::<i64, 8>(*arg)?,
            I8x16ReplaceLane(lane) => *acc128 = acc128.i8x16_replace_lane(*lane, *acc32 as i8),
            I16x8ReplaceLane(lane) => *acc128 = acc128.i16x8_replace_lane(*lane, *acc32 as i16),
            I32x4ReplaceLane(lane) => *acc128 = acc128.i32x4_replace_lane(*lane, *acc32 as i32),
            I64x2ReplaceLane(lane) => *acc128 = acc128.i64x2_replace_lane(*lane, *acc64 as i64),
            F32x4ReplaceLane(lane) => *acc128 = acc128.f32x4_replace_lane(*lane, f32::from_bits(*acc32)),
            F64x2ReplaceLane(lane) => *acc128 = acc128.f64x2_replace_lane(*lane, f64::from_bits(*acc64)),
            I8x16Splat => exec_op!(executor; unary i32 => Value128, |v| Value128::splat_i8(v as i8)),
            I16x8Splat => exec_op!(executor; unary i32 => Value128, |v| Value128::splat_i16(v as i16)),
            I32x4Splat => exec_op!(executor; unary i32 => Value128, |v| Value128::splat_i32(v)),
            I64x2Splat => exec_op!(executor; unary i64 => Value128, |v| Value128::splat_i64(v)),
            F32x4Splat => exec_op!(executor; unary f32 => Value128, |v| Value128::splat_f32(v)),
            F64x2Splat => exec_op!(executor; unary f64 => Value128, |v| Value128::splat_f64(v)),
            I8x16Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_eq(b)),
            I16x8Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_eq(b)),
            I32x4Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_eq(b)),
            I64x2Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_eq(b)),
            F32x4Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_eq(b)),
            F64x2Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_eq(b)),
            I8x16Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_ne(b)),
            I16x8Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_ne(b)),
            I32x4Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_ne(b)),
            I64x2Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_ne(b)),
            F32x4Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_ne(b)),
            F64x2Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_ne(b)),
            I8x16LtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_lt_s(b)),
            I16x8LtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_lt_s(b)),
            I32x4LtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_lt_s(b)),
            I64x2LtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_lt_s(b)),
            I8x16LtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_lt_u(b)),
            I16x8LtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_lt_u(b)),
            I32x4LtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_lt_u(b)),
            F32x4Lt => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_lt(b)),
            F64x2Lt => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_lt(b)),
            F32x4Gt => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_gt(b)),
            F64x2Gt => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_gt(b)),
            I8x16GtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_gt_s(b)),
            I16x8GtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_gt_s(b)),
            I32x4GtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_gt_s(b)),
            I64x2GtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_gt_s(b)),
            I64x2LeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_le_s(b)),
            F32x4Le => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_le(b)),
            F64x2Le => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_le(b)),
            I8x16GtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_gt_u(b)),
            I16x8GtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_gt_u(b)),
            I32x4GtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_gt_u(b)),
            F32x4Ge => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_ge(b)),
            F64x2Ge => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_ge(b)),
            I8x16LeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_le_s(b)),
            I16x8LeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_le_s(b)),
            I32x4LeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_le_s(b)),
            I8x16LeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_le_u(b)),
            I16x8LeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_le_u(b)),
            I32x4LeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_le_u(b)),
            I8x16GeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_ge_s(b)),
            I16x8GeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_ge_s(b)),
            I32x4GeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_ge_s(b)),
            I64x2GeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_ge_s(b)),
            I8x16GeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_ge_u(b)),
            I16x8GeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_ge_u(b)),
            I32x4GeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_ge_u(b)),
            I8x16Abs => exec_op!(executor; unary Value128 => Value128, |a| a.i8x16_abs()),
            I16x8Abs => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_abs()),
            I32x4Abs => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_abs()),
            I64x2Abs => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_abs()),
            I8x16Neg => exec_op!(executor; unary Value128 => Value128, |a| a.i8x16_neg()),
            I16x8Neg => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_neg()),
            I32x4Neg => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_neg()),
            I64x2Neg => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_neg()),
            I8x16AllTrue => exec_op!(executor; unary Value128 => i32, |v| v.i8x16_all_true() as i32),
            I16x8AllTrue => exec_op!(executor; unary Value128 => i32, |v| v.i16x8_all_true() as i32),
            I32x4AllTrue => exec_op!(executor; unary Value128 => i32, |v| v.i32x4_all_true() as i32),
            I64x2AllTrue => exec_op!(executor; unary Value128 => i32, |v| v.i64x2_all_true() as i32),
            I8x16Bitmask => exec_op!(executor; unary Value128 => i32, |v| v.i8x16_bitmask() as i32),
            I16x8Bitmask => exec_op!(executor; unary Value128 => i32, |v| v.i16x8_bitmask() as i32),
            I32x4Bitmask => exec_op!(executor; unary Value128 => i32, |v| v.i32x4_bitmask() as i32),
            I64x2Bitmask => exec_op!(executor; unary Value128 => i32, |v| v.i64x2_bitmask() as i32),
            I8x16Shl => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i8x16_shl(a as u32)),
            I16x8Shl => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i16x8_shl(a as u32)),
            I32x4Shl => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i32x4_shl(a as u32)),
            I64x2Shl => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i64x2_shl(a as u32)),
            I8x16ShrS => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i8x16_shr_s(a as u32)),
            I16x8ShrS => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i16x8_shr_s(a as u32)),
            I32x4ShrS => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i32x4_shr_s(a as u32)),
            I64x2ShrS => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i64x2_shr_s(a as u32)),
            I8x16ShrU => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i8x16_shr_u(a as u32)),
            I16x8ShrU => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i16x8_shr_u(a as u32)),
            I32x4ShrU => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i32x4_shr_u(a as u32)),
            I64x2ShrU => exec_op!(executor; mixed i32, Value128 => Value128, |a, b| b.i64x2_shr_u(a as u32)),
            I8x16Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_add(b)),
            I16x8Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_add(b)),
            I32x4Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_add(b)),
            I64x2Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_add(b)),
            I8x16Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_sub(b)),
            I16x8Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_sub(b)),
            I32x4Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_sub(b)),
            I64x2Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_sub(b)),
            I8x16MinS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_min_s(b)),
            I16x8MinS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_min_s(b)),
            I32x4MinS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_min_s(b)),
            I8x16MinU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_min_u(b)),
            I16x8MinU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_min_u(b)),
            I32x4MinU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_min_u(b)),
            I8x16MaxS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_max_s(b)),
            I16x8MaxS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_max_s(b)),
            I32x4MaxS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_max_s(b)),
            I8x16MaxU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_max_u(b)),
            I16x8MaxU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_max_u(b)),
            I32x4MaxU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_max_u(b)),
            I64x2Mul => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_mul(b)),
            I16x8Mul => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_mul(b)),
            I32x4Mul => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_mul(b)),
            I8x16NarrowI16x8S => exec_op!(executor; binary Value128 => Value128, |a, b| Value128::i8x16_narrow_i16x8_s(a, b)),
            I8x16NarrowI16x8U => exec_op!(executor; binary Value128 => Value128, |a, b| Value128::i8x16_narrow_i16x8_u(a, b)),
            I16x8NarrowI32x4S => exec_op!(executor; binary Value128 => Value128, |a, b| Value128::i16x8_narrow_i32x4_s(a, b)),
            I16x8NarrowI32x4U => exec_op!(executor; binary Value128 => Value128, |a, b| Value128::i16x8_narrow_i32x4_u(a, b)),
            I8x16AddSatS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_add_sat_s(b)),
            I16x8AddSatS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_add_sat_s(b)),
            I8x16AddSatU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_add_sat_u(b)),
            I16x8AddSatU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_add_sat_u(b)),
            I8x16SubSatS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_sub_sat_s(b)),
            I16x8SubSatS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_sub_sat_s(b)),
            I8x16SubSatU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_sub_sat_u(b)),
            I16x8SubSatU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_sub_sat_u(b)),
            I8x16AvgrU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_avgr_u(b)),
            I16x8AvgrU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_avgr_u(b)),
            I16x8ExtAddPairwiseI8x16S => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extadd_pairwise_i8x16_s()),
            I16x8ExtAddPairwiseI8x16U => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extadd_pairwise_i8x16_u()),
            I32x4ExtAddPairwiseI16x8S => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extadd_pairwise_i16x8_s()),
            I32x4ExtAddPairwiseI16x8U => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extadd_pairwise_i16x8_u()),
            I16x8ExtMulLowI8x16S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_extmul_low_i8x16_s(b)),
            I16x8ExtMulLowI8x16U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_extmul_low_i8x16_u(b)),
            I16x8ExtMulHighI8x16S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_extmul_high_i8x16_s(b)),
            I16x8ExtMulHighI8x16U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_extmul_high_i8x16_u(b)),
            I32x4ExtMulLowI16x8S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_extmul_low_i16x8_s(b)),
            I32x4ExtMulLowI16x8U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_extmul_low_i16x8_u(b)),
            I32x4ExtMulHighI16x8S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_extmul_high_i16x8_s(b)),
            I32x4ExtMulHighI16x8U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_extmul_high_i16x8_u(b)),
            I64x2ExtMulLowI32x4S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_extmul_low_i32x4_s(b)),
            I64x2ExtMulLowI32x4U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_extmul_low_i32x4_u(b)),
            I64x2ExtMulHighI32x4S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_extmul_high_i32x4_s(b)),
            I64x2ExtMulHighI32x4U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_extmul_high_i32x4_u(b)),
            I16x8ExtendLowI8x16S => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extend_low_i8x16_s()),
            I16x8ExtendLowI8x16U => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extend_low_i8x16_u()),
            I16x8ExtendHighI8x16S => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extend_high_i8x16_s()),
            I16x8ExtendHighI8x16U => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extend_high_i8x16_u()),
            I32x4ExtendLowI16x8S => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extend_low_i16x8_s()),
            I32x4ExtendLowI16x8U => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extend_low_i16x8_u()),
            I32x4ExtendHighI16x8S => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extend_high_i16x8_s()),
            I32x4ExtendHighI16x8U => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extend_high_i16x8_u()),
            I64x2ExtendLowI32x4S => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_extend_low_i32x4_s()),
            I64x2ExtendLowI32x4U => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_extend_low_i32x4_u()),
            I64x2ExtendHighI32x4S => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_extend_high_i32x4_s()),
            I64x2ExtendHighI32x4U => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_extend_high_i32x4_u()),
            I8x16Popcnt => exec_op!(executor; unary Value128 => Value128, |v| v.i8x16_popcnt()),
            I8x16Shuffle(idx) => *acc128 = executor.exec_simd_shuffle(*acc128, Value128(idx.resolve(&executor.func.data).value())),
            I16x8Q15MulrSatS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_q15mulr_sat_s(b)),
            I32x4DotI16x8S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_dot_i16x8_s(b)),
            I8x16RelaxedLaneselect => exec_op!(executor; ternary Value128 => Value128, |a, b, c| Value128::i8x16_relaxed_laneselect(a, b, c)),
            I16x8RelaxedLaneselect => exec_op!(executor; ternary Value128 => Value128, |a, b, c| Value128::i16x8_relaxed_laneselect(a, b, c)),
            I32x4RelaxedLaneselect => exec_op!(executor; ternary Value128 => Value128, |a, b, c| Value128::i32x4_relaxed_laneselect(a, b, c)),
            I64x2RelaxedLaneselect => exec_op!(executor; ternary Value128 => Value128, |a, b, c| Value128::i64x2_relaxed_laneselect(a, b, c)),
            I16x8RelaxedQ15mulrS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_relaxed_q15mulr_s(b)),
            I16x8RelaxedDotI8x16I7x16S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_relaxed_dot_i8x16_i7x16_s(b)),
            I32x4RelaxedDotI8x16I7x16AddS => exec_op!(executor; ternary Value128 => Value128, |a, b, c| a.i32x4_relaxed_dot_i8x16_i7x16_add_s(b, c)),
            F32x4Ceil => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_ceil()),
            F64x2Ceil => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_ceil()),
            F32x4Floor => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_floor()),
            F64x2Floor => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_floor()),
            F32x4Trunc => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_trunc()),
            F64x2Trunc => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_trunc()),
            F32x4Nearest => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_nearest()),
            F64x2Nearest => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_nearest()),
            F32x4Abs => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_abs()),
            F64x2Abs => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_abs()),
            F32x4Neg => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_neg()),
            F64x2Neg => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_neg()),
            F32x4Sqrt => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_sqrt()),
            F64x2Sqrt => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_sqrt()),
            F32x4Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_add(b)),
            F64x2Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_add(b)),
            F32x4Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_sub(b)),
            F64x2Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_sub(b)),
            F32x4Mul => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_mul(b)),
            F64x2Mul => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_mul(b)),
            F32x4Div => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_div(b)),
            F64x2Div => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_div(b)),
            F32x4Min => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_min(b)),
            F64x2Min => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_min(b)),
            F32x4Max => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_max(b)),
            F64x2Max => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_max(b)),
            F32x4PMin => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_pmin(b)),
            F32x4PMax => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_pmax(b)),
            F64x2PMin => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_pmin(b)),
            F64x2PMax => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_pmax(b)),
            F32x4RelaxedMadd => exec_op!(executor; ternary Value128 => Value128, |a, b, c| a.f32x4_relaxed_madd(b, c)),
            F32x4RelaxedNmadd => exec_op!(executor; ternary Value128 => Value128, |a, b, c| a.f32x4_relaxed_nmadd(b, c)),
            F64x2RelaxedMadd => exec_op!(executor; ternary Value128 => Value128, |a, b, c| a.f64x2_relaxed_madd(b, c)),
            F64x2RelaxedNmadd => exec_op!(executor; ternary Value128 => Value128, |a, b, c| a.f64x2_relaxed_nmadd(b, c)),
            F32x4RelaxedMin => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_relaxed_min(b)),
            F32x4RelaxedMax => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_relaxed_max(b)),
            F64x2RelaxedMin => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_relaxed_min(b)),
            F64x2RelaxedMax => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_relaxed_max(b)),
            I32x4TruncSatF32x4S => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_trunc_sat_f32x4_s()),
            I32x4TruncSatF32x4U => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_trunc_sat_f32x4_u()),
            F32x4ConvertI32x4S => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_convert_i32x4_s()),
            F32x4ConvertI32x4U => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_convert_i32x4_u()),
            F64x2ConvertLowI32x4S => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_convert_low_i32x4_s()),
            F64x2ConvertLowI32x4U => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_convert_low_i32x4_u()),
            F32x4DemoteF64x2Zero => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_demote_f64x2_zero()),
            F64x2PromoteLowF32x4 => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_promote_low_f32x4()),
            I32x4TruncSatF64x2SZero => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_trunc_sat_f64x2_s_zero()),
            I32x4TruncSatF64x2UZero => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_trunc_sat_f64x2_u_zero()),
            I32x4RelaxedTruncF32x4S => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f32x4_s()),
            I32x4RelaxedTruncF32x4U => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f32x4_u()),
            I32x4RelaxedTruncF64x2SZero => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f64x2_s_zero()),
            I32x4RelaxedTruncF64x2UZero => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f64x2_u_zero()),
        }
    };
}

pub(super) use {execute_instruction, instruction_handlers};

use alloc::boxed::Box;

use super::{FuncAddr, GlobalAddr, LocalAddr, TableAddr, TagAddr, TypeAddr, ValueCounts, ValueLane, ValueLanes};
use crate::operands::*;
use crate::operations::{AtomicArg, AtomicWaitOp, BinOp, CmpOp, FloatCmpOp, LoadKind32, MemoryUpdateOp};
use crate::{DataAddr, ElemAddr, MemAddr, ModuleFuncIdx, RefType};

/// A catch clause attached to a lowered `try_table` instruction.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ExceptionCatch {
    /// Catch exceptions carrying the specified module-local tag.
    Tag { tag: TagAddr, landing_pad: u32, base: ValueCounts, with_ref: bool },
    /// Catch any exception.
    All { landing_pad: u32, base: ValueCounts, with_ref: bool },
}

/// A statically lowered exception handler range.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ExceptionHandler {
    /// First protected instruction, inclusive.
    pub start_ip: u32,
    /// End of the protected instruction range, exclusive.
    pub end_ip: u32,
    /// Catch clauses in source order.
    pub catches: Box<[ExceptionCatch]>,
}

#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ConstInstruction {
    I32Const(i32),
    I64Const(i64),
    F32Const(f32),
    F64Const(f64),
    V128Const([u8; 16]),
    GlobalGet32(GlobalAddr),
    GlobalGet64(GlobalAddr),
    GlobalGet128(GlobalAddr),
    GlobalGetRef(GlobalAddr),
    RefNull(RefType),
    RefFunc(ModuleFuncIdx),
    RefI31,
    AnyConvertExtern,
    ExternConvertAny,
    StructNew(TypeAddr),
    StructNewDefault(TypeAddr),
    ArrayNew(TypeAddr),
    ArrayNewDefault(TypeAddr),
    ArrayNewFixed(TypeAddr, u32),
    I32Add,
    I32Sub,
    I32Mul,
    I64Add,
    I64Sub,
    I64Mul,
}

/// Operand-lane effects used while lowering an instruction.
///
/// Inputs identify lanes whose top value is consumed, not the instruction's
/// complete Wasm operand list. Earlier operands in the same lane stay stacked.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum InstructionEffects {
    /// Consumes the indicated lane tops and optionally produces a new lane top.
    Operands { inputs: ValueLanes, output: Option<ValueLane> },
    /// Consumes a branch condition, then materializes surviving values at the edge.
    Branch { inputs: ValueLanes },
    /// Requires materialized lanes or signature-dependent handling by the emitter.
    Boundary,
}

/// A known copy of a cached value, interpreted within its value lane.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum ValueSource {
    /// A frame local containing the same value.
    Local(LocalAddr),
    /// An inline 32-bit constant.
    Const32(i32),
    /// An inline, sign-extended 64-bit constant.
    Const64Imm(i32),
    /// An inline, zero-extended vector constant.
    Const128Imm(u32),
    /// A pooled 64-bit constant.
    Const64(Operand64Idx<i64>),
    /// A pooled vector constant.
    Const128(Operand128Idx<[u8; 16]>),
}

/// An inline or pooled destination local used by instruction metadata.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum LocalDestination {
    /// An inline frame-local address.
    Inline(LocalAddr),
    /// The third local in a packed local/local/destination operand.
    Third64(Operand64Idx<(u16, u16, u16)>),
    /// The destination following a local in a packed 32-bit local/constant operand.
    Second64(Operand64Idx<(u16, u16, u32)>),
    /// The destination following a local in a packed 64-bit local/constant operand.
    Second128(Operand128Idx<(u16, u16, u64)>),
    /// The local following a target and 32-bit value in a branch-update operand.
    Third128(Operand128Idx<(u32, i32, u16)>),
    /// The local following a target and full-width 64-bit value in a branch-update operand.
    Third128Wide(Operand128Idx<(u32, i64, u16)>),
}

/// Value provenance and direct transfers used during instruction lowering.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum InstructionValueEffect {
    /// No declared provenance, local writes must be treated conservatively.
    Unknown,
    /// Does not write frame locals, independently of memory/global or operand effects.
    NoLocalWrites,
    /// Reads a known value without other observable effects.
    Read(ValueSource),
    /// Copies the consumed lane top to a local, optionally retaining it as a result.
    LocalStore(ValueLane, LocalAddr),
    /// An effectful producer also saves its result in this local.
    LocalResult(LocalDestination),
    /// Writes a local without producing a cached result.
    LocalWrite(ValueLane, LocalDestination),
    /// Discards a live operand without changing the physical cached bits.
    Drop(ValueLane),
}

// Bind operands in the generated match arm, shared by metadata and form methods.
macro_rules! instruction_pattern {
    ($variant:ident, $arg:ident; ([load $lane:ident]); ($($value:tt)?)) => { Instruction::$variant($arg) };
    ($variant:ident, $arg:ident; ([store $lane:ident]); ($($value:tt)?)) => { Instruction::$variant($arg) };
    ($variant:ident, $arg:ident; ($($effect:tt)?); ([$kind:ident($($binding:pat),*) => $($value:expr),+])) => {
        Instruction::$variant($($binding),*)
    };
    ($variant:ident, $arg:ident; ($($effect:tt)?); ($($value:tt)?)) => { Instruction::$variant { .. } };
    ($variant:ident, form($field:ty), $value:ident, $first:ident, $second:ident, $third:ident, $form:ident) => {
        Instruction::$variant($value)
    };
    ($variant:ident, form($a:ty, $b:ty, $c:ty), $value:ident, $first:ident, $second:ident, $third:ident, $form:ident) => {
        Instruction::$variant($first, $second, $third)
    };
    ($variant:ident, form $(($($field:ty),*))?, $value:ident, $first:ident, $second:ident, $third:ident) => {
        Instruction::$variant { .. }
    };
    ($variant:ident, target, [$location:ident $binding:ident => $destination:expr]) => { Instruction::$variant($binding) };
    ($variant:ident, target) => { Instruction::$variant { .. } };
}

macro_rules! instruction_value_effect {
    () => { InstructionValueEffect::Unknown };
    ([$kind:ident($($binding:pat),*) => $($value:expr),+]) => { InstructionValueEffect::$kind($($value),+) };
    ([$kind:ident => $($value:expr),+]) => { InstructionValueEffect::$kind($($value),+) };
    ([$kind:ident]) => { InstructionValueEffect::$kind };
}

macro_rules! instruction_form {
    ($value:ident, $first:ident, $second:ident, $third:ident $(; ($($field:ty),*))?) => {
        None
    };
    ($value:ident, $first:ident, $second:ident, $third:ident; ($field:ty), $form:ident) => {
        Some(Instruction::$form($value))
    };
    ($value:ident, $first:ident, $second:ident, $third:ident; ($a:ty, $b:ty, $c:ty), $form:ident) => {
        Some(Instruction::$form($first, $second, $third))
    };
}

macro_rules! instruction_effect {
    ($arg:ident) => { InstructionEffects::Boundary };
    ($arg:ident, [stack $($input:ident),* -> $($output:ident)?]) => {
        instruction_effect!($arg, [$($input),* -> $($output)?])
    };
    ($arg:ident, [$($input:ident),* -> $($output:ident)?]) => {
        InstructionEffects::Operands {
            inputs: ValueLanes::EMPTY $(.with(ValueLane::$input))*,
            output: instruction_effect!(@output $($output)?),
        }
    };
    (@output) => { None };
    (@output $lane:ident) => { Some(ValueLane::$lane) };
    ($arg:ident, [branch $lane:ident]) => {
        InstructionEffects::Branch { inputs: ValueLanes::EMPTY.with(ValueLane::$lane) }
    };
    ($arg:ident, [load $lane:ident]) => {
        InstructionEffects::Operands {
            inputs: ValueLanes::EMPTY.with(if $arg.address64 { ValueLane::S64 } else { ValueLane::S32 }),
            output: Some(ValueLane::$lane),
        }
    };
    ($arg:ident, [store $lane:ident]) => {
        InstructionEffects::Operands {
            inputs: ValueLanes::EMPTY.with(ValueLane::$lane)
                .with(if $arg.address64 { ValueLane::S64 } else { ValueLane::S32 }),
            output: None,
        }
    };
}

macro_rules! instruction_input_counts {
    ([stack $($input:ident),* -> $($output:ident)?]) => {
        Some(ValueCounts {
            c32: 0 $(+ matches!(ValueLane::$input, ValueLane::S32) as u16)*,
            c64: 0 $(+ matches!(ValueLane::$input, ValueLane::S64) as u16)*,
            c128: 0 $(+ matches!(ValueLane::$input, ValueLane::S128) as u16)*,
        })
    };
    ($($effect:tt)?) => { None };
}

macro_rules! instruction_retarget {
    ($target:ident, $operands64:ident, $operands128:ident) => {
        false
    };
    ($target:ident, $operands64:ident, $operands128:ident,
        [inline $binding:ident => $destination:expr]) => {{
        $destination = $target;
        true
    }};
    ($target:ident, $operands64:ident, $operands128:ident,
        [pool64 $binding:ident => $index:expr]) => {{
        let Some(operand) = $operands64.get_mut(($index).index()) else { return false };
        *operand = operand.with_target($target);
        true
    }};
    ($target:ident, $operands64:ident, $operands128:ident,
        [pool128 $binding:ident => $index:expr]) => {{
        let Some(operand) = $operands128.get_mut(($index).index()) else { return false };
        *operand = operand.with_target($target);
        true
    }};
}

macro_rules! define_instructions {
    ($($variant:ident $(($($field:ty),* $(,)?))? $({ $($name:ident: $named_field:ty),* $(,)? })? $(=> $effect:tt)?
        $(@value $value_effect:tt)? $(@preserve $preserving:ident)? $(@discard $discarding:ident)? $(@target $target:tt)? $(@loop $looping:ident)?),* $(,)?) => {
        /// A TinyWasm bytecode instruction.
        ///
        /// These instructions are an internal, version-specific representation and do not
        /// map one-to-one to WebAssembly instructions. Their variants and serialized form
        /// may change between TinyWasm releases.
        ///
        /// Ordinary scalar arithmetic reads its last operand from the fixed 32-bit or
        /// 64-bit accumulator and writes its result to the accumulator for the result's
        /// width. Earlier operands come from their stack lane. Explicit spill/reload
        /// instructions bridge materialized-stack boundaries. Vector arithmetic uses an
        /// cached lane top, with earlier operands on the vector stack. Tail dispatch can
        /// carry this top in a native SIMD register on supported backends.
        /// Calls and returns keep the last argument or result in each scalar lane in
        /// its accumulator, with earlier same-lane values and all vectors on the stack.
        /// Host callbacks use owned scalar payloads and native argument/result buffers.
        /// Public Wasm returns and fuel suspension materialize scalar results.
        #[derive(Clone, Copy, PartialEq)]
        #[cfg_attr(feature = "debug", derive(Debug))]
        #[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
        pub enum Instruction {
            $($variant $(($($field),*))? $({ $($name: $named_field),* })?),*
        }

        #[doc(hidden)]
        #[repr(u16)]
        #[derive(Clone, Copy)]
        pub enum InstructionOpcode {
            $($variant,)*
        }

        impl InstructionOpcode {
            #[doc(hidden)]
            pub const COUNT: usize = [$(Self::$variant),*].len();

        }

        impl Instruction {
            /// Returns operand-lane effects declared beside this instruction.
            ///
            /// Unannotated instructions are conservative boundaries. Call and return
            /// signature handling remains the emitter's responsibility.
            #[inline(always)]
            pub const fn effects(&self) -> InstructionEffects {
                match *self {
                    $(instruction_pattern!($variant, arg; ($($effect)?); ()) => instruction_effect!(arg $(, $effect)?),)*
                }
            }

            /// Returns input counts, or `None` when unknown.
            #[inline(always)]
            pub const fn input_counts(&self) -> Option<ValueCounts> {
                match self {
                    $(Self::$variant { .. } => instruction_input_counts!($($effect)?),)*
                }
            }

            /// Returns declared value provenance without assuming a full operand-stack signature.
            #[inline(always)]
            pub const fn value_effect(&self) -> InstructionValueEffect {
                match *self {
                    $(instruction_pattern!($variant, arg; (); ($($value_effect)?)) => instruction_value_effect!($($value_effect)?),)*
                }
            }

            /// Returns lane effects and value provenance with one instruction decode.
            #[inline(always)]
            pub const fn lowering_effects(&self) -> (InstructionEffects, InstructionValueEffect) {
                match *self {
                    $(instruction_pattern!($variant, arg; ($($effect)?); ($($value_effect)?)) => (
                        instruction_effect!(arg $(, $effect)?),
                        instruction_value_effect!($($value_effect)?),
                    ),)*
                }
            }

            /// Returns a form that saves the previous same-lane top before producing a new one.
            #[inline(always)]
            pub const fn preserving(&self) -> Option<Self> {
                match *self {
                    $(instruction_pattern!($variant, form $(($($field),*))?, value, first, second, third $(, $preserving)?) =>
                        instruction_form!(value, first, second, third $(; ($($field),*))? $(, $preserving)?),)*
                }
            }

            /// Returns a form with the same side effects that discards the produced value.
            #[inline(always)]
            pub const fn without_result(&self) -> Option<Self> {
                match *self {
                    $(instruction_pattern!($variant, form $(($($field),*))?, value, first, second, third $(, $discarding)?) =>
                        instruction_form!(value, first, second, third $(; ($($field),*))? $(, $discarding)?),)*
                }
            }

            /// Returns an equivalent form for a branch resolved to this instruction itself.
            ///
            /// Use only after confirming that the resolved target equals the source instruction index.
            #[inline(always)]
            pub const fn self_loop_form(&self) -> Option<Self> {
                match *self {
                    $(instruction_pattern!($variant, form $(($($field),*))?, value, first, second, third $(, $looping)?) =>
                        instruction_form!(value, first, second, third $(; ($($field),*))? $(, $looping)?),)*
                }
            }

            /// Updates this instruction's declared branch target.
            ///
            /// Pooled target operands must be private to the instruction, not shared
            /// with immutable operands. Returns false for an unsupported instruction
            /// or an out-of-bounds pool index, without changing either in that case.
            #[doc(hidden)]
            pub fn retarget(&mut self, target: u32, operands64: &mut [Operand64], operands128: &mut [Operand128]) -> bool {
                match self {
                    $(instruction_pattern!($variant, target $(, $target)?) =>
                        instruction_retarget!(target, operands64, operands128 $(, $target)?),)*
                }
            }

            /// Returns the compact opcode used to dispatch this instruction.
            #[inline(always)]
            pub const fn opcode(&self) -> InstructionOpcode {
                match self {
                    $(Self::$variant { .. } => InstructionOpcode::$variant,)*
                }
            }
        }
    };
}

define_instructions! {
    // Core sections follow the WebAssembly binary instruction specification.
    // TinyWasm uses lane-specific forms and lowers structured control to jumps.
    // Effects describe consumed/produced lane tops, not full Wasm signatures.
    // [->] preserves all cached lanes, omitted effects require a materialized boundary.
    // Load/store effects select their address lane from MemoryArg::address64.
    // [stack ...] lists all inputs, other annotations list cached tops.

    // > Parametric Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#parametric-instructions>
    Unreachable,
    Drop32 => [S32 ->] @value [Drop => ValueLane::S32], Select32 => [S32 -> S32],
    Drop64 => [S64 ->] @value [Drop => ValueLane::S64], Select64 => [S32, S64 -> S64],
    Drop128 => [S128 ->] @value [Drop => ValueLane::S128], Select128 => [S32, S128 -> S128],
    SelectMulti(ValueCounts),

    // > Control Instructions (jump-oriented, lowered from structured control during parsing)
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#control-instructions>
    Jump(u32) @value [NoLocalWrites] @target [inline target => *target],
    // Final lowering forms. Keep bypasses a forward target's Reload32 without suspending.
    JumpSpill32(u32) @target [inline target => *target], JumpKeep32(u32) @target [inline target => *target],
    JumpSpill64(u32) @target [inline target => *target], JumpKeep64(u32) @target [inline target => *target],
    JumpSpill128(u32) @target [inline target => *target], JumpKeep128(u32) @target [inline target => *target],
    JumpIfZero32(u32) => [branch S32] @value [NoLocalWrites] @target [inline target => *target],
    JumpIfNonZero32(u32) => [branch S32] @value [NoLocalWrites] @target [inline target => *target],
    JumpIfZero64(u32) => [branch S64] @value [NoLocalWrites] @target [inline target => *target],
    JumpIfNonZero64(u32) => [branch S64] @value [NoLocalWrites] @target [inline target => *target],
    JumpIfRefNull(u32) @target [inline target => *target],
    JumpIfRefNonNull(u32) @target [inline target => *target],
    BrOnCast(Operand64Idx<(u32, u32)>) @target [pool64 index => *index],
    BrOnCastFail(Operand64Idx<(u32, u32)>) @target [pool64 index => *index],
    DropKeep32 { base: u16, keep: u16 }, DropKeep64 { base: u16, keep: u16 }, DropKeep128 { base: u16, keep: u16 }, BranchTable(Operand128Idx<BranchTableOperand>) => [branch S32] @target [pool128 index => *index],
    Return,
    ReturnVoid,
    Return32,
    Return64,
    Return128,
    Call(FuncAddr),
    CallSelf,
    CallIndirect(Operand64Idx<(u32, u32)>),
    CallRef(TypeAddr),
    ReturnCall(FuncAddr),
    ReturnCallSelf,
    ReturnCallIndirect(Operand64Idx<(u32, u32)>),
    ReturnCallRef(TypeAddr),
    Throw(TagAddr),
    ThrowRef,

    // > Variable Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#variable-instructions>
    GlobalGet32(GlobalAddr) => [-> S32], GlobalSet32(GlobalAddr) => [S32 ->] @value [NoLocalWrites],
    LocalGet32(LocalAddr) => [stack -> S32] @value [Read(local) => ValueSource::Local(local)] @preserve LocalGet32Preserve,
    LocalSet32(LocalAddr) => [stack S32 ->] @value [LocalStore(local) => ValueLane::S32, local],
    LocalTee32(LocalAddr) => [stack S32 -> S32] @value [LocalStore(local) => ValueLane::S32, local] @discard LocalSet32,
    GlobalGet64(GlobalAddr) => [-> S64], GlobalSet64(GlobalAddr) => [S64 ->] @value [NoLocalWrites],
    LocalGet64(LocalAddr) => [stack -> S64] @value [Read(local) => ValueSource::Local(local)] @preserve LocalGet64Preserve,
    LocalSet64(LocalAddr) => [stack S64 ->] @value [LocalStore(local) => ValueLane::S64, local],
    LocalTee64(LocalAddr) => [stack S64 -> S64] @value [LocalStore(local) => ValueLane::S64, local] @discard LocalSet64,
    GlobalGet128(GlobalAddr), GlobalSet128(GlobalAddr),
    LocalGet128(LocalAddr) => [stack -> S128] @value [Read(local) => ValueSource::Local(local)] @preserve LocalGet128Preserve,
    LocalSet128(LocalAddr) => [stack S128 ->] @value [LocalStore(local) => ValueLane::S128, local],
    LocalTee128(LocalAddr) => [stack S128 -> S128] @value [LocalStore(local) => ValueLane::S128, local] @discard LocalSet128,

    // > Table Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#table-instructions>
    TableInit(Operand64Idx<(u32, u32)>),
    TableGet(TableAddr),
    TableSet(TableAddr),
    TableCopy(Operand64Idx<(u32, u32)>),
    TableGrow(TableAddr),
    TableSize(TableAddr),
    TableFill(TableAddr),
    ElemDrop(ElemAddr),

    // > Memory Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#memory-instructions>
    I32Load(MemoryArg) => [load S32], I64Load(MemoryArg) => [load S64], F32Load(MemoryArg) => [load S32], F64Load(MemoryArg) => [load S64],
    I32Load8S(MemoryArg) => [load S32], I32Load8U(MemoryArg) => [load S32], I32Load16S(MemoryArg) => [load S32], I32Load16U(MemoryArg) => [load S32],
    I64Load8S(MemoryArg) => [load S64], I64Load8U(MemoryArg) => [load S64], I64Load16S(MemoryArg) => [load S64], I64Load16U(MemoryArg) => [load S64], I64Load32S(MemoryArg) => [load S64], I64Load32U(MemoryArg) => [load S64],
    I32Store(MemoryArg) => [store S32], I64Store(MemoryArg) => [store S64], F32Store(MemoryArg) => [store S32], F64Store(MemoryArg) => [store S64],
    I32Store8(MemoryArg) => [store S32], I32Store16(MemoryArg) => [store S32], I64Store8(MemoryArg) => [store S64], I64Store16(MemoryArg) => [store S64], I64Store32(MemoryArg) => [store S64],
    MemorySize(MemAddr),
    MemoryGrow(MemAddr),
    MemoryInit(Operand64Idx<(u32, u32)>), MemoryCopy(Operand64Idx<(u32, u32)>),
    MemoryFill(MemAddr),
    DataDrop(DataAddr),

    // > Reference Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#reference-instructions>
    RefNull(RefType),
    RefFunc(FuncAddr),
    RefIsNull => [S32 -> S32],
    RefAsNonNull => [S32 -> S32],
    RefEq => [S32 -> S32],
    RefTest(RefType),
    RefCast(RefType),

    // > Aggregate Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#aggregate-instructions>
    StructNew(TypeAddr),
    StructNewDefault(TypeAddr),
    StructGet(Operand64Idx<(u32, u32)>), StructGetS(Operand64Idx<(u32, u32)>), StructGetU(Operand64Idx<(u32, u32)>), StructSet(Operand64Idx<(u32, u32)>),
    ArrayNew(TypeAddr),
    ArrayNewDefault(TypeAddr),
    ArrayNewFixed(Operand64Idx<(u32, u32)>),
    ArrayNewData(Operand64Idx<(u32, u32)>), ArrayNewElem(Operand64Idx<(u32, u32)>),
    ArrayGet(TypeAddr),
    ArrayGetS(TypeAddr),
    ArrayGetU(TypeAddr),
    ArraySet(TypeAddr),
    ArrayLen,
    ArrayFill(TypeAddr),
    ArrayCopy(Operand64Idx<(u32, u32)>), ArrayInitData(Operand64Idx<(u32, u32)>), ArrayInitElem(Operand64Idx<(u32, u32)>),
    RefI31 => [S32 -> S32],
    I31GetS => [S32 -> S32],
    I31GetU => [S32 -> S32],

    // > Numeric Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#numeric-instructions>
    Const32(i32) => [stack -> S32] @value [Read(value) => ValueSource::Const32(value)] @preserve Const32Preserve,
    Const64Imm(i32) => [stack -> S64] @value [Read(value) => ValueSource::Const64Imm(value)] @preserve Const64ImmPreserve,
    Const64(Operand64Idx<i64>) => [stack -> S64] @value [Read(index) => ValueSource::Const64(index)] @preserve Const64Preserve,

    // Comparisons
    I32Eqz => [S32 -> S32], I32Eq => [S32 -> S32], I32Ne => [S32 -> S32], I32LtS => [S32 -> S32], I32LtU => [S32 -> S32], I32GtS => [S32 -> S32], I32GtU => [S32 -> S32], I32LeS => [S32 -> S32], I32LeU => [S32 -> S32], I32GeS => [S32 -> S32], I32GeU => [S32 -> S32],
    I64Eqz => [S64 -> S32], I64Eq => [S64 -> S32], I64Ne => [S64 -> S32], I64LtS => [S64 -> S32], I64LtU => [S64 -> S32], I64GtS => [S64 -> S32], I64GtU => [S64 -> S32], I64LeS => [S64 -> S32], I64LeU => [S64 -> S32], I64GeS => [S64 -> S32], I64GeU => [S64 -> S32],

    F32Eq => [S32 -> S32], F32Ne => [S32 -> S32], F32Lt => [S32 -> S32], F32Gt => [S32 -> S32], F32Le => [S32 -> S32], F32Ge => [S32 -> S32],
    F64Eq => [S64 -> S32], F64Ne => [S64 -> S32], F64Lt => [S64 -> S32], F64Gt => [S64 -> S32], F64Le => [S64 -> S32], F64Ge => [S64 -> S32],

    // Integer arithmetic
    I32Clz => [stack S32 -> S32] @value [NoLocalWrites], I32Ctz => [stack S32 -> S32] @value [NoLocalWrites], I32Popcnt => [stack S32 -> S32] @value [NoLocalWrites],
    I32Add => [stack S32, S32 -> S32] @value [NoLocalWrites], I32Sub => [stack S32, S32 -> S32] @value [NoLocalWrites], I32Mul => [stack S32, S32 -> S32] @value [NoLocalWrites],
    I32DivS => [stack S32, S32 -> S32] @value [NoLocalWrites], I32DivU => [stack S32, S32 -> S32] @value [NoLocalWrites], I32RemS => [stack S32, S32 -> S32] @value [NoLocalWrites], I32RemU => [stack S32, S32 -> S32] @value [NoLocalWrites],
    I64Clz => [stack S64 -> S64] @value [NoLocalWrites], I64Ctz => [stack S64 -> S64] @value [NoLocalWrites], I64Popcnt => [stack S64 -> S64] @value [NoLocalWrites],
    I64Add => [stack S64, S64 -> S64] @value [NoLocalWrites], I64Sub => [stack S64, S64 -> S64] @value [NoLocalWrites], I64Mul => [stack S64, S64 -> S64] @value [NoLocalWrites],
    I64DivS => [stack S64, S64 -> S64] @value [NoLocalWrites], I64DivU => [stack S64, S64 -> S64] @value [NoLocalWrites], I64RemS => [stack S64, S64 -> S64] @value [NoLocalWrites], I64RemU => [stack S64, S64 -> S64] @value [NoLocalWrites],

    // Bitwise
    I32And => [stack S32, S32 -> S32] @value [NoLocalWrites], I32Or => [stack S32, S32 -> S32] @value [NoLocalWrites], I32Xor => [stack S32, S32 -> S32] @value [NoLocalWrites],
    I32Shl => [stack S32, S32 -> S32] @value [NoLocalWrites], I32ShrS => [stack S32, S32 -> S32] @value [NoLocalWrites], I32ShrU => [stack S32, S32 -> S32] @value [NoLocalWrites], I32Rotl => [stack S32, S32 -> S32] @value [NoLocalWrites], I32Rotr => [stack S32, S32 -> S32] @value [NoLocalWrites],
    I64And => [stack S64, S64 -> S64] @value [NoLocalWrites], I64Or => [stack S64, S64 -> S64] @value [NoLocalWrites], I64Xor => [stack S64, S64 -> S64] @value [NoLocalWrites],
    I64Shl => [stack S64, S64 -> S64] @value [NoLocalWrites], I64ShrS => [stack S64, S64 -> S64] @value [NoLocalWrites], I64ShrU => [stack S64, S64 -> S64] @value [NoLocalWrites], I64Rotl => [stack S64, S64 -> S64] @value [NoLocalWrites], I64Rotr => [stack S64, S64 -> S64] @value [NoLocalWrites],

    // Floating Point
    F32Abs => [S32 -> S32], F32Neg => [S32 -> S32], F32Ceil => [S32 -> S32], F32Floor => [S32 -> S32], F32Trunc => [S32 -> S32], F32Nearest => [S32 -> S32], F32Sqrt => [S32 -> S32], F32Add => [S32 -> S32], F32Sub => [S32 -> S32], F32Mul => [S32 -> S32], F32Div => [S32 -> S32], F32Min => [S32 -> S32], F32Max => [S32 -> S32], F32Copysign => [S32 -> S32],
    F64Abs => [S64 -> S64], F64Neg => [S64 -> S64], F64Ceil => [S64 -> S64], F64Floor => [S64 -> S64], F64Trunc => [S64 -> S64], F64Nearest => [S64 -> S64], F64Sqrt => [S64 -> S64], F64Add => [S64 -> S64], F64Sub => [S64 -> S64], F64Mul => [S64 -> S64], F64Div => [S64 -> S64], F64Min => [S64 -> S64], F64Max => [S64 -> S64], F64Copysign => [S64 -> S64],
    I32WrapI64 => [stack S64 -> S32] @value [NoLocalWrites], I32TruncF32S => [S32 -> S32], I32TruncF32U => [S32 -> S32], I32TruncF64S => [S64 -> S32], I32TruncF64U => [S64 -> S32], I32Extend8S => [S32 -> S32], I32Extend16S => [S32 -> S32],
    I64Extend8S => [S64 -> S64], I64Extend16S => [S64 -> S64], I64Extend32S => [S64 -> S64], I64ExtendI32S => [stack S32 -> S64] @value [NoLocalWrites], I64ExtendI32U => [stack S32 -> S64] @value [NoLocalWrites], I64TruncF32S => [S32 -> S64], I64TruncF32U => [S32 -> S64], I64TruncF64S => [S64 -> S64], I64TruncF64U => [S64 -> S64],
    F32ConvertI32S => [S32 -> S32], F32ConvertI32U => [S32 -> S32], F32ConvertI64S => [S64 -> S32], F32ConvertI64U => [S64 -> S32], F32DemoteF64 => [S64 -> S32],
    F64ConvertI32S => [S32 -> S64], F64ConvertI32U => [S32 -> S64], F64ConvertI64S => [S64 -> S64], F64ConvertI64U => [S64 -> S64], F64PromoteF32 => [S32 -> S64],

    // Saturating Float-to-Int Conversions
    I32TruncSatF32S => [S32 -> S32], I32TruncSatF32U => [S32 -> S32], I32TruncSatF64S => [S64 -> S32], I32TruncSatF64U => [S64 -> S32],
    I64TruncSatF32S => [S32 -> S64], I64TruncSatF32U => [S32 -> S64], I64TruncSatF64S => [S64 -> S64], I64TruncSatF64U => [S64 -> S64],

    // Wide arithmetic extension
    I64Add128 => [S64 -> S64], I64Sub128 => [S64 -> S64], I64MulWideS => [S64 -> S64], I64MulWideU => [S64 -> S64],

    // > Vector Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#vector-instructions>
    V128Load(MemoryArg) => [load S128], V128Load8x8S(MemoryArg) => [load S128], V128Load8x8U(MemoryArg) => [load S128],
    V128Load16x4S(MemoryArg) => [load S128], V128Load16x4U(MemoryArg) => [load S128], V128Load32x2S(MemoryArg) => [load S128], V128Load32x2U(MemoryArg) => [load S128],

    V128Load8Splat(MemoryArg) => [load S128], V128Load16Splat(MemoryArg) => [load S128], V128Load32Splat(MemoryArg) => [load S128], V128Load64Splat(MemoryArg) => [load S128],
    V128Load8Lane(MemoryLaneArg), V128Load16Lane(MemoryLaneArg), V128Load32Lane(MemoryLaneArg), V128Load64Lane(MemoryLaneArg),

    V128Load32Zero(MemoryArg) => [load S128], V128Load64Zero(MemoryArg) => [load S128],

    V128Store(MemoryArg) => [store S128], V128Store8Lane(MemoryLaneArg), V128Store16Lane(MemoryLaneArg), V128Store32Lane(MemoryLaneArg), V128Store64Lane(MemoryLaneArg),

    I8x16Shuffle(Operand128Idx<[u8; 16]>) => [S128 -> S128],
    Const128Imm(u32) => [-> S128] @value [Read(value) => ValueSource::Const128Imm(value)] @preserve Const128ImmPreserve,
    Const128(Operand128Idx<[u8; 16]>) => [-> S128] @value [Read(index) => ValueSource::Const128(index)] @preserve Const128Preserve,

    I8x16ExtractLaneS(u8) => [S128 -> S32], I8x16ExtractLaneU(u8) => [S128 -> S32], I8x16ReplaceLane(u8) => [S32, S128 -> S128],
    I16x8ExtractLaneS(u8) => [S128 -> S32], I16x8ExtractLaneU(u8) => [S128 -> S32], I16x8ReplaceLane(u8) => [S32, S128 -> S128],
    I32x4ExtractLane(u8) => [S128 -> S32], I32x4ReplaceLane(u8) => [S32, S128 -> S128],
    I64x2ExtractLane(u8) => [S128 -> S64], I64x2ReplaceLane(u8) => [S64, S128 -> S128],
    F32x4ExtractLane(u8) => [S128 -> S32], F32x4ReplaceLane(u8) => [S32, S128 -> S128],
    F64x2ExtractLane(u8) => [S128 -> S64], F64x2ReplaceLane(u8) => [S64, S128 -> S128],

    V128Not => [S128 -> S128], V128And => [S128 -> S128], V128AndNot => [S128 -> S128], V128Or => [S128 -> S128], V128Xor => [S128 -> S128], V128Bitselect => [S128 -> S128], V128AnyTrue => [S128 -> S32], I8x16Swizzle => [S128 -> S128],
    I8x16Splat => [S32 -> S128], I8x16Eq => [S128 -> S128], I8x16Ne => [S128 -> S128], I8x16LtS => [S128 -> S128], I8x16LtU => [S128 -> S128], I8x16GtS => [S128 -> S128], I8x16GtU => [S128 -> S128], I8x16LeS => [S128 -> S128], I8x16LeU => [S128 -> S128], I8x16GeS => [S128 -> S128], I8x16GeU => [S128 -> S128],
    I16x8Splat => [S32 -> S128], I16x8Eq => [S128 -> S128], I16x8Ne => [S128 -> S128], I16x8LtS => [S128 -> S128], I16x8LtU => [S128 -> S128], I16x8GtS => [S128 -> S128], I16x8GtU => [S128 -> S128], I16x8LeS => [S128 -> S128], I16x8LeU => [S128 -> S128], I16x8GeS => [S128 -> S128], I16x8GeU => [S128 -> S128],
    I32x4Splat => [S32 -> S128], I32x4Eq => [S128 -> S128], I32x4Ne => [S128 -> S128], I32x4LtS => [S128 -> S128], I32x4LtU => [S128 -> S128], I32x4GtS => [S128 -> S128], I32x4GtU => [S128 -> S128], I32x4LeS => [S128 -> S128], I32x4LeU => [S128 -> S128], I32x4GeS => [S128 -> S128], I32x4GeU => [S128 -> S128],
    I64x2Splat => [S64 -> S128], I64x2Eq => [S128 -> S128], I64x2Ne => [S128 -> S128], I64x2LtS => [S128 -> S128], I64x2GtS => [S128 -> S128], I64x2LeS => [S128 -> S128], I64x2GeS => [S128 -> S128],
    F32x4Splat => [S32 -> S128], F32x4Eq => [S128 -> S128], F32x4Ne => [S128 -> S128], F32x4Lt => [S128 -> S128], F32x4Gt => [S128 -> S128], F32x4Le => [S128 -> S128], F32x4Ge => [S128 -> S128],
    F64x2Splat => [S64 -> S128], F64x2Eq => [S128 -> S128], F64x2Ne => [S128 -> S128], F64x2Lt => [S128 -> S128], F64x2Gt => [S128 -> S128], F64x2Le => [S128 -> S128], F64x2Ge => [S128 -> S128],

    I8x16Abs => [S128 -> S128], I8x16Neg => [S128 -> S128], I8x16AllTrue => [S128 -> S32], I8x16Bitmask => [S128 -> S32], I8x16Shl => [S32, S128 -> S128], I8x16ShrS => [S32, S128 -> S128], I8x16ShrU => [S32, S128 -> S128], I8x16Add => [S128 -> S128], I8x16Sub => [S128 -> S128], I8x16MinS => [S128 -> S128], I8x16MinU => [S128 -> S128], I8x16MaxS => [S128 -> S128], I8x16MaxU => [S128 -> S128],
    I16x8Abs => [S128 -> S128], I16x8Neg => [S128 -> S128], I16x8AllTrue => [S128 -> S32], I16x8Bitmask => [S128 -> S32], I16x8Shl => [S32, S128 -> S128], I16x8ShrS => [S32, S128 -> S128], I16x8ShrU => [S32, S128 -> S128], I16x8Add => [S128 -> S128], I16x8Sub => [S128 -> S128], I16x8MinS => [S128 -> S128], I16x8MinU => [S128 -> S128], I16x8MaxS => [S128 -> S128], I16x8MaxU => [S128 -> S128],
    I32x4Abs => [S128 -> S128], I32x4Neg => [S128 -> S128], I32x4AllTrue => [S128 -> S32], I32x4Bitmask => [S128 -> S32], I32x4Shl => [S32, S128 -> S128], I32x4ShrS => [S32, S128 -> S128], I32x4ShrU => [S32, S128 -> S128], I32x4Add => [S128 -> S128], I32x4Sub => [S128 -> S128], I32x4MinS => [S128 -> S128], I32x4MinU => [S128 -> S128], I32x4MaxS => [S128 -> S128], I32x4MaxU => [S128 -> S128],
    I64x2Abs => [S128 -> S128], I64x2Neg => [S128 -> S128], I64x2AllTrue => [S128 -> S32], I64x2Bitmask => [S128 -> S32], I64x2Shl => [S32, S128 -> S128], I64x2ShrS => [S32, S128 -> S128], I64x2ShrU => [S32, S128 -> S128], I64x2Add => [S128 -> S128], I64x2Sub => [S128 -> S128], I64x2Mul => [S128 -> S128],

    I8x16NarrowI16x8S => [S128 -> S128], I8x16NarrowI16x8U => [S128 -> S128], I8x16AddSatS => [S128 -> S128], I8x16AddSatU => [S128 -> S128], I8x16SubSatS => [S128 -> S128], I8x16SubSatU => [S128 -> S128], I8x16AvgrU => [S128 -> S128],
    I16x8NarrowI32x4S => [S128 -> S128], I16x8NarrowI32x4U => [S128 -> S128], I16x8AddSatS => [S128 -> S128], I16x8AddSatU => [S128 -> S128], I16x8SubSatS => [S128 -> S128], I16x8SubSatU => [S128 -> S128], I16x8AvgrU => [S128 -> S128],

    I16x8ExtAddPairwiseI8x16S => [S128 -> S128], I16x8ExtAddPairwiseI8x16U => [S128 -> S128], I16x8Mul => [S128 -> S128],
    I32x4ExtAddPairwiseI16x8S => [S128 -> S128], I32x4ExtAddPairwiseI16x8U => [S128 -> S128], I32x4Mul => [S128 -> S128],

    I16x8ExtMulLowI8x16S => [S128 -> S128], I16x8ExtMulLowI8x16U => [S128 -> S128], I16x8ExtMulHighI8x16S => [S128 -> S128], I16x8ExtMulHighI8x16U => [S128 -> S128],
    I32x4ExtMulLowI16x8S => [S128 -> S128], I32x4ExtMulLowI16x8U => [S128 -> S128], I32x4ExtMulHighI16x8S => [S128 -> S128], I32x4ExtMulHighI16x8U => [S128 -> S128],
    I64x2ExtMulLowI32x4S => [S128 -> S128], I64x2ExtMulLowI32x4U => [S128 -> S128], I64x2ExtMulHighI32x4S => [S128 -> S128], I64x2ExtMulHighI32x4U => [S128 -> S128],

    I16x8ExtendLowI8x16S => [S128 -> S128], I16x8ExtendLowI8x16U => [S128 -> S128], I16x8ExtendHighI8x16S => [S128 -> S128], I16x8ExtendHighI8x16U => [S128 -> S128],
    I32x4ExtendLowI16x8S => [S128 -> S128], I32x4ExtendLowI16x8U => [S128 -> S128], I32x4ExtendHighI16x8S => [S128 -> S128], I32x4ExtendHighI16x8U => [S128 -> S128],
    I64x2ExtendLowI32x4S => [S128 -> S128], I64x2ExtendLowI32x4U => [S128 -> S128], I64x2ExtendHighI32x4S => [S128 -> S128], I64x2ExtendHighI32x4U => [S128 -> S128],

    I8x16Popcnt => [S128 -> S128], I16x8Q15MulrSatS => [S128 -> S128], I32x4DotI16x8S => [S128 -> S128],

    F32x4Ceil => [S128 -> S128], F32x4Floor => [S128 -> S128], F32x4Trunc => [S128 -> S128], F32x4Nearest => [S128 -> S128], F32x4Abs => [S128 -> S128], F32x4Neg => [S128 -> S128], F32x4Sqrt => [S128 -> S128], F32x4Add => [S128 -> S128], F32x4Sub => [S128 -> S128], F32x4Mul => [S128 -> S128], F32x4Div => [S128 -> S128], F32x4Min => [S128 -> S128], F32x4Max => [S128 -> S128], F32x4PMin => [S128 -> S128], F32x4PMax => [S128 -> S128],
    F64x2Ceil => [S128 -> S128], F64x2Floor => [S128 -> S128], F64x2Trunc => [S128 -> S128], F64x2Nearest => [S128 -> S128], F64x2Abs => [S128 -> S128], F64x2Neg => [S128 -> S128], F64x2Sqrt => [S128 -> S128], F64x2Add => [S128 -> S128], F64x2Sub => [S128 -> S128], F64x2Mul => [S128 -> S128], F64x2Div => [S128 -> S128], F64x2Min => [S128 -> S128], F64x2Max => [S128 -> S128], F64x2PMin => [S128 -> S128], F64x2PMax => [S128 -> S128],
    I32x4TruncSatF32x4S => [S128 -> S128], I32x4TruncSatF32x4U => [S128 -> S128],
    F32x4ConvertI32x4S => [S128 -> S128], F32x4ConvertI32x4U => [S128 -> S128],
    I32x4TruncSatF64x2SZero => [S128 -> S128], I32x4TruncSatF64x2UZero => [S128 -> S128],
    F64x2ConvertLowI32x4S => [S128 -> S128], F64x2ConvertLowI32x4U => [S128 -> S128],
    F32x4DemoteF64x2Zero => [S128 -> S128], F64x2PromoteLowF32x4 => [S128 -> S128],

    // Relaxed SIMD
    I8x16RelaxedSwizzle => [S128 -> S128],
    I32x4RelaxedTruncF32x4S => [S128 -> S128], I32x4RelaxedTruncF32x4U => [S128 -> S128],
    I32x4RelaxedTruncF64x2SZero => [S128 -> S128], I32x4RelaxedTruncF64x2UZero => [S128 -> S128],
    F32x4RelaxedMadd => [S128 -> S128], F32x4RelaxedNmadd => [S128 -> S128],
    F64x2RelaxedMadd => [S128 -> S128], F64x2RelaxedNmadd => [S128 -> S128],
    I8x16RelaxedLaneselect => [S128 -> S128], I16x8RelaxedLaneselect => [S128 -> S128],
    I32x4RelaxedLaneselect => [S128 -> S128], I64x2RelaxedLaneselect => [S128 -> S128],
    F32x4RelaxedMin => [S128 -> S128], F32x4RelaxedMax => [S128 -> S128],
    F64x2RelaxedMin => [S128 -> S128], F64x2RelaxedMax => [S128 -> S128],
    I16x8RelaxedQ15mulrS => [S128 -> S128],
    I16x8RelaxedDotI8x16I7x16S => [S128 -> S128],
    I32x4RelaxedDotI8x16I7x16AddS => [S128 -> S128],

    // > Atomic Instructions (threads extension)
    // See <https://webassembly.github.io/threads/core/binary/instructions.html#atomic-memory-instructions>
    Atomic(AtomicArg),
    AtomicWait(Operand128Idx<MemoryOperand>, AtomicWaitOp),
    AtomicFence,

    // > Specialized variable instructions
    GlobalTee32(GlobalAddr) => [S32 -> S32], GlobalTee64(GlobalAddr) => [S64 -> S64], GlobalTee128(GlobalAddr),
    LocalCopy32(LocalAddr, LocalAddr) => [->] @value [LocalWrite(_, local) => ValueLane::S32, LocalDestination::Inline(local)],
    LocalCopy64(LocalAddr, LocalAddr) => [->] @value [LocalWrite(_, local) => ValueLane::S64, LocalDestination::Inline(local)],
    LocalCopy128(LocalAddr, LocalAddr) => [->] @value [LocalWrite(_, local) => ValueLane::S128, LocalDestination::Inline(local)],
    LocalCopyPair32(u8, u8, u8, u8) => [->], LocalCopyPair64(u8, u8, u8, u8) => [->],
    SetLocalConst32(I32LocalArg) => [->] @value [LocalWrite(arg) => ValueLane::S32, LocalDestination::Inline(arg.local)],
    SetLocalConst64(PackedOp64<LocalAddr, i64>) => [->] @value [LocalWrite(arg) => ValueLane::S64, LocalDestination::Inline(arg.op)],
    SetLocalConst128(PackedOp128<LocalAddr, [u8; 16]>) => [->] @value [LocalWrite(arg) => ValueLane::S128, LocalDestination::Inline(arg.op)],
    LocalGet32Preserve(LocalAddr) => [S32 -> S32], LocalGet64Preserve(LocalAddr) => [S64 -> S64],
    Const32Preserve(i32) => [S32 -> S32], Const64ImmPreserve(i32) => [S64 -> S64], Const64Preserve(Operand64Idx<i64>) => [S64 -> S64],
    LocalGet128Preserve(LocalAddr) => [S128 -> S128], Const128ImmPreserve(u32) => [S128 -> S128], Const128Preserve(Operand128Idx<[u8; 16]>) => [S128 -> S128],

    // > Specialized arithmetic instructions
    I32AddConst(i32) => [stack S32 -> S32] @value [NoLocalWrites], I64AddConst(Operand64Idx<i64>) => [stack S64 -> S64] @value [NoLocalWrites],
    I32AndConst(i32) => [stack S32 -> S32] @value [NoLocalWrites], I32XorConst(i32) => [stack S32 -> S32] @value [NoLocalWrites], I32ShlConst(i32) => [stack S32 -> S32] @value [NoLocalWrites], I32ShrUConst(i32) => [stack S32 -> S32] @value [NoLocalWrites],
    I32AddLocal(LocalAddr) => [stack S32 -> S32] @value [NoLocalWrites], I32AddLocalConst(I32LocalArg) => [stack -> S32] @value [NoLocalWrites],
    I32AddLocalShl(LocalAddr, u8) => [S32 -> S32],
    I32AddLocalAndShl(PackedOp64<u8, (u16, u32)>) => [S32 -> S32], I64AddLocalAndShl(PackedOp128<u8, (u16, u64)>) => [S64 -> S64],
    BinOpLocal32(BinOp, LocalAddr) => [stack S32 -> S32] @value [NoLocalWrites], BinOpLocal64(BinOp, LocalAddr) => [stack S64 -> S64] @value [NoLocalWrites],
    BinOpConst32(BinOp, i32) => [stack S32 -> S32] @value [NoLocalWrites], BinOpConst64(PackedOp64<BinOp, i64>) => [stack S64 -> S64] @value [NoLocalWrites],
    BinOpLocalLocal32(BinOp, LocalAddr, LocalAddr) => [stack -> S32] @value [NoLocalWrites], BinOpLocalLocal64(BinOp, LocalAddr, LocalAddr) => [stack -> S64] @value [NoLocalWrites],
    BinOpLocalConst32(PackedOp64<BinOp, (u16, u32)>) => [stack -> S32] @value [NoLocalWrites], BinOpLocalConst64(PackedOp128<BinOp, (u16, u64)>) => [stack -> S64] @value [NoLocalWrites],
    BinOpLocalConstCompact32(BinOpLocalCompact32) => [stack -> S32] @value [NoLocalWrites],
    BinOpLocalConstRhs32(PackedOp64<(BinOp, BinOp), (u16, u32)>) => [stack S32 -> S32] @value [NoLocalWrites],
    BinOpLocalConstRhs64(PackedOp128<(BinOp, BinOp), (u16, u64)>) => [stack S64 -> S64] @value [NoLocalWrites],
    BinOpLocalLocalRhs32(BinOp, BinOp, LocalAddr, LocalAddr) => [stack S32 -> S32] @value [NoLocalWrites],
    BinOpLocalLocalRhs64(BinOp, BinOp, LocalAddr, LocalAddr) => [stack S64 -> S64] @value [NoLocalWrites],
    BinOpLocalSet32(BinOp, LocalAddr, LocalAddr) => [S32 ->] @value [LocalWrite(_, _, local) => ValueLane::S32, LocalDestination::Inline(local)],
    BinOpLocalSet64(BinOp, LocalAddr, LocalAddr) => [S64 ->] @value [LocalWrite(_, _, local) => ValueLane::S64, LocalDestination::Inline(local)],
    BinOpLocalTee32(BinOp, LocalAddr, LocalAddr) => [S32 -> S32] @value [LocalResult(_, _, local) => LocalDestination::Inline(local)] @discard BinOpLocalSet32,
    BinOpLocalTee64(BinOp, LocalAddr, LocalAddr) => [S64 -> S64] @value [LocalResult(_, _, local) => LocalDestination::Inline(local)] @discard BinOpLocalSet64,
    BinOpLocalLocalSet32(PackedOp64<BinOp, (u16, u16, u16)>) => [->] @value [LocalWrite(arg) => ValueLane::S32, LocalDestination::Third64(arg.index)],
    BinOpLocalLocalSet64(PackedOp64<BinOp, (u16, u16, u16)>) => [->] @value [LocalWrite(arg) => ValueLane::S64, LocalDestination::Third64(arg.index)],
    BinOpLocalLocalTee32(PackedOp64<BinOp, (u16, u16, u16)>) => [stack -> S32] @value [LocalResult(arg) => LocalDestination::Third64(arg.index)] @discard BinOpLocalLocalSet32,
    BinOpLocalLocalTee64(PackedOp64<BinOp, (u16, u16, u16)>) => [stack -> S64] @value [LocalResult(arg) => LocalDestination::Third64(arg.index)] @discard BinOpLocalLocalSet64,
    BinOpLocalConstSet32(PackedOp64<BinOp, (u16, u16, u32)>) => [->] @value [LocalWrite(arg) => ValueLane::S32, LocalDestination::Second64(arg.index)],
    BinOpLocalConstSet64(PackedOp128<BinOp, (u16, u16, u64)>) => [->] @value [LocalWrite(arg) => ValueLane::S64, LocalDestination::Second128(arg.index)],
    BinOpLocalConstTee32(PackedOp64<BinOp, (u16, u16, u32)>) => [stack -> S32] @value [LocalResult(arg) => LocalDestination::Second64(arg.index)] @discard BinOpLocalConstSet32,
    BinOpLocalConstTee64(PackedOp128<BinOp, (u16, u16, u64)>) => [stack -> S64] @value [LocalResult(arg) => LocalDestination::Second128(arg.index)] @discard BinOpLocalConstSet64,
    I32AddLocalSetConst(I32LocalArg) => [->] @value [LocalWrite(arg) => ValueLane::S32, LocalDestination::Inline(arg.local)],
    I64AddLocalSetConst(PackedOp64<LocalAddr, i64>) => [->] @value [LocalWrite(arg) => ValueLane::S64, LocalDestination::Inline(arg.op)],
    I32AndConstTee(I32LocalArg) => [S32 -> S32] @value [LocalResult(arg) => LocalDestination::Inline(arg.local)],
    I32AddConstTee(I32LocalArg) => [S32 -> S32] @value [LocalResult(arg) => LocalDestination::Inline(arg.local)],
    I64AndConstTee(PackedOp64<LocalAddr, i64>) => [S64 -> S64] @value [LocalResult(arg) => LocalDestination::Inline(arg.op)],
    I64AddConstTee(PackedOp64<LocalAddr, i64>) => [S64 -> S64] @value [LocalResult(arg) => LocalDestination::Inline(arg.op)],
    I32MulAddLocal(LocalAddr) => [S32 ->], I64MulAddLocal(LocalAddr) => [S64 ->],
    F32MulAddLocal(LocalAddr) => [S32 ->], F64MulAddLocal(LocalAddr) => [S64 ->],
    I32Add3 => [S32 -> S32], I64Add3 => [S64 -> S64],
    I32XorRotlConst(u8) => [stack S32, S32 -> S32] @value [NoLocalWrites],
    I32XorLocalRotlConst(LocalAddr, u8) => [stack S32 -> S32] @value [NoLocalWrites],
    I32XorLocalLocalRotlConst(LocalAddr, LocalAddr, u8) => [stack -> S32] @value [NoLocalWrites],
    I64XorRotlConst(u8) => [stack S64, S64 -> S64] @value [NoLocalWrites],
    I64XorLocalRotlConst(LocalAddr, u8) => [stack S64 -> S64] @value [NoLocalWrites],
    I64XorLocalLocalRotlConst(LocalAddr, LocalAddr, u8) => [stack -> S64] @value [NoLocalWrites],
    CmpLocalLocal32(CmpOp, LocalAddr, LocalAddr) => [-> S32], CmpLocalLocal64(CmpOp, LocalAddr, LocalAddr) => [-> S32],

    // > Specialized branch instructions
    JumpIfLocalZero32(TargetLocalArg) @value [NoLocalWrites] @target [inline arg => arg.target_ip], JumpIfLocalNonZero32(TargetLocalArg) @value [NoLocalWrites] @target [inline arg => arg.target_ip],
    JumpIfLocalZero64(TargetLocalArg) @value [NoLocalWrites] @target [inline arg => arg.target_ip], JumpIfLocalNonZero64(TargetLocalArg) @value [NoLocalWrites] @target [inline arg => arg.target_ip],
    JumpCmpAccConst32(PackedOp64<CmpOp, (u32, i32)>) => [branch S32] @value [NoLocalWrites] @target [pool64 packed => packed.index], JumpCmpAccConst64(PackedOp128<CmpOp, (u32, i64)>) => [branch S64] @value [NoLocalWrites] @target [pool128 packed => packed.index],
    JumpFCmpAccConst32(PackedOp64<(FloatCmpOp, bool), (u32, i32)>) => [branch S32] @value [NoLocalWrites] @target [pool64 packed => packed.index],
    JumpFCmpAccConst64(PackedOp128<(FloatCmpOp, bool), (u32, i64)>) => [branch S64] @value [NoLocalWrites] @target [pool128 packed => packed.index],
    JumpCmpAccLocal32(PackedOp64<CmpOp, (u32, u16)>) => [branch S32] @value [NoLocalWrites] @target [pool64 packed => packed.index], JumpCmpAccLocal64(PackedOp64<CmpOp, (u32, u16)>) => [branch S64] @value [NoLocalWrites] @target [pool64 packed => packed.index],
    JumpCmpLocalConst32(PackedOp128<CmpOp, (u32, i32, u16)>) @value [NoLocalWrites] @target [pool128 packed => packed.index], JumpCmpLocalConst64(PackedOp128<CmpOp, (u32, i64, u16)>) @value [NoLocalWrites] @target [pool128 packed => packed.index],
    JumpCmpLocalLocal32(PackedOp64<CmpOp, (u32, u16, u16)>) @value [NoLocalWrites] @target [pool64 packed => packed.index], JumpCmpLocalLocal64(PackedOp64<CmpOp, (u32, u16, u16)>) @value [NoLocalWrites] @target [pool64 packed => packed.index],
    BinOpLocalConstTestJump32(PackedOp128<BinOp, LocalUpdateOperand>) @target [pool128 packed => packed.index],
    BinOpLocalLocalTestJump32(PackedOp64<(BinOp, bool), (u32, u16, u16)>) @target [pool64 packed => packed.index],
    BinOpLocalConstJumpCmpConst32(PackedOp128<(BinOp, CmpOp), LocalExprCmpOperand>) @target [pool128 packed => packed.index],
    BinOpLocalConstSetJump32(PackedOp128<BinOp, LocalUpdateOperand>) @value [LocalWrite(packed) => ValueLane::S32, LocalDestination::Third128(Operand128Idx::new(packed.index.index() as u32))] @target [pool128 packed => packed.index],
    BinOpLocalConstSetJumpCmpLocal32(PackedOp128<(BinOp, CmpOp), LocalUpdateCmpOperand>) @value [LocalWrite(packed) => ValueLane::S32, LocalDestination::Third128(Operand128Idx::new(packed.index.index() as u32))] @target [pool128 packed => packed.index],
    BinOpLocalConstSetJumpCmpLocal64(PackedOp128<(BinOp, CmpOp), LocalUpdateCmpOperand64>) @value [LocalWrite(packed) => ValueLane::S64, LocalDestination::Third128Wide(Operand128Idx::new(packed.index.index() as u32))] @target [pool128 packed => packed.index],
    BinOpLocalLocalSetJumpCmpLocal32(PackedOp128<(BinOp, CmpOp), LocalBinOpCmpOperand>) @value [LocalWrite(packed) => ValueLane::S32, LocalDestination::Third128(Operand128Idx::new(packed.index.index() as u32))] @target [pool128 packed => packed.index],
    BinOpLocalLocalSetJumpCmpLocal64(PackedOp128<(BinOp, CmpOp), LocalBinOpCmpOperand>) @value [LocalWrite(packed) => ValueLane::S64, LocalDestination::Third128(Operand128Idx::new(packed.index.index() as u32))] @target [pool128 packed => packed.index],
    BinOpStackConstTeeLocalJump32(PackedOp128<BinOp, LocalUpdateOperand>) @target [pool128 packed => packed.index],
    BinOpGlobalConstSetJump32(PackedOp128<BinOp, GlobalUpdateOperand>) @target [pool128 packed => packed.index],
    I32AddLocalSetConstJump(Operand128Idx<LocalUpdateOperand>) @value [LocalWrite(index) => ValueLane::S32, LocalDestination::Third128(Operand128Idx::new(index.index() as u32))] @target [pool128 index => *index] @loop I32AddLocalSetConstLoop,
    I32AddStackConstTeeLocalJump(Operand128Idx<LocalUpdateOperand>) @target [pool128 index => *index] @loop I32AddStackConstTeeLocalLoop,
    I32AddGlobalSetConstJump(Operand128Idx<GlobalUpdateOperand>) @target [pool128 index => *index] @loop I32AddGlobalSetConstLoop,
    I32AddLocalSetConstJumpCmpLocal(PackedOp128<CmpOp, LocalUpdateCmpOperand>) @value [LocalWrite(packed) => ValueLane::S32, LocalDestination::Third128(Operand128Idx::new(packed.index.index() as u32))] @target [pool128 packed => packed.index],
    I32AddLocalSetConstLoop(Operand128Idx<LocalUpdateOperand>) @value [LocalWrite(index) => ValueLane::S32, LocalDestination::Third128(Operand128Idx::new(index.index() as u32))],
    I32AddStackConstTeeLocalLoop(Operand128Idx<LocalUpdateOperand>),
    I32AddGlobalSetConstLoop(Operand128Idx<GlobalUpdateOperand>),
    I32LoadLocalTeeJump(PackedOp128<bool, MemoryLocalJumpOperand>) @target [pool128 packed => packed.index],

    // > Specialized memory instructions
    IncMemoryLocal32(MemoryLocalArg) => [->] @value [NoLocalWrites], IncMemoryLocal64(MemoryLocalArg) => [->] @value [NoLocalWrites],
    I32StoreLocalLocal(MemoryLocalArg) => [->] @value [NoLocalWrites],
    I64StoreLocalLocal(MemoryLocalArg) => [->] @value [NoLocalWrites],
    V128StoreLocalLocal(MemoryLocalArg) => [->] @value [NoLocalWrites],
    I32Store8LocalLocal(MemoryLocalArg) => [->] @value [NoLocalWrites], I32Store16LocalLocal(MemoryLocalArg) => [->] @value [NoLocalWrites],
    I64Store8LocalLocal(MemoryLocalArg) => [->] @value [NoLocalWrites], I64Store16LocalLocal(MemoryLocalArg) => [->] @value [NoLocalWrites],
    I64Store32LocalLocal(MemoryLocalArg) => [->] @value [NoLocalWrites],
    I32LoadLocal(MemoryLocalArg) => [-> S32], I64LoadLocal(MemoryLocalArg) => [-> S64],
    I32LoadLocalAdd(MemoryLocalArg) => [-> S32], I64LoadLocalAdd(MemoryLocalArg) => [-> S64],
    I32Load8SLocalAdd(MemoryLocalArg) => [-> S32], I32Load8ULocalAdd(MemoryLocalArg) => [-> S32],
    I32Load16SLocalAdd(MemoryLocalArg) => [-> S32], I32Load16ULocalAdd(MemoryLocalArg) => [-> S32],
    I32LoadLocalCompact(MemoryLocalCompactArg) => [-> S32],
    I32Load8SLocal(MemoryLocalArg) => [-> S32], I32Load8ULocal(MemoryLocalArg) => [-> S32],
    I32Load16SLocal(MemoryLocalArg) => [-> S32], I32Load16ULocal(MemoryLocalArg) => [-> S32],
    I32LoadLocalTee(MemoryLocalArg) => [-> S32] @value [LocalResult(arg) => LocalDestination::Inline(arg.local2 as u16)] @discard I32LoadLocalSet,
    I32LoadLocalSet(MemoryLocalArg) => [->] @value [LocalWrite(arg) => ValueLane::S32, LocalDestination::Inline(arg.local2 as u16)],
    I32Load8SLocalTee(MemoryLocalArg) => [-> S32] @value [LocalResult(arg) => LocalDestination::Inline(arg.local2 as u16)] @discard I32Load8SLocalSet,
    I32Load8ULocalTee(MemoryLocalArg) => [-> S32] @value [LocalResult(arg) => LocalDestination::Inline(arg.local2 as u16)] @discard I32Load8ULocalSet,
    I32Load16SLocalTee(MemoryLocalArg) => [-> S32] @value [LocalResult(arg) => LocalDestination::Inline(arg.local2 as u16)] @discard I32Load16SLocalSet,
    I32Load16ULocalTee(MemoryLocalArg) => [-> S32] @value [LocalResult(arg) => LocalDestination::Inline(arg.local2 as u16)] @discard I32Load16ULocalSet,
    I32Load8SLocalSet(MemoryLocalArg) => [->] @value [LocalWrite(arg) => ValueLane::S32, LocalDestination::Inline(arg.local2 as u16)],
    I32Load8ULocalSet(MemoryLocalArg) => [->] @value [LocalWrite(arg) => ValueLane::S32, LocalDestination::Inline(arg.local2 as u16)],
    I32Load16SLocalSet(MemoryLocalArg) => [->] @value [LocalWrite(arg) => ValueLane::S32, LocalDestination::Inline(arg.local2 as u16)],
    I32Load16ULocalSet(MemoryLocalArg) => [->] @value [LocalWrite(arg) => ValueLane::S32, LocalDestination::Inline(arg.local2 as u16)],
    V128LoadLocalTee(MemoryLocalArg) => [-> S128] @value [LocalResult(arg) => LocalDestination::Inline(arg.local2 as u16)] @discard V128LoadLocalSet,
    V128LoadLocalSet(MemoryLocalArg) => [->] @value [LocalWrite(arg) => ValueLane::S128, LocalDestination::Inline(arg.local2 as u16)],
    LoadLocalBinOp32(PackedOp64<(LoadKind32, BinOp), (u8, u16, u32)>) => [-> S32],
    BinOpLoadLocalRhs32(PackedOp64<(LoadKind32, BinOp), (u8, u16, u32)>) => [S32 -> S32],
    F64AddLoadLocal(MemoryLocalArg) => [stack S64 -> S64] @value [NoLocalWrites],
    F64SubLoadLocal(MemoryLocalArg) => [stack S64 -> S64] @value [NoLocalWrites],
    UpdateMemoryLocal32(PackedOp128<MemoryUpdateOp, MemoryUpdateOperand>) => [->],
    SelectStore32(MemoryArg) => [store S32], SelectStore64(Operand128Idx<MemoryOperand>) => [S32, S64 ->],
    F32MulAddStore(MemoryArg) => [store S32], F64MulAddStore(MemoryArg) => [store S64],
    MemoryFillConst(Operand128Idx<MemoryFillOperand>),

    // > Specialized SIMD instructions
    // Rotates each i64 lane of the vector stack top, the count is masked to 63.
    I64x2RotlConst(u8) => [S128 -> S128],

    // > Lowering-only instructions
    Checkpoint,
    Spill32 => [S32 ->], Spill64 => [S64 ->], Spill128 => [S128 ->],
    Reload32 => [-> S32], Reload64 => [-> S64], Reload128 => [-> S128],
    I32Load8ULocalAddCompact(u16, u8, u8) => [-> S32],
    CopyByteLocalAdd(u8, u8, u8) => [->] @value [NoLocalWrites],
    I32StoreLocalCompact(u16, u8, u8) => [->] @value [NoLocalWrites],
    I64StoreLocalCompact(u16, u8, u8) => [->] @value [NoLocalWrites],
    I64LoadLocalTee(MemoryLocalArg) => [-> S64] @value [LocalResult(arg) => LocalDestination::Inline(arg.local2 as u16)] @discard I64LoadLocalSet,
    I64LoadLocalSet(MemoryLocalArg) => [->] @value [LocalWrite(arg) => ValueLane::S64, LocalDestination::Inline(arg.local2 as u16)],
}

const _: () = assert!(core::mem::size_of::<Instruction>() == 8);

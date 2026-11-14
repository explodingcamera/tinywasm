use super::num;
use super::stack::{CallFrame, ValueStack};
use crate::store::Globals;
use crate::{Error, Result, Store, WasmValue, interpreter::simd::Value128};
use tinywasm_types::{BinOp, CmpOp, FloatCmpOp, GlobalAddr, LocalAddr, WasmType};

pub(crate) type Value32 = u32;
pub(crate) type Value64 = u64;

/// Raw scalar argument and result tops at call boundaries.
///
/// Vector tops remain on the value stack to keep this common call payload small.
/// Reference bits must be decoded into rooted host values before invoking a callback.
#[derive(Clone, Copy, Default)]
pub(crate) struct ScalarValues {
    /// Raw bits of the signature's last 32-bit value, or unused when absent.
    pub(crate) acc32: u32,
    /// Raw bits of the signature's last 64-bit value, or unused when absent.
    pub(crate) acc64: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
/// Packed internal representation of a WebAssembly reference.
///
/// Unlike the public [`RefValue`], this stores no explicit reference category.
/// Converting it back therefore requires the value's canonical reference type.
pub(crate) struct ValueRef(u32);

impl ValueRef {
    const HOST_ANY_TAG: u32 = 1 << 30;
    pub(crate) const NULL: Self = Self(0);

    #[inline]
    pub(crate) const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[inline]
    pub(crate) const fn from_category_addr(addr: u32) -> Self {
        let Some(raw) = addr.checked_add(1) else { panic!("reference address does not fit in the runtime encoding") };
        let Some(raw) = raw.checked_mul(2) else { panic!("reference address does not fit in the runtime encoding") };
        Self(raw)
    }

    #[inline]
    pub(crate) const fn from_i31(value: i32) -> Self {
        Self(((value as u32) << 1) | 1)
    }

    #[inline]
    pub(crate) const fn try_from_host_any(addr: u32) -> Option<Self> {
        if addr >= Self::HOST_ANY_TAG - 1 {
            return None;
        }
        Some(Self::from_category_addr(addr | Self::HOST_ANY_TAG))
    }

    pub(crate) const fn is_host_any(self) -> bool {
        matches!(self.addr(), Some(addr) if addr & Self::HOST_ANY_TAG != 0)
    }

    #[inline]
    pub(crate) const fn addr(self) -> Option<u32> {
        if self.is_null() || self.is_i31() { None } else { Some(self.0 / 2 - 1) }
    }

    #[inline]
    pub(crate) const fn is_null(self) -> bool {
        self.0 == Self::NULL.0
    }

    #[inline]
    pub(crate) const fn is_i31(self) -> bool {
        self.0 & 1 != 0
    }

    #[inline]
    pub(crate) const fn i31_s(self) -> Option<i32> {
        if self.is_i31() { Some((self.0 as i32) >> 1) } else { None }
    }

    #[inline]
    pub(crate) const fn i31_u(self) -> Option<u32> {
        if self.is_i31() { Some(self.0 >> 1) } else { None }
    }

    #[inline]
    pub(crate) const fn raw(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// An untyped internal WebAssembly value.
pub(crate) enum RuntimeValue {
    /// A 32-bit value.
    Value32(Value32),
    /// A 64-bit value.
    Value64(Value64),
    /// A 128-bit value.
    Value128(Value128),
    /// A reference value.
    ValueRef(ValueRef),
}

impl RuntimeValue {
    pub(crate) fn into_wasm(self, store: &mut Store, ty: WasmType) -> Result<WasmValue> {
        Ok(match (self, ty) {
            (Self::Value32(value), WasmType::I32) => WasmValue::I32(value as i32),
            (Self::Value32(value), WasmType::F32) => WasmValue::F32(f32::from_bits(value)),
            (Self::Value64(value), WasmType::I64) => WasmValue::I64(value as i64),
            (Self::Value64(value), WasmType::F64) => WasmValue::F64(f64::from_bits(value)),
            (Self::Value128(value), WasmType::V128) => WasmValue::V128(value.0),
            (Self::ValueRef(value), WasmType::Ref(ty)) => WasmValue::Ref(store.decode_ref(value, ty)?),
            _ => return Err(Error::other("internal value does not match its WebAssembly type")),
        })
    }
}

/// Access to locals, globals, and materialized stack values.
pub(crate) trait InternalValue: Copy + Default {
    fn stack_push(stack: &mut ValueStack, value: Self);
    fn stack_pop(stack: &mut ValueStack) -> Self;
    fn stack_peek(stack: &ValueStack) -> Self;
    fn stack_update(stack: &mut ValueStack, f: impl FnOnce(Self) -> Self) -> Self;
    fn local_get(stack: &ValueStack, frame: &CallFrame, index: LocalAddr) -> Self;
    fn local_set(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, value: Self);
    fn local_copy(stack: &mut ValueStack, frame: &CallFrame, from: LocalAddr, to: LocalAddr);
    fn global_get(globals: &Globals, addr: GlobalAddr) -> Self;
    fn global_set(globals: &mut Globals, addr: GlobalAddr, value: Self);

    #[inline(always)]
    fn local_update(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, f: impl FnOnce(Self) -> Self) -> Self {
        let value = f(Self::local_get(stack, frame, index));
        Self::local_set(stack, frame, index, value);
        value
    }
}

macro_rules! impl_internalvalue {
    (
        $(
            $stack:ident, $stack_base:ident, $global_get:ident, $global_set:ident, $outer:ty,
            |$to_stack_v:ident| $to_stack:expr,
            |$from_stack_v:ident| $from_stack:expr
        )*
    ) => {
        $(
            impl InternalValue for $outer {
                #[inline(always)]
                fn local_get(stack: &ValueStack, frame: &CallFrame, index: LocalAddr) -> Self {
                    let $from_stack_v =
                        *stack.$stack.get_local(frame.locals_base.$stack_base as usize + index as usize);
                    $from_stack
                }

                #[inline(always)]
                fn local_set(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, value: Self) {
                    let $to_stack_v = value;
                    let abs_index = frame.locals_base.$stack_base as usize + index as usize;
                    stack.$stack.set_local(abs_index, $to_stack);
                }

                #[inline(always)]
                fn local_copy(stack: &mut ValueStack, frame: &CallFrame, from: LocalAddr, to: LocalAddr) {
                    let from = frame.locals_base.$stack_base as usize + from as usize;
                    let to = frame.locals_base.$stack_base as usize + to as usize;
                    let value = *stack.$stack.get_local(from);
                    stack.$stack.set_local(to, value);
                }

                #[inline(always)]
                fn global_get(globals: &Globals, addr: GlobalAddr) -> Self {
                    let $from_stack_v = globals.$global_get(addr);
                    $from_stack
                }

                #[inline(always)]
                fn global_set(globals: &mut Globals, addr: GlobalAddr, value: Self) {
                    let $to_stack_v = value;
                    globals.$global_set(addr, $to_stack);
                }

                #[inline(always)]
                fn stack_push(stack: &mut ValueStack, value: Self) {
                    let $to_stack_v = value;
                    stack.$stack.push($to_stack);
                }

                #[inline(always)]
                fn stack_pop(stack: &mut ValueStack) -> Self {
                    let $from_stack_v = stack.$stack.pop();
                    $from_stack
                }

                #[inline(always)]
                fn stack_peek(stack: &ValueStack) -> Self {
                    let $from_stack_v = *stack.$stack.last();
                    $from_stack
                }

                #[inline(always)]
                fn stack_update(stack: &mut ValueStack, f: impl FnOnce(Self) -> Self) -> Self {
                    let index = stack.$stack.len() - 1;
                    let $from_stack_v = *stack.$stack.get(index);
                    let value = f($from_stack);
                    let $to_stack_v = value;
                    stack.$stack.set(index, $to_stack);
                    value
                }

            }
        )*
    };
}

impl_internalvalue! {
    stack_32,  s32,  get_32,  set_32,  u32,      |v| v,               |v| v
    stack_64,  s64,  get_64,  set_64,  u64,      |v| v,               |v| v
    stack_32,  s32,  get_32,  set_32,  i32,      |v| v as u32,        |v| v as i32
    stack_64,  s64,  get_64,  set_64,  i64,      |v| v as u64,        |v| v as i64
    stack_32,  s32,  get_32,  set_32,  f32,      |v| f32::to_bits(v), |v| f32::from_bits(v)
    stack_64,  s64,  get_64,  set_64,  f64,      |v| f64::to_bits(v), |v| f64::from_bits(v)
    stack_32,  s32,  get_32,  set_32,  ValueRef, |v| v.raw(),         |v| ValueRef(v)
    stack_128, s128, get_128, set_128, Value128, |v| v,               |v| v
}

pub(crate) trait BinOpExt<T>: Copy {
    fn exec(self, lhs: T, rhs: T) -> T;
}

pub(crate) trait CmpOpExt<T>: Copy {
    fn cmp(self, lhs: T, rhs: T) -> bool;
}

macro_rules! impl_value_ops {
    ($unsigned:ty, $signed:ty, $float:ident) => {
        impl BinOpExt<$unsigned> for BinOp {
            #[inline(always)]
            fn exec(self, lhs: $unsigned, rhs: $unsigned) -> $unsigned {
                match self {
                    BinOp::IAdd => lhs.wrapping_add(rhs),
                    BinOp::ISub => lhs.wrapping_sub(rhs),
                    BinOp::IMul => lhs.wrapping_mul(rhs),
                    BinOp::IAnd => lhs & rhs,
                    BinOp::IOr => lhs | rhs,
                    BinOp::IXor => lhs ^ rhs,
                    BinOp::IShl => (lhs as $signed).wrapping_shl(rhs as u32) as $unsigned,
                    BinOp::IShrS => (lhs as $signed).wrapping_shr(rhs as u32) as $unsigned,
                    BinOp::IShrU => lhs.wrapping_shr(rhs as u32),
                    BinOp::IRotl => (lhs as $signed).rotate_left(rhs as u32) as $unsigned,
                    BinOp::IRotr => (lhs as $signed).rotate_right(rhs as u32) as $unsigned,
                    BinOp::FAdd => (<$float>::from_bits(lhs) + <$float>::from_bits(rhs)).to_bits(),
                    BinOp::FSub => (<$float>::from_bits(lhs) - <$float>::from_bits(rhs)).to_bits(),
                    BinOp::FMul => (<$float>::from_bits(lhs) * <$float>::from_bits(rhs)).to_bits(),
                    BinOp::FDiv => (<$float>::from_bits(lhs) / <$float>::from_bits(rhs)).to_bits(),
                    BinOp::FMin => num::$float::minimum(<$float>::from_bits(lhs), <$float>::from_bits(rhs)).to_bits(),
                    BinOp::FMax => num::$float::maximum(<$float>::from_bits(lhs), <$float>::from_bits(rhs)).to_bits(),
                    BinOp::FCopysign => <$float>::from_bits(lhs).copysign(<$float>::from_bits(rhs)).to_bits(),
                }
            }
        }

        impl CmpOpExt<$signed> for CmpOp {
            #[inline(always)]
            fn cmp(self, lhs: $signed, rhs: $signed) -> bool {
                match self {
                    CmpOp::Eq => lhs == rhs,
                    CmpOp::Ne => lhs != rhs,
                    CmpOp::LtS => lhs < rhs,
                    CmpOp::LtU => (lhs as $unsigned) < (rhs as $unsigned),
                    CmpOp::GtS => lhs > rhs,
                    CmpOp::GtU => (lhs as $unsigned) > (rhs as $unsigned),
                    CmpOp::LeS => lhs <= rhs,
                    CmpOp::LeU => (lhs as $unsigned) <= (rhs as $unsigned),
                    CmpOp::GeS => lhs >= rhs,
                    CmpOp::GeU => (lhs as $unsigned) >= (rhs as $unsigned),
                }
            }
        }

        impl CmpOpExt<$float> for FloatCmpOp {
            #[inline(always)]
            fn cmp(self, lhs: $float, rhs: $float) -> bool {
                match self {
                    Self::Eq => lhs == rhs,
                    Self::Ne => lhs != rhs,
                    Self::Lt => lhs < rhs,
                    Self::Gt => lhs > rhs,
                    Self::Le => lhs <= rhs,
                    Self::Ge => lhs >= rhs,
                }
            }
        }
    };
}

impl_value_ops!(Value32, i32, f32);
impl_value_ops!(Value64, i64, f64);

#[cfg(test)]
mod tests {
    use super::ValueRef;

    #[test]
    fn value_ref_remains_four_bytes() {
        assert_eq!(core::mem::size_of::<ValueRef>(), 4);
    }
}

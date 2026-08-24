use super::stack::{CallFrame, ValueStack};
use crate::store::Globals;
use crate::{Error, Result, Store, WasmValue, interpreter::simd::Value128};
use tinywasm_types::{GlobalAddr, LocalAddr, WasmType};

pub(crate) type Value32 = u32;
pub(crate) type Value64 = u64;

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

mod sealed {
    #[expect(unreachable_pub)]
    pub trait Sealed {}
}

/// Typed access to values in their physical [`ValueStack`] and [`Globals`] lanes.
pub(crate) trait InternalValue: sealed::Sealed + Copy + Default {
    fn stack_push(stack: &mut ValueStack, value: Self) -> Result<(), crate::Trap>;
    fn stack_pop(stack: &mut ValueStack) -> Self;
    fn stack_peek(stack: &ValueStack) -> Self;
    fn stack_select(stack: &mut ValueStack);
    fn local_get(stack: &ValueStack, frame: &CallFrame, index: LocalAddr) -> Self;
    fn local_push(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr) -> Result<(), crate::Trap>;
    fn local_set(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, value: Self);
    fn local_update(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, f: impl FnOnce(Self) -> Self);
    fn local_copy(stack: &mut ValueStack, frame: &CallFrame, from: LocalAddr, to: LocalAddr);
    fn global_get(globals: &Globals, addr: GlobalAddr) -> Self;
    fn global_set(globals: &mut Globals, addr: GlobalAddr, value: Self);
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
            impl sealed::Sealed for $outer {}

            impl InternalValue for $outer {
                #[inline(always)]
                fn stack_push(stack: &mut ValueStack, value: Self) -> Result<(), crate::Trap> {
                    let $to_stack_v = value;
                    cold_err!(stack.$stack.push($to_stack))?;
                    Ok(())
                }

                #[inline(always)]
                fn local_get(stack: &ValueStack, frame: &CallFrame, index: LocalAddr) -> Self {
                    let $from_stack_v =
                        *stack.$stack.get(frame.locals_base.$stack_base.wrapping_add(u32::from(index)) as usize);
                    $from_stack
                }

                #[inline(always)]
                fn local_push(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr) -> Result<(), crate::Trap> {
                    stack.$stack.push_copy(frame.locals_base.$stack_base.wrapping_add(u32::from(index)) as usize)
                }

                #[inline(always)]
                fn local_set(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, value: Self) {
                    let $to_stack_v = value;
                    let abs_index = frame.locals_base.$stack_base.wrapping_add(u32::from(index)) as usize;
                    stack.$stack.set(abs_index, $to_stack);
                }

                #[inline(always)]
                fn local_update(
                    stack: &mut ValueStack,
                    frame: &CallFrame,
                    index: LocalAddr,
                    f: impl FnOnce(Self) -> Self,
                ) {
                    let abs_index = frame.locals_base.$stack_base.wrapping_add(u32::from(index)) as usize;
                    let $from_stack_v = *stack.$stack.get(abs_index);
                    let $to_stack_v = f($from_stack);
                    stack.$stack.set(abs_index, $to_stack);
                }

                #[inline(always)]
                fn local_copy(stack: &mut ValueStack, frame: &CallFrame, from: LocalAddr, to: LocalAddr) {
                    let from = frame.locals_base.$stack_base.wrapping_add(u32::from(from)) as usize;
                    let to = frame.locals_base.$stack_base.wrapping_add(u32::from(to)) as usize;
                    stack.$stack.copy(from, to);
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
                fn stack_select(stack: &mut ValueStack) {
                    let cond = stack.stack_32.pop() as i32;
                    let val2 = stack.$stack.pop();

                    if cond == 0 {
                        let val1 = stack.$stack.len() - 1;
                        stack.$stack.set(val1, val2);
                    }
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

#[cfg(test)]
mod tests {
    use super::ValueRef;

    #[test]
    fn value_ref_remains_four_bytes() {
        assert_eq!(core::mem::size_of::<ValueRef>(), 4);
    }
}

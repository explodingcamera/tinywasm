//! Host-call conversion without materializing scalar lane tops on the value stack.

use super::*;
use crate::interpreter::{ScalarValues, ValueRef};

/// Reads earlier call values from the stack and scalar lane tops from an owned payload.
pub(crate) struct CallValueIter<'a> {
    store: &'a mut Store,
    type_addr: TypeAddr,
    types: FuncValueTypes,
    index: StackBase,
    position: usize,
    len: usize,
    last: [usize; 2],
    scalars: ScalarValues,
}

impl Iterator for CallValueIter<'_> {
    type Item = WasmValue;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position == self.len {
            return None;
        }
        let ty = self.store.state.get_canonical_func_type(self.type_addr);
        let ty = match self.types {
            FuncValueTypes::Params => ty.params()[self.position],
            FuncValueTypes::Results => ty.results()[self.position],
        };
        let value = if self.position == self.last[0] {
            let value = match ty {
                WasmType::Ref(_) => RuntimeValue::ValueRef(ValueRef::from_raw(self.scalars.acc32)),
                _ => RuntimeValue::Value32(self.scalars.acc32),
            };
            value.into_wasm(self.store, ty)
        } else if self.position == self.last[1] {
            RuntimeValue::Value64(self.scalars.acc64).into_wasm(self.store, ty)
        } else {
            self.store.stack_value(ty, &mut self.index)
        };
        self.position += 1;
        Some(value.expect_unreachable("invalid typed host value"))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len - self.position;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for CallValueIter<'_> {}

impl Store {
    /// Finds a call's argument base excluding the two accumulator-held scalar tops.
    pub(crate) fn call_parameter_base(&self, type_addr: TypeAddr) -> StackBase {
        let mut counts: ValueCounts = self.state.get_canonical_func_type(type_addr).params().iter().collect();
        counts.c32 = counts.c32.saturating_sub(1);
        counts.c64 = counts.c64.saturating_sub(1);
        self.value_stack.base_before(counts)
    }

    /// Roots references while reading call values from the stack and owned scalar payload.
    pub(crate) fn call_value_iter(
        &mut self,
        type_addr: TypeAddr,
        types: FuncValueTypes,
        index: StackBase,
        scalars: ScalarValues,
    ) -> Result<CallValueIter<'_>, Trap> {
        let ty = self.state.get_canonical_func_type(type_addr);
        let value_types = match types {
            FuncValueTypes::Params => ty.params(),
            FuncValueTypes::Results => ty.results(),
        };
        let len = value_types.len();
        let last = [
            value_types.iter().rposition(|ty| ValueLane::from(ty) == ValueLane::S32).unwrap_or(usize::MAX),
            value_types.iter().rposition(|ty| ValueLane::from(ty) == ValueLane::S64).unwrap_or(usize::MAX),
        ];
        let references = value_types.iter().filter(|ty| matches!(ty, WasmType::Ref(_))).count();
        if references != 0 {
            self.state.roots.reserve(references)?;
        }
        Ok(CallValueIter { store: self, type_addr, types, index, position: 0, len, last, scalars })
    }

    /// Validates call values, retaining the last scalar in each lane outside the stack.
    pub(crate) fn push_call_values<const RESULTS: bool>(
        &mut self,
        type_addr: TypeAddr,
        mut values: impl Iterator<Item = WasmValue>,
        base: StackBase,
    ) -> Result<ScalarValues> {
        let result = (|| {
            let ty = self.state.get_canonical_func_type(type_addr);
            let expected = if RESULTS { ty.results() } else { ty.params() };
            let last = [
                expected.iter().rposition(|ty| ValueLane::from(ty) == ValueLane::S32).unwrap_or(usize::MAX),
                expected.iter().rposition(|ty| ValueLane::from(ty) == ValueLane::S64).unwrap_or(usize::MAX),
            ];
            let mut scalars = ScalarValues::default();
            for (position, &ty) in expected.iter().enumerate() {
                let value = values.next().ok_or_else(|| Error::other("not enough typed function values"))?;
                let internal = value.to_runtime(self)?;
                if !self.value_matches_type(&value, ty) {
                    return Err(Error::other("typed function value does not match its signature"));
                }
                if position == last[0] {
                    scalars.acc32 = match internal {
                        RuntimeValue::Value32(value) => value,
                        RuntimeValue::ValueRef(value) => value.raw(),
                        _ => unreachable!("invalid 32-bit call value"),
                    };
                } else if position == last[1] {
                    let RuntimeValue::Value64(value) = internal else { unreachable!("invalid 64-bit call value") };
                    scalars.acc64 = value;
                } else {
                    self.value_stack.push_dyn(internal)?;
                }
            }
            if values.next().is_some() {
                return Err(Error::other("too many typed function values"));
            }
            Ok(scalars)
        })();
        if result.is_err() {
            self.value_stack.truncate_to_base(base);
        }
        result
    }
}

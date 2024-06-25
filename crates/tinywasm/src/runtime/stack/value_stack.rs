use alloc::vec::Vec;
use tinywasm_types::{ValType, WasmValue};

use crate::{Error, Result};

pub(crate) const VALUE_STACK_SIZE: usize = 1024 * 128;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct RawWasmValue<const N: usize = 16>(pub(crate) [u8; N]);

impl<const N: usize> From<&[u8]> for RawWasmValue<N> {
    fn from(bytes: &[u8]) -> Self {
        assert!(bytes.len() != N, "Invalid byte length");
        let mut value = [0; N];
        value.copy_from_slice(bytes);
        Self(value)
    }
}

pub(crate) fn wasmvalue_to_bytes(value: WasmValue, mut cb: impl FnMut(&[u8])) {
    match value {
        WasmValue::I32(v) => cb(&v.to_le_bytes()),
        WasmValue::I64(v) => cb(&v.to_le_bytes()),
        WasmValue::F32(v) => cb(&v.to_le_bytes()),
        WasmValue::F64(v) => cb(&v.to_le_bytes()),
        WasmValue::V128(v) => cb(&v.to_le_bytes()),
        WasmValue::RefExtern(v) => cb(&(v as i64).to_le_bytes()),
        WasmValue::RefFunc(v) => cb(&(v as i64).to_le_bytes()),
        WasmValue::RefNull(_) => cb(&(-1i64).to_le_bytes()),
    }
}

pub(crate) fn wasmvalue_to_localvalue(value: WasmValue) -> [u8; 16] {
    let mut res = [0; 16];
    match value {
        WasmValue::I32(v) => res.copy_from_slice(&v.to_le_bytes()),
        WasmValue::I64(v) => res.copy_from_slice(&v.to_le_bytes()),
        WasmValue::F32(v) => res.copy_from_slice(&v.to_le_bytes()),
        WasmValue::F64(v) => res.copy_from_slice(&v.to_le_bytes()),
        WasmValue::V128(v) => res.copy_from_slice(&v.to_le_bytes()),
        WasmValue::RefExtern(v) => res.copy_from_slice(&(v as i64).to_le_bytes()),
        WasmValue::RefFunc(v) => res.copy_from_slice(&(v as i64).to_le_bytes()),
        WasmValue::RefNull(_) => res.copy_from_slice(&(-1i64).to_le_bytes()),
    }
    res
}

pub(crate) fn bytes_to_localvalues(bytes: &[u8], tys: &[ValType]) -> Vec<[u8; 16]> {
    let mut values = Vec::with_capacity(tys.len());
    let mut offset = 0;
    for ty in tys {
        match ty.size() {
            4 | 8 | 16 => {
                let mut value = [0; 16];
                value[..ty.size()].copy_from_slice(&bytes[offset..offset + ty.size()]);
                values.push(value);
            }
            _ => unreachable!(),
        }
        offset += 16;
    }
    values
}

pub(crate) fn bytes_to_wasmvalues(bytes: &[u8], tys: &[ValType]) -> Vec<WasmValue> {
    let mut values = Vec::with_capacity(tys.len());
    let mut offset = 0;
    for ty in tys {
        let (value, new_offset) = match ty {
            ValType::I32 => (WasmValue::I32(i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())), 4),
            ValType::I64 => (WasmValue::I64(i64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())), 8),
            ValType::F32 => (WasmValue::F32(f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())), 4),
            ValType::F64 => (WasmValue::F64(f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())), 8),
            ValType::V128 => (WasmValue::V128(u128::from_le_bytes(bytes[offset..offset + 16].try_into().unwrap())), 16),
            ValType::RefExtern => {
                let v = i64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
                (WasmValue::RefExtern(v as u32), 8)
            }
            ValType::RefFunc => {
                let v = i64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
                (WasmValue::RefFunc(v as u32), 8)
            }
        };
        values.push(value);
        offset += new_offset;
    }
    values
}

#[derive(Debug)]
pub(crate) struct ValueStack {
    pub(crate) stack: Vec<u8>,
}

impl ValueStack {
    pub(crate) fn new() -> Self {
        Self { stack: Vec::with_capacity(VALUE_STACK_SIZE) }
    }

    pub(crate) fn height(&self) -> usize {
        self.stack.len()
    }

    pub(crate) fn replace_top<const N: usize>(&mut self, func: fn(RawWasmValue<N>) -> RawWasmValue<N>) -> Result<()> {
        let value = func(self.pop::<N>()?);
        self.push(value);
        Ok(())
    }

    pub(crate) fn replace_top_trap<const N: usize>(
        &mut self,
        func: fn(RawWasmValue<N>) -> Result<RawWasmValue<N>>,
    ) -> Result<()> {
        let value = func(self.pop::<N>()?)?;
        self.push(value);
        Ok(())
    }

    pub(crate) fn calculate<const N: usize>(
        &mut self,
        func: fn(RawWasmValue<N>, RawWasmValue<N>) -> RawWasmValue<N>,
    ) -> Result<()> {
        let v2 = self.pop::<N>()?;
        let v1 = self.pop::<N>()?;
        self.push(func(v1, v2));
        Ok(())
    }

    pub(crate) fn calculate_trap<const N: usize>(
        &mut self,
        func: fn(RawWasmValue<N>, RawWasmValue<N>) -> Result<RawWasmValue<N>>,
    ) -> Result<()> {
        let v2 = self.pop::<N>()?;
        let v1 = self.pop::<N>()?;
        self.push(func(v1, v2)?);
        Ok(())
    }

    pub(crate) fn drop(&mut self, len: usize) -> Result<()> {
        if len > self.stack.len() {
            return Err(Error::ValueStackUnderflow).into();
        }
        self.stack.truncate(self.stack.len() - len);
        Ok(())
    }

    pub(crate) fn truncate(&mut self, len: usize) {
        self.stack.truncate(len);
    }

    /// Truncate the stack to `len`, but still keep `keep` elements at the end.
    pub(crate) fn truncate_keep<const N: usize>(&mut self, len: usize, keep: usize) {
        let total_to_keep = len + keep;
        let len = self.height();
        assert!(len >= total_to_keep, "RawWasmValueotal to keep should be less than or equal to self.top");

        if len <= total_to_keep {
            return; // No need to truncate if the current size is already less than or equal to total_to_keep
        }

        let items_to_remove = len - total_to_keep;
        let remove_start_index = (len - items_to_remove - keep) as usize;
        let remove_end_index = (len - keep) as usize;
        self.stack.drain(remove_start_index..remove_end_index);
    }

    pub(crate) fn last<const N: usize>(&self) -> Result<RawWasmValue<N>> {
        let len = self.stack.len();
        if len < N {
            return Err(Error::ValueStackUnderflow);
        }
        Ok(RawWasmValue::from(&self.stack[len - N..]))
    }

    pub(crate) fn pop<const N: usize>(&mut self) -> Result<RawWasmValue<N>> {
        let len = self.stack.len();
        if len < N {
            return Err(Error::ValueStackUnderflow);
        }

        let mut bytes = [0; N];
        bytes.copy_from_slice(&self.stack[len - N..]);
        self.stack.truncate(len - N);
        Ok(RawWasmValue(bytes))
    }

    pub(crate) fn push<const N: usize>(&mut self, value: RawWasmValue<N>) {
        self.stack.extend_from_slice(&value.0);
    }

    pub(crate) fn pop_n_typed(&mut self, tys: &[ValType]) -> Vec<WasmValue> {
        let len: usize = tys.iter().map(|ty| ty.size()).sum();
        let values = bytes_to_wasmvalues(&self.stack[self.stack.len() - len..], tys);
        self.stack.truncate(self.stack.len() - len);
        values
    }

    pub(crate) fn pop_locals(&mut self, locals: &[ValType]) -> Vec<[u8; 16]> {
        let len: usize = locals.iter().map(|ty| ty.size()).sum();
        let values = bytes_to_localvalues(&self.stack[self.stack.len() - len..], locals);
        self.stack.truncate(self.stack.len() - len);
        values
    }

    pub(crate) fn push_typed(&mut self, values: &[WasmValue]) {
        values.iter().for_each(|v| wasmvalue_to_bytes(*v, |bytes| self.stack.extend_from_slice(bytes)));
    }
}

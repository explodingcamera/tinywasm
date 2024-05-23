use core::cell::Cell;

use alloc::{format, string::ToString};
use tinywasm_types::*;

use crate::{
    runtime::{RawWasmValue, WasmValueRepr},
    unlikely, Error, Result,
};

/// A WebAssembly Global Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#global-instances>
#[derive(Debug)]
pub(crate) struct GlobalInstance {
    pub(crate) value: Cell<RawWasmValue>,
    pub(crate) ty: GlobalType,
    pub(crate) _owner: ModuleInstanceAddr, // index into store.module_instances
}

impl GlobalInstance {
    pub(crate) fn new(ty: GlobalType, value: RawWasmValue, owner: ModuleInstanceAddr) -> Self {
        Self { ty, value: value.into(), _owner: owner }
    }

    #[inline]
    pub(crate) fn get(&self) -> WasmValue {
        self.value.get().attach_type(self.ty.ty)
    }

    pub(crate) fn set(&mut self, val: WasmValue) -> Result<()> {
        if unlikely(val.val_type() != self.ty.ty) {
            return Err(Error::Other(format!(
                "global type mismatch: expected {:?}, got {:?}",
                self.ty.ty,
                val.val_type()
            )));
        }

        if unlikely(!self.ty.mutable) {
            return Err(Error::Other("global is immutable".to_string()));
        }

        self.value.set(val.into());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_instance_get_set() {
        let global_type = GlobalType { ty: ValType::I32, mutable: true };
        let initial_value = RawWasmValue::from(10i32);
        let owner = 0;

        let mut global_instance = GlobalInstance::new(global_type, initial_value, owner);

        // Test `get`
        assert_eq!(global_instance.get(), WasmValue::I32(10), "global value should be 10");

        // Test `set` with correct type
        assert!(global_instance.set(WasmValue::I32(20)).is_ok(), "set should succeed");
        assert_eq!(global_instance.get(), WasmValue::I32(20), "global value should be 20");

        // Test `set` with incorrect type
        assert!(matches!(global_instance.set(WasmValue::F32(1.0)), Err(Error::Other(_))), "set should fail");

        // Test `set` on immutable global
        let immutable_global_type = GlobalType { ty: ValType::I32, mutable: false };
        let mut immutable_global_instance = GlobalInstance::new(immutable_global_type, initial_value, owner);
        assert!(matches!(immutable_global_instance.set(WasmValue::I32(30)), Err(Error::Other(_))));
    }
}

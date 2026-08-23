mod managed;
mod store;
mod value;

pub use managed::{AnyRef, ArrayRef, EqRef, ExnRef, ExternRef, FuncRef, I31Ref, RefValue, StructRef};
pub(crate) use managed::{ReferentKind, RootedItem, StoreId, StoredRef};
#[cfg(feature = "std")]
pub use store::MemoryCursor;
pub(crate) use store::StoreItem;
pub use store::{
    GcFieldType, GcHeapType, GcRefType, GcStorageType, GcType, GcTypeKind, GcValueType, Global, Memory, Table, Tag,
};
pub use value::WasmValue;

use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::interpreter::ValueRef;
use crate::{Result, Store, Trap};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StoreId(u32);

impl StoreId {
    pub(crate) fn fresh() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        next_store_id(&NEXT)
    }

    pub(crate) const fn get(self) -> u32 {
        self.0
    }
}

fn next_store_id(counter: &AtomicU32) -> StoreId {
    let id = counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("Store identity space exhausted");
    StoreId(id)
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for StoreId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("StoreId(..)")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) enum ReferentKind {
    I31,
    HostExtern,
    Struct,
    Array,
    Exception,
}

#[derive(Clone)]
pub(crate) struct RootedItem {
    pub(crate) store: StoreId,
    pub(crate) value: ValueRef,
    pub(crate) kind: ReferentKind,
    pub(crate) _token: Option<Arc<()>>,
}

impl PartialEq for RootedItem {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.value == other.value
            && (self.kind == ReferentKind::I31 || self.store == other.store)
    }
}

impl Eq for RootedItem {}

#[cfg(feature = "debug")]
impl core::fmt::Debug for RootedItem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Reference").field("kind", &self.kind).finish_non_exhaustive()
    }
}

pub(crate) trait StoredRef: Clone + Sized {
    fn from_rooted_item(item: RootedItem) -> Self;
    fn rooted_item(&self) -> &RootedItem;
}

macro_rules! reference_types {
    ($($(#[$meta:meta])* $name:ident),* $(,)?) => {$($(
        #[$meta]
    )*
        #[derive(Clone, PartialEq, Eq)]
        #[cfg_attr(feature = "debug", derive(Debug))]
        pub struct $name(RootedItem);

        impl StoredRef for $name {
            fn from_rooted_item(item: RootedItem) -> Self { Self(item) }
            fn rooted_item(&self) -> &RootedItem { &self.0 }
        }
    )*};
}

reference_types! {
    /// An owned WebAssembly `anyref`.
    AnyRef,
    /// An owned WebAssembly `eqref`.
    EqRef,
    /// An owned WebAssembly `i31ref`.
    I31Ref,
    /// An owned WebAssembly `structref`.
    StructRef,
    /// An owned WebAssembly `arrayref`.
    ArrayRef,
    /// An owned WebAssembly `externref`.
    ExternRef,
    /// An owned WebAssembly `exnref`.
    ExnRef,
}

impl AnyRef {
    /// Returns this reference as an `eqref` when its referent is comparable.
    pub fn as_eq(&self) -> Option<EqRef> {
        matches!(self.0.kind, ReferentKind::I31 | ReferentKind::Struct | ReferentKind::Array)
            .then(|| EqRef(self.0.clone()))
    }

    /// Returns this reference as an `i31ref` when it contains an i31.
    pub fn as_i31(&self) -> Option<I31Ref> {
        (self.0.kind == ReferentKind::I31).then(|| I31Ref(self.0.clone()))
    }

    /// Returns this reference as a `structref` when it refers to a struct.
    pub fn as_struct(&self) -> Option<StructRef> {
        (self.0.kind == ReferentKind::Struct).then(|| StructRef(self.0.clone()))
    }

    /// Returns this reference as an `arrayref` when it refers to an array.
    pub fn as_array(&self) -> Option<ArrayRef> {
        (self.0.kind == ReferentKind::Array).then(|| ArrayRef(self.0.clone()))
    }

    /// Returns this reference with the `externref` view.
    pub fn to_extern(&self) -> ExternRef {
        ExternRef(self.0.clone())
    }
}

macro_rules! impl_to_any {
    ($($ty:ty),* $(,)?) => {$(
        impl $ty {
            /// Returns this reference with the `anyref` view.
            pub fn to_any(&self) -> AnyRef { AnyRef(self.0.clone()) }
        }
    )*};
}

impl_to_any!(EqRef, I31Ref, StructRef, ArrayRef, ExternRef);

/// A Store-aware WebAssembly function reference.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FuncRef {
    store: StoreId,
    addr: u32,
}

impl FuncRef {
    pub(crate) const fn new(store: StoreId, addr: u32) -> Self {
        Self { store, addr }
    }

    pub(crate) fn addr(self, store: StoreId) -> Option<u32> {
        (self.store == store).then_some(self.addr)
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for FuncRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("FuncRef(..)")
    }
}

/// A host-facing WebAssembly reference value.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum RefValue {
    /// A null reference.
    Null,
    /// A function reference.
    Func(FuncRef),
    /// An external reference.
    Extern(ExternRef),
    /// A reference in the `any` hierarchy.
    Any(AnyRef),
    /// An exception reference.
    Exn(ExnRef),
}

impl ExternRef {
    /// Creates an external reference containing a host-defined key.
    ///
    /// Keys must be in `0..=(1 << 30) - 2`.
    pub fn try_new(store: &mut Store, key: u32) -> Result<Self> {
        let value = ValueRef::try_from_host_any(key).ok_or(Trap::InvalidReference)?;
        store.root_reference(value, ReferentKind::HostExtern)
    }

    /// Returns the host-defined key stored in this external reference.
    ///
    /// Returns an error for a reference produced by `extern.convert_any` from a
    /// guest GC object or when `store` does not own the reference.
    pub fn key(&self, store: &Store) -> Result<u32> {
        let value = store.resolve_ref(self)?;
        let addr = value.addr().filter(|_| value.is_host_any()).ok_or(Trap::InvalidReference)?;
        Ok(addr & !(1 << 30))
    }
}

impl I31Ref {
    /// Creates a signed WebAssembly i31 reference.
    ///
    /// Values must be in `-2^30..=2^30 - 1`.
    pub fn try_new(store: &mut Store, value: i32) -> Result<Self> {
        if !(-(1 << 30)..(1 << 30)).contains(&value) {
            return Err(Trap::InvalidReference.into());
        }
        store.root_reference(ValueRef::from_i31(value), ReferentKind::I31)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn exhausted_store_identity_counter_never_wraps() {
        let counter = AtomicU32::new(u32::MAX - 1);
        assert_eq!(next_store_id(&counter).get(), u32::MAX - 1);
        assert!(std::panic::catch_unwind(|| next_store_id(&counter)).is_err());
        assert!(std::panic::catch_unwind(|| next_store_id(&counter)).is_err());
    }

    #[test]
    fn host_reference_key_fits_internal_encoding() {
        assert!(ValueRef::try_from_host_any((1 << 30) - 2).is_some());
        assert!(ValueRef::try_from_host_any((1 << 30) - 1).is_none());
    }
}

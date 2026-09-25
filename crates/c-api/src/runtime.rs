use crate::{
    boxed,
    objects::{Object, ObjectKind},
};
use std::{
    cell::{Cell, RefCell, UnsafeCell},
    ptr,
    rc::Rc,
};
use tinywasm::{Engine, FuncContext, Function, Store, WasmValue};

pub struct wasm_config_t(pub(crate) tinywasm::engine::Config);
pub struct wasm_engine_t(Engine);
pub struct wasm_store_t(pub(crate) Rc<StoreState>);

/// Thread-confined store ownership and the dynamic callback borrow stack.
pub(crate) struct StoreState {
    store: UnsafeCell<Store>,
    busy: Cell<bool>,
    active: Cell<*mut FuncContext<'static>>,
    pub(crate) objects: RefCell<Vec<Rc<Object>>>,
    pub(crate) trap: RefCell<Option<Rc<Object>>>,
}

/// An exclusive reborrow from either the idle store or the current host callback.
pub(crate) enum Access<'a> {
    Store(&'a mut Store),
    Callback(&'a mut FuncContext<'static>),
}

impl Access<'_> {
    pub(crate) fn store(&mut self) -> &mut Store {
        match self {
            Self::Store(store) => store,
            Self::Callback(ctx) => ctx.store_mut(),
        }
    }

    pub(crate) fn call(
        &mut self,
        function: &Function,
        args: &[WasmValue],
        results: &mut [WasmValue],
    ) -> tinywasm::Result<()> {
        match self {
            Self::Store(store) => function.call(store, args, results),
            Self::Callback(ctx) => ctx.call_untyped(function, args, results),
        }
    }

    pub(crate) fn is_callback(&self) -> bool {
        matches!(self, Self::Callback(_))
    }
}

impl StoreState {
    fn new(engine: Engine) -> Self {
        Self {
            store: UnsafeCell::new(Store::new(engine)),
            busy: Cell::new(false),
            active: Cell::new(ptr::null_mut()),
            objects: RefCell::new(Vec::new()),
            trap: RefCell::new(None),
        }
    }

    /// Acquires one exclusive borrow without aliasing an executing interpreter.
    pub(crate) fn access<T>(&self, action: impl FnOnce(&mut Access<'_>) -> tinywasm::Result<T>) -> tinywasm::Result<T> {
        if self.busy.replace(true) {
            return Err(tinywasm::Error::Other("store is already borrowed".into()));
        }
        struct Reset<'a>(&'a Cell<bool>);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _reset = Reset(&self.busy);
        // SAFETY: the store is thread-confined. `busy` excludes simultaneous access.
        // During C callbacks, only the explicitly suspended FuncContext is reborrowed,
        // never the original Store pointer. The guard ends the reborrow before reuse.
        unsafe {
            if self.active.get().is_null() {
                action(&mut Access::Store(&mut *self.store.get()))
            } else {
                action(&mut Access::Callback(&mut *self.active.get()))
            }
        }
    }

    /// Suspends the Rust callback context while C runs and may synchronously reenter.
    pub(crate) fn callback<T>(&self, context: &mut FuncContext<'_>, action: impl FnOnce() -> T) -> T {
        struct Restore<'a> {
            state: &'a StoreState,
            active: *mut FuncContext<'static>,
            busy: bool,
        }
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.state.active.set(self.active);
                self.state.busy.set(self.busy);
            }
        }
        // Erasure is confined to this dynamic extent. `action` cannot retain a Rust
        // reference to context, and context is not touched until the old pointer is restored.
        let active = self.active.replace((context as *mut FuncContext<'_>).cast());
        let busy = self.busy.replace(false);
        let _restore = Restore { state: self, active, busy };
        action()
    }

    /// Interns store objects so independent export and table lookups share identity.
    pub(crate) fn intern(self: &Rc<Self>, kind: ObjectKind) -> Rc<Object> {
        let mut objects = self.objects.borrow_mut();
        if let Some(existing) = objects.iter().find(|object| object.kind.same(&kind)) {
            return existing.clone();
        }
        let object = Rc::new(Object::new(Rc::downgrade(self), kind));
        objects.push(object.clone());
        object
    }
}

export! { pub unsafe extern "C" fn wasm_config_new() -> *mut wasm_config_t { boxed(wasm_config_t(Default::default())) }}
export! { pub unsafe extern "C" fn wasm_config_delete(value: *mut wasm_config_t) { unsafe { crate::delete(value) }; }}
export! { pub unsafe extern "C" fn wasm_engine_new() -> *mut wasm_engine_t { boxed(wasm_engine_t(Engine::default())) }}
export! { pub unsafe extern "C" fn wasm_engine_new_with_config(config: *mut wasm_config_t) -> *mut wasm_engine_t {
    let config = unsafe { Box::from_raw(config) };
    boxed(wasm_engine_t(Engine::new(config.0)))
}}
export! { pub unsafe extern "C" fn wasm_engine_delete(value: *mut wasm_engine_t) { unsafe { crate::delete(value) }; }}
export! { pub unsafe extern "C" fn wasm_store_new(engine: *mut wasm_engine_t) -> *mut wasm_store_t {
    boxed(wasm_store_t(Rc::new(StoreState::new(unsafe { (*engine).0.clone() }))))
}}
export! { pub unsafe extern "C" fn wasm_store_delete(value: *mut wasm_store_t) { unsafe { crate::delete(value) }; }}

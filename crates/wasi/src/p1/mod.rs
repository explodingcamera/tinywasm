//! Synchronous WASI Preview 1 command support.

mod abi;
mod ctx;
mod imports;
mod memory;

pub use ctx::{WasiCtx, WasiCtxError};

use tinywasm::Imports;

/// Creates a new [`Imports`] object with the WASI Preview 1 namespace registered.
pub fn imports() -> Imports {
    let mut imports = Imports::new();
    register(&mut imports);
    imports
}

/// Registers the frozen WASI Preview 1 namespace on an [`Imports`] object.
pub fn register(imports: &mut Imports) {
    self::imports::register(imports);
}

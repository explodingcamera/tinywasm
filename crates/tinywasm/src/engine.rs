use core::fmt::Debug;

use alloc::sync::Arc;

/// Global configuration for the WebAssembly interpreter
///
/// Can be cheaply cloned and shared across multiple executions and threads.
#[derive(Clone)]
pub struct Engine {
    pub(crate) inner: Arc<EngineInner>,
}

impl Debug for Engine {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Engine").finish()
    }
}

impl Engine {
    /// Create a new engine with the given configuration
    pub fn new(config: Config) -> Self {
        Self { inner: Arc::new(EngineInner { config }) }
    }

    /// Get a reference to the engine's configuration
    pub fn config(&self) -> &Config {
        &self.inner.config
    }
}

impl Default for Engine {
    fn default() -> Engine {
        Engine::new(Config::default())
    }
}

pub(crate) struct EngineInner {
    pub(crate) config: Config,
    // pub(crate) allocator: Box<dyn Allocator + Send + Sync>,
}

/// Fuel accounting policy for budgeted execution.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
#[derive(Default)]
pub enum FuelPolicy {
    /// Charge one fuel unit per retired instruction.
    #[default]
    PerInstruction,
    /// Charge one fuel unit per instruction plus predefined extra cost for specific operations.
    Weighted,
}

/// Default initial size for the 32-bit value stack (i32, f32 values).
pub const DEFAULT_VALUE_STACK_32_SIZE: usize = 64 * 1024; // 64k slots

/// Default initial size for the 64-bit value stack (i64, f64 values).
pub const DEFAULT_VALUE_STACK_64_SIZE: usize = 32 * 1024; // 32k slots

/// Default initial size for the 128-bit value stack (v128 values).
pub const DEFAULT_VALUE_STACK_128_SIZE: usize = 4 * 1024; // 4k slots

/// Default initial size for the reference value stack (funcref, externref values).
pub const DEFAULT_VALUE_STACK_REF_SIZE: usize = 4 * 1024; // 4k slots

/// Default initial size for the call stack (function frames).
pub const DEFAULT_CALL_STACK_SIZE: usize = 2048; // 1024 frames

/// Configuration for the WebAssembly interpreter
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    /// Initial size of the 32-bit value stack (i32, f32 values).
    pub stack_32_size: usize,
    /// Initial size of the 64-bit value stack (i64, f64 values).
    pub stack_64_size: usize,
    /// Initial size of the 128-bit value stack (v128 values).
    pub stack_128_size: usize,
    /// Initial size of the reference value stack (funcref, externref values).
    pub stack_ref_size: usize,
    /// Initial size of the call stack.
    pub call_stack_size: usize,
    /// Fuel accounting policy used by budgeted execution.
    pub fuel_policy: FuelPolicy,
}

impl Config {
    /// Create a new stack configuration with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the fuel accounting policy for budgeted execution.
    pub fn fuel_policy(mut self, fuel_policy: FuelPolicy) -> Self {
        self.fuel_policy = fuel_policy;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stack_32_size: DEFAULT_VALUE_STACK_32_SIZE,
            stack_64_size: DEFAULT_VALUE_STACK_64_SIZE,
            stack_128_size: DEFAULT_VALUE_STACK_128_SIZE,
            stack_ref_size: DEFAULT_VALUE_STACK_REF_SIZE,
            call_stack_size: DEFAULT_CALL_STACK_SIZE,
            fuel_policy: FuelPolicy::default(),
        }
    }
}

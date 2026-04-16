use alloc::sync::Arc;

/// Global configuration for the WebAssembly interpreter
///
/// Can be cheaply cloned and shared across multiple executions and threads.
#[derive(Clone, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct Engine {
    pub(crate) inner: Arc<EngineInner>,
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

#[derive(Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct EngineInner {
    pub(crate) config: Config,
}

/// Fuel accounting policy for budgeted execution.
#[non_exhaustive]
#[derive(Default, Clone, Copy)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum FuelPolicy {
    /// Charge one fuel unit per retired instruction.
    #[default]
    PerInstruction,
    /// Charge one fuel unit per instruction plus predefined extra cost for specific operations.
    Weighted,
}

/// Default size for the 32-bit value stack (i32, f32, ref values).
pub const DEFAULT_VALUE_STACK_32_SIZE: usize = 36 * 1024; // 36k slots

/// Default size for the 64-bit value stack (i64, f64 values).
pub const DEFAULT_VALUE_STACK_64_SIZE: usize = 32 * 1024; // 32k slots

/// Default size for the 128-bit value stack (v128 values).
pub const DEFAULT_VALUE_STACK_128_SIZE: usize = 4 * 1024; // 4k slots

/// Default maximum size for the call stack (function frames).
pub const DEFAULT_MAX_CALL_STACK_SIZE: usize = 1024; // 1024 frames

/// Configuration for the WebAssembly interpreter
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[non_exhaustive]
pub struct Config {
    /// Size of the 32-bit value stack (i32, f32, ref values).
    pub stack_32_size: usize,
    /// Size of the 64-bit value stack (i64, f64 values).
    pub stack_64_size: usize,
    /// Size of the 128-bit value stack (v128 values).
    pub stack_128_size: usize,
    /// Maximum size of the call stack
    pub max_call_stack_size: usize,
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
            max_call_stack_size: DEFAULT_MAX_CALL_STACK_SIZE,
            fuel_policy: FuelPolicy::default(),
        }
    }
}

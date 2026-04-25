/// Memory backend types and traits.
pub use crate::store::{LazyLinearMemory, LinearMemory, MemoryBackend, PagedMemory, VecMemory};

/// Global configuration for the WebAssembly interpreter
///
/// Can be cheaply cloned and shared across multiple executions and threads.
#[derive(Clone, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct Engine {
    pub(crate) config: Config,
}

impl Engine {
    /// Create a new engine with the given configuration
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Get a reference to the engine's configuration
    pub fn config(&self) -> &Config {
        &self.config
    }
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

/// Stack allocation policy.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct StackConfig {
    /// Initial reserved capacity for the stack.
    pub initial_size: usize,
    /// Maximum number of elements the stack may contain.
    pub max_size: usize,
    /// Whether the stack may grow past its initial capacity.
    pub dynamic: bool,
}

impl StackConfig {
    /// Creates a fixed-capacity stack that reserves all space up front.
    pub const fn fixed(size: usize) -> Self {
        Self { initial_size: size, max_size: size, dynamic: false }
    }

    /// Creates a dynamically growing stack with the given initial and maximum sizes.
    pub const fn dynamic(initial_size: usize, max_size: usize) -> Self {
        assert!(initial_size <= max_size, "initial_size must be less than or equal to max_size");
        Self { initial_size, max_size, dynamic: true }
    }
}

/// Configuration for the WebAssembly interpreter
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[non_exhaustive]
pub struct Config {
    /// Configuration for the 32-bit value stack (i32, f32, ref values).
    pub value_stack_32: StackConfig,
    /// Configuration for the 64-bit value stack (i64, f64 values).
    pub value_stack_64: StackConfig,
    /// Configuration for the 128-bit value stack (v128 values).
    pub value_stack_128: StackConfig,
    /// Configuration for the call stack.
    pub call_stack: StackConfig,
    /// Fuel accounting policy used by budgeted execution.
    pub fuel_policy: FuelPolicy,
    /// Backend used for runtime memories.
    pub memory_backend: MemoryBackend,
    /// Whether memory and stack allocation failures should trap instead of degrading into normal operation failure modes.
    pub trap_on_oom: bool,
}

impl Config {
    /// Create a new stack configuration with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the fuel accounting policy for budgeted execution.
    pub fn with_fuel_policy(mut self, fuel_policy: FuelPolicy) -> Self {
        self.fuel_policy = fuel_policy;
        self
    }

    /// Set the backend used for runtime memories.
    pub fn with_memory_backend(mut self, memory_backend: MemoryBackend) -> Self {
        self.memory_backend = memory_backend;
        self
    }

    /// Set the configuration used for the 32-bit value stack.
    pub fn with_value_stack_32(mut self, stack: StackConfig) -> Self {
        self.value_stack_32 = stack;
        self
    }

    /// Set the same configuration for all value stack lanes.
    pub fn with_value_stack(mut self, stack: StackConfig) -> Self {
        self.value_stack_32 = stack;
        self.value_stack_64 = stack;
        self.value_stack_128 = stack;
        self
    }

    /// Set the configuration used for the 64-bit value stack.
    pub fn with_value_stack_64(mut self, stack: StackConfig) -> Self {
        self.value_stack_64 = stack;
        self
    }

    /// Set the configuration used for the 128-bit value stack.
    pub fn with_value_stack_128(mut self, stack: StackConfig) -> Self {
        self.value_stack_128 = stack;
        self
    }

    /// Set the configuration used for the call stack.
    pub fn with_call_stack(mut self, stack: StackConfig) -> Self {
        self.call_stack = stack;
        self
    }

    /// Configure whether memory and stack allocation failures trap immediately.
    pub fn with_trap_on_oom(mut self, trap_on_oom: bool) -> Self {
        self.trap_on_oom = trap_on_oom;
        self
    }

    /// Get the current fuel policy
    pub fn fuel_policy(&self) -> FuelPolicy {
        self.fuel_policy
    }

    /// Get the current memory backend
    pub fn memory_backend(&self) -> &MemoryBackend {
        &self.memory_backend
    }

    pub(crate) const fn trap_on_oom(&self) -> bool {
        self.trap_on_oom
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            value_stack_32: StackConfig::fixed(DEFAULT_VALUE_STACK_32_SIZE),
            value_stack_64: StackConfig::fixed(DEFAULT_VALUE_STACK_64_SIZE),
            value_stack_128: StackConfig::fixed(DEFAULT_VALUE_STACK_128_SIZE),
            call_stack: StackConfig::fixed(DEFAULT_MAX_CALL_STACK_SIZE),
            fuel_policy: FuelPolicy::default(),
            memory_backend: MemoryBackend::default(),
            trap_on_oom: false,
        }
    }
}

use alloc::sync::Arc;

use crate::ResourceLimiter;

/// Global configuration for the WebAssembly interpreter
///
/// Can be cheaply cloned and shared across multiple executions and threads.
///
/// ## Example
/// ```rust
/// use tinywasm::engine::{Config, StackConfig};
/// use tinywasm::{Engine, Store};
///
/// let config = Config::new()
///     .with_value_stack(StackConfig::dynamic(1024, 16 * 1024))
///     .with_call_stack(StackConfig::fixed(256));
/// let engine = Engine::new(config);
/// let store = Store::new(engine);
/// # _ = store;
/// ```
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
///
/// ## Example
/// ```rust
/// use tinywasm::engine::{Config, FuelPolicy, StackConfig};
///
/// let config = Config::new()
///     .with_fuel_policy(FuelPolicy::Weighted)
///     .with_value_stack_32(StackConfig::dynamic(1024, 36 * 1024))
///     .with_value_stack_64(StackConfig::dynamic(1024, 32 * 1024))
///     .with_value_stack_128(StackConfig::dynamic(256, 4 * 1024))
///     .with_call_stack(StackConfig::dynamic(64, 1024))
///     .with_trap_on_oom(true);
///
/// assert!(matches!(config.fuel_policy(), FuelPolicy::Weighted));
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct Config {
    /// Configuration for the 32-bit value stack (i32, f32, ref values).
    /// Defaults to `StackConfig::fixed(36 * 1024)`.
    pub value_stack_32: StackConfig,
    /// Configuration for the 64-bit value stack (i64, f64 values).
    /// Defaults to `StackConfig::fixed(32 * 1024)`.
    pub value_stack_64: StackConfig,
    /// Configuration for the 128-bit value stack (v128 values).
    /// Defaults to `StackConfig::fixed(4 * 1024)`.
    pub value_stack_128: StackConfig,
    /// Configuration for the call stack. Defaults to `StackConfig::fixed(1024)`.
    pub call_stack: StackConfig,
    /// Fuel accounting policy used by budgeted execution. Defaults to [`FuelPolicy::PerInstruction`].
    pub fuel_policy: FuelPolicy,
    /// Whether memory and stack allocation failures should trap instead of degrading into normal operation failure modes.
    /// Defaults to `false`.
    pub trap_on_oom: bool,
    /// Resource limiter shared across all stores created from this engine. Defaults to `None`.
    pub resource_limiter: Option<Arc<dyn ResourceLimiter>>,
    /// Initial number of GC heap bytes that triggers collection.
    /// Defaults to 1 MiB.
    pub gc_collection_threshold: usize,
}

impl Config {
    /// Create a new interpreter configuration with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the fuel accounting policy for budgeted execution.
    pub fn with_fuel_policy(mut self, fuel_policy: FuelPolicy) -> Self {
        self.fuel_policy = fuel_policy;
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

    /// Set the resource limiter shared across all stores created from this engine.
    pub fn with_resource_limiter(mut self, limiter: Arc<dyn ResourceLimiter>) -> Self {
        self.resource_limiter = Some(limiter);
        self
    }

    /// Set the initial GC heap collection threshold in bytes.
    pub fn with_gc_collection_threshold(mut self, threshold: usize) -> Self {
        self.gc_collection_threshold = threshold;
        self
    }

    /// Get the current fuel policy
    pub fn fuel_policy(&self) -> FuelPolicy {
        self.fuel_policy
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
            trap_on_oom: false,
            resource_limiter: None,
            gc_collection_threshold: 1024 * 1024,
        }
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for Config {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Config")
            .field("value_stack_32", &self.value_stack_32)
            .field("value_stack_64", &self.value_stack_64)
            .field("value_stack_128", &self.value_stack_128)
            .field("call_stack", &self.call_stack)
            .field("fuel_policy", &self.fuel_policy)
            .field("trap_on_oom", &self.trap_on_oom)
            .field("resource_limiter", &self.resource_limiter.is_some())
            .field("gc_collection_threshold", &self.gc_collection_threshold)
            .finish()
    }
}

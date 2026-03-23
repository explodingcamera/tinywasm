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

// pub(crate) trait Allocator {}
// pub(crate) struct DefaultAllocator;
// impl Allocator for DefaultAllocator {}

/// Default initial size for the 32-bit value stack (i32, f32 values).
pub const DEFAULT_VALUE_STACK_32_INIT_SIZE: usize = 32 * 1024; // 32KB

/// Default initial size for the 64-bit value stack (i64, f64 values).
pub const DEFAULT_VALUE_STACK_64_INIT_SIZE: usize = 16 * 1024; // 16KB

/// Default initial size for the 128-bit value stack (v128 values).
pub const DEFAULT_VALUE_STACK_128_INIT_SIZE: usize = 8 * 1024; // 8KB

/// Default initial size for the reference value stack (funcref, externref values).
pub const DEFAULT_VALUE_STACK_REF_INIT_SIZE: usize = 1024; // 1KB

/// Default initial size for the block stack.
pub const DEFAULT_BLOCK_STACK_INIT_SIZE: usize = 128;

/// Default initial size for the call stack.
pub const DEFAULT_CALL_STACK_INIT_SIZE: usize = 128;

/// Configuration for the WebAssembly interpreter
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    /// Initial size of the 32-bit value stack (i32, f32 values).
    pub stack_32_init_size: usize,
    /// Initial size of the 64-bit value stack (i64, f64 values).
    pub stack_64_init_size: usize,
    /// Initial size of the 128-bit value stack (v128 values).
    pub stack_128_init_size: usize,
    /// Initial size of the reference value stack (funcref, externref values).
    pub stack_ref_init_size: usize,
    /// Optional maximum sizes for the stacks. If set, the interpreter will enforce these limits and return an error if they are exceeded.
    pub stack_32_max_size: Option<usize>,
    /// Optional maximum sizes for the stacks. If set, the interpreter will enforce these limits and return an error if they are exceeded.
    pub stack_64_max_size: Option<usize>,
    /// Optional maximum sizes for the stacks. If set, the interpreter will enforce these limits and return an error if they are exceeded.
    pub stack_128_max_size: Option<usize>,
    /// Optional maximum sizes for the stacks. If set, the interpreter will enforce these limits and return an error if they are exceeded.
    pub stack_ref_max_size: Option<usize>,

    /// Initial size of the call stack.
    pub call_stack_init_size: usize,
    /// The maximum size of the call stack. If set, the interpreter will enforce this limit and return an error if it is exceeded.
    pub call_stack_max_size: Option<usize>,

    /// Initial size of the control stack (block stack).
    pub block_stack_init_size: usize,
    /// Optional maximum size for the control stack (block stack). If set, the interpreter will enforce this limit and return an error if it is exceeded.
    pub block_stack_max_size: Option<usize>,
}

impl Config {
    /// Create a new stack configuration with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the same maximum size for all stacks. If set, the interpreter will enforce this limit and return an error if it is exceeded.
    pub fn with_max_stack_size(mut self, max_size: usize) -> Self {
        self.stack_32_max_size = Some(max_size);
        self.stack_64_max_size = Some(max_size);
        self.stack_128_max_size = Some(max_size);
        self.stack_ref_max_size = Some(max_size);
        self.block_stack_max_size = Some(max_size);
        self
    }

    /// Set the same initial size for all stacks.
    pub fn with_initial_stack_size(mut self, init_size: usize) -> Self {
        self.stack_32_init_size = init_size;
        self.stack_64_init_size = init_size;
        self.stack_128_init_size = init_size;
        self.stack_ref_init_size = init_size;
        self.block_stack_init_size = init_size;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stack_32_init_size: DEFAULT_VALUE_STACK_32_INIT_SIZE,
            stack_64_init_size: DEFAULT_VALUE_STACK_64_INIT_SIZE,
            stack_128_init_size: DEFAULT_VALUE_STACK_128_INIT_SIZE,
            stack_ref_init_size: DEFAULT_VALUE_STACK_REF_INIT_SIZE,
            block_stack_init_size: DEFAULT_BLOCK_STACK_INIT_SIZE,
            call_stack_init_size: DEFAULT_CALL_STACK_INIT_SIZE,
            call_stack_max_size: None,
            stack_32_max_size: None,
            stack_64_max_size: None,
            stack_128_max_size: None,
            stack_ref_max_size: None,
            block_stack_max_size: None,
        }
    }
}

use core::fmt;

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

/// Configuration for the WebAssembly interpreter's stack preallocation.
///
/// This struct allows you to configure how much space is preallocated for the
/// different parts of the stack that the interpreter uses to store values.
#[derive(Debug, Clone)]
pub struct StackConfig {
    value_stack_32_init_size: Option<usize>,
    value_stack_64_init_size: Option<usize>,
    value_stack_128_init_size: Option<usize>,
    value_stack_ref_init_size: Option<usize>,
    block_stack_init_size: Option<usize>,
}

impl StackConfig {
    /// Create a new stack configuration with default settings.
    pub fn new() -> Self {
        Self {
            value_stack_32_init_size: None,
            value_stack_64_init_size: None,
            value_stack_128_init_size: None,
            value_stack_ref_init_size: None,
            block_stack_init_size: None,
        }
    }

    /// Get the initial size for the 32-bit value stack.
    pub fn value_stack_32_init_size(&self) -> usize {
        self.value_stack_32_init_size.unwrap_or(DEFAULT_VALUE_STACK_32_INIT_SIZE)
    }

    /// Get the initial size for the 64-bit value stack.
    pub fn value_stack_64_init_size(&self) -> usize {
        self.value_stack_64_init_size.unwrap_or(DEFAULT_VALUE_STACK_64_INIT_SIZE)
    }

    /// Get the initial size for the 128-bit value stack.
    pub fn value_stack_128_init_size(&self) -> usize {
        self.value_stack_128_init_size.unwrap_or(DEFAULT_VALUE_STACK_128_INIT_SIZE)
    }

    /// Get the initial size for the reference value stack.
    pub fn value_stack_ref_init_size(&self) -> usize {
        self.value_stack_ref_init_size.unwrap_or(DEFAULT_VALUE_STACK_REF_INIT_SIZE)
    }

    /// Get the initial size for the block stack.
    pub fn block_stack_init_size(&self) -> usize {
        self.block_stack_init_size.unwrap_or(DEFAULT_BLOCK_STACK_INIT_SIZE)
    }

    /// Set the initial capacity for the 32-bit value stack.
    pub fn with_value_stack_32_init_size(mut self, capacity: usize) -> Self {
        self.value_stack_32_init_size = Some(capacity);
        self
    }

    /// Set the initial capacity for the 64-bit value stack.
    pub fn with_value_stack_64_init_size(mut self, capacity: usize) -> Self {
        self.value_stack_64_init_size = Some(capacity);
        self
    }

    /// Set the initial capacity for the 128-bit value stack.
    pub fn with_value_stack_128_init_size(mut self, capacity: usize) -> Self {
        self.value_stack_128_init_size = Some(capacity);
        self
    }

    /// Set the initial capacity for the reference value stack.
    pub fn with_value_stack_ref_init_size(mut self, capacity: usize) -> Self {
        self.value_stack_ref_init_size = Some(capacity);
        self
    }

    /// Set the initial capacity for the block stack.
    pub fn with_block_stack_init_size(mut self, capacity: usize) -> Self {
        self.block_stack_init_size = Some(capacity);
        self
    }
}

impl Default for StackConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for StackConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StackConfig {{ ")?;
        write!(f, "value_stack_32: {}, ", self.value_stack_32_init_size())?;
        write!(f, "value_stack_64: {}, ", self.value_stack_64_init_size())?;
        write!(f, "value_stack_128: {}, ", self.value_stack_128_init_size())?;
        write!(f, "value_stack_ref: {}, ", self.value_stack_ref_init_size())?;
        write!(f, "block_stack: {} }}", self.block_stack_init_size())?;
        Ok(())
    }
}

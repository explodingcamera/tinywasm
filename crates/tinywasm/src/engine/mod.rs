mod executer;
mod stack;
mod types;

pub use executer::*;
pub use stack::*;
pub use types::*;

#[derive(Debug)]
pub struct Engine {
    pub stack: Stack,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            stack: Stack::default(),
        }
    }
}

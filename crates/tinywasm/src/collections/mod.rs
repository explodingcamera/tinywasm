//! Internal storage collections used by stacks and the managed heap.

mod arena;
mod stack;

pub(crate) use arena::{AllocError, Arena, Handle, Trace};
pub(crate) use stack::StackLane;

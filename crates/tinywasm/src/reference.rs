use core::cell::RefCell;

use crate::{GlobalInstance, MemoryInstance};
use alloc::rc::Rc;
// This module essentially contains the public APIs to interact with the data stored in the store

/// A reference to a memory instance
#[derive(Debug, Clone)]
pub struct MemoryRef {
    pub(crate) _instance: Rc<RefCell<MemoryInstance>>,
}

/// A reference to a global instance
#[derive(Debug, Clone)]
pub struct GlobalRef {
    pub(crate) _instance: Rc<RefCell<GlobalInstance>>,
}

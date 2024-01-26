use core::cell::RefCell;

use alloc::rc::Rc;

use crate::{GlobalInstance, MemoryInstance};

/// A reference to a memory instance
#[derive(Debug, Clone)]
pub struct MemoryRef {
    pub(crate) instance: Rc<RefCell<MemoryInstance>>,
}

/// A reference to a global instance
#[derive(Debug, Clone)]
pub struct GlobalRef {
    pub(crate) instance: Rc<RefCell<GlobalInstance>>,
}

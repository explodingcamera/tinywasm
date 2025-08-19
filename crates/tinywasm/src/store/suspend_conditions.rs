use crate::store::Store;
use alloc::boxed::Box;
use core::fmt::Debug;
use core::ops::ControlFlow;

/// user callback for use in [SuspendConditions::suspend_cb]
pub type ShouldSuspendCb = Box<dyn FnMut(&Store) -> ControlFlow<(), ()>>;

/// used to limit execution time wasm code takes
#[derive(Default)]
#[non_exhaustive] // some fields are feature-gated, use with*-methods to construct
pub struct SuspendConditions {
    /// atomic flag. when set to true it means execution should suspend
    /// can be used to tell executor to stop from another thread
    pub suspend_flag: Option<alloc::sync::Arc<core::sync::atomic::AtomicBool>>,

    /// instant at which execution should suspend
    /// can be used to control how much time will be spent in wasm without requiring other threads
    /// such as for time-slice multitasking
    /// uses rust standard library for checking time - so not available in no-std
    #[cfg(feature = "std")]
    pub timeout_instant: Option<crate::std::time::Instant>,

    /// callback that returns [`ControlFlow::Break`]` when execution should suspend
    /// can be used when above ways are insufficient or
    /// instead of [`timeout_instant`] in no-std builds, with your own clock function
    pub suspend_cb: Option<ShouldSuspendCb>,
}

impl Debug for SuspendConditions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let stop_cb_text = if self.suspend_cb.is_some() { "<present>" } else { "<not present>" };
        let mut f = f.debug_struct("SuspendConditions");
        f.field("stop_flag", &self.suspend_flag);
        #[cfg(feature = "std")]
        {
            f.field("timeout_instant", &self.timeout_instant);
        }
        f.field("stop_cb", &stop_cb_text).finish()
    }
}

impl SuspendConditions {
    /// creates suspend_conditions with every condition unset
    pub fn new() -> Self {
        Default::default()
    }

    /// sets timeout_instant to `how_long` from now
    #[cfg(feature = "std")]
    pub fn set_timeout_in(&mut self, how_long: crate::std::time::Duration) -> &mut Self {
        self.timeout_instant = Some(crate::std::time::Instant::now() + how_long);
        self
    }
    /// adds timeout at specified instant
    #[cfg(feature = "std")]
    pub fn with_timeout_at(self, when: crate::std::time::Instant) -> Self {
        Self { timeout_instant: Some(when), ..self }
    }
    /// adds timeout in specified duration
    #[cfg(feature = "std")]
    pub fn with_timeout_in(self, how_long: crate::std::time::Duration) -> Self {
        Self { timeout_instant: Some(crate::std::time::Instant::now() + how_long), ..self }
    }
    /// removes timeout
    pub fn without_timeout(self) -> Self {
        #[cfg(feature = "std")]
        {
            Self { timeout_instant: None, ..self }
        }
        #[cfg(not(feature = "std"))]
        {
            self
        }
    }

    /// adds susped flag
    pub fn with_suspend_flag(self, should_suspend: alloc::sync::Arc<core::sync::atomic::AtomicBool>) -> Self {
        Self { suspend_flag: Some(should_suspend), ..self }
    }
    /// removes susped flag
    pub fn without_suspend_flag(self) -> Self {
        Self { suspend_flag: None, ..self }
    }

    /// adds suspend callback
    pub fn with_suspend_callback(self, cb: ShouldSuspendCb) -> Self {
        Self { suspend_cb: Some(cb), ..self }
    }
    /// removes suspend callback
    pub fn without_suspend_callback(self) -> Self {
        Self { suspend_cb: None, ..self }
    }
}

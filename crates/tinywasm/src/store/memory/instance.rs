use tinywasm_types::Shared;
use tinywasm_types::{MemoryArch, MemoryType};

use crate::shared::{AtomicUsize, Ordering};
use crate::{Error, ResourceLimiter, Result, Trap};

use super::{MemoryStorage, memory_oob};

const MEMORY64_MAX_BYTES: u64 = 16 * 1024 * 1024 * 1024;

/// A WebAssembly Memory Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#memory-instances>
pub(crate) struct MemoryInstance {
    pub(crate) kind: MemoryType,
    pub(crate) inner: MemoryStorage,
    pub(crate) page_count: usize,
    // Fields drop in declaration order: release the charge after the backing storage.
    pub(super) charge: Option<MemoryCharge>,
}

/// The part of a memory's logical size approved by one limiter.
pub(super) struct MemoryCharge {
    limiter: Shared<dyn ResourceLimiter>,
    bytes: AtomicUsize,
}

impl MemoryCharge {
    pub(super) fn new(limiter: Shared<dyn ResourceLimiter>, bytes: usize) -> Self {
        Self { limiter, bytes: AtomicUsize::new(bytes) }
    }

    pub(super) fn limiter(&self) -> &dyn ResourceLimiter {
        self.limiter.as_ref()
    }

    pub(super) fn add(&self, bytes: usize) {
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
    }
}

impl Drop for MemoryCharge {
    fn drop(&mut self) {
        let bytes = self.bytes.load(Ordering::Relaxed);
        if bytes != 0 {
            self.limiter.memory_dropped(bytes);
        }
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for MemoryInstance {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MemoryInstance").field("kind", &self.kind).field("page_count", &self.page_count).finish()
    }
}

impl MemoryInstance {
    /// Converts a page count to a byte length that fits the host address space.
    #[inline]
    pub(super) fn host_size(kind: MemoryType, pages: u64) -> Option<usize> {
        pages.checked_mul(kind.page_size()).and_then(|size| usize::try_from(size).ok())
    }

    /// Returns the declared byte limit, saturating when it exceeds the host address space.
    #[inline]
    pub(super) fn maximum_size(kind: MemoryType) -> Option<usize> {
        kind.page_count_max_declared().map(|pages| Self::host_size(kind, pages).unwrap_or(usize::MAX))
    }

    /// Applies the runtime's memory64 allocation cap to the declared page limit.
    #[inline]
    pub(super) fn page_count_max(kind: MemoryType) -> u64 {
        match kind.arch() {
            MemoryArch::I32 => kind.page_count_max(),
            MemoryArch::I64 => kind.page_count_max().min(MEMORY64_MAX_BYTES / kind.page_size()),
        }
    }

    pub(crate) fn new(kind: MemoryType, limiter: Option<Shared<dyn ResourceLimiter>>) -> Result<Self> {
        Self::new_with_storage(kind, limiter, MemoryStorage::try_new)
    }

    fn new_with_storage(
        kind: MemoryType,
        limiter: Option<Shared<dyn ResourceLimiter>>,
        allocate: impl FnOnce(MemoryArch, usize, usize) -> core::result::Result<MemoryStorage, Trap>,
    ) -> Result<Self> {
        if kind.shared() && kind.page_count_max_declared().is_none() {
            return Err(Error::UnsupportedFeature("shared memory requires a maximum"));
        }
        #[cfg(not(feature = "std"))]
        if kind.shared() {
            return Err(Error::UnsupportedFeature("shared memory requires std"));
        }
        if kind.page_size() == 0 {
            return Err(Error::UnsupportedFeature("zero-byte memory pages"));
        }
        let max_pages = Self::page_count_max(kind);
        if kind.page_count_initial() > max_pages {
            return Err(Trap::OutOfMemory.into());
        }
        let initial_len = cold_err!(
            Self::host_size(kind, kind.page_count_initial())
                .ok_or(Error::UnsupportedFeature("memory size exceeds the host address space"))
        )?;

        crate::log::debug!(
            "initializing memory with {} pages of {} bytes",
            kind.page_count_initial(),
            kind.page_size()
        );

        if initial_len != 0
            && let Some(limiter) = limiter.as_deref()
            && !limiter.memory_growing(0, initial_len, Self::maximum_size(kind))?
        {
            return cold!(Err(Trap::OutOfMemory.into()));
        }

        let max_len = Self::host_size(kind, max_pages).unwrap_or(usize::MAX);
        let storage = match allocate(kind.arch(), initial_len, max_len) {
            Ok(storage) => storage,
            Err(error) => {
                if initial_len != 0
                    && let Some(limiter) = limiter.as_deref()
                {
                    limiter.memory_grow_failed(0, initial_len);
                }
                return Err(error.into());
            }
        };
        Ok(Self {
            kind,
            inner: storage,
            page_count: kind.page_count_initial() as usize,
            charge: limiter.map(|limiter| MemoryCharge::new(limiter, initial_len)),
        })
    }

    pub(crate) fn copy_from_memory(
        &mut self,
        dst: usize,
        src_mem: &MemoryInstance,
        src: usize,
        len: usize,
    ) -> Result<(), Trap> {
        self.inner.copy_from(dst, &src_mem.inner, src, len)
    }

    pub(crate) fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Result<(), Trap> {
        cold_err!(self.inner.copy_within(dst, src, len).ok_or_else(|| memory_oob(dst, len, self.inner.len())))
    }

    pub(crate) fn grow(&mut self, pages_delta: i64) -> Result<Option<i64>, Trap> {
        let before = self.inner.len();
        let result = Self::grow_storage(
            self.kind,
            &mut self.inner,
            &mut self.page_count,
            pages_delta,
            self.charge.as_ref().map(MemoryCharge::limiter),
        )?;
        if result.is_some()
            && let Some(charge) = &self.charge
        {
            charge.add(self.inner.len() - before);
        }
        Ok(result)
    }

    /// Grows exclusively borrowed storage after checking limits and the host limiter.
    pub(super) fn grow_storage(
        kind: MemoryType,
        inner: &mut MemoryStorage,
        page_count: &mut usize,
        pages_delta: i64,
        limiter: Option<&dyn ResourceLimiter>,
    ) -> Result<Option<i64>, Trap> {
        Self::grow_storage_with(kind, inner, page_count, pages_delta, limiter, MemoryStorage::grow_to)
    }

    fn grow_storage_with(
        kind: MemoryType,
        inner: &mut MemoryStorage,
        page_count: &mut usize,
        pages_delta: i64,
        limiter: Option<&dyn ResourceLimiter>,
        grow: impl FnOnce(&mut MemoryStorage, usize) -> core::result::Result<(), Trap>,
    ) -> Result<Option<i64>, Trap> {
        let current_pages = *page_count;
        let Some(new_pages) = usize::try_from(pages_delta).ok().and_then(|delta| current_pages.checked_add(delta))
        else {
            return cold!(Ok(None));
        };
        let max_pages = Self::page_count_max(kind).try_into().unwrap_or(usize::MAX);

        if new_pages > max_pages {
            return cold!({
                crate::log::debug!("memory.grow failed: new_pages={}, max_pages={}", new_pages, max_pages);
                Ok(None)
            });
        }

        let Some(new_size) = Self::host_size(kind, new_pages as u64) else {
            return cold!(Ok(None));
        };
        let current_size = inner.len();
        if new_size == current_size {
            return Ok(i64::try_from(current_pages).ok());
        }

        if let Some(limiter) = limiter
            && !limiter.memory_growing(current_size, new_size, Self::maximum_size(kind))?
        {
            return cold!(Ok(None));
        }

        if grow(inner, new_size).is_err() {
            if let Some(limiter) = limiter {
                limiter.memory_grow_failed(current_size, new_size);
            }
            return cold!(Ok(None));
        }
        *page_count = new_pages;
        Ok(i64::try_from(current_pages).ok())
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use tinywasm_types::Shared;

    use super::*;
    use crate::shared::Ordering;

    struct ReservingLimiter {
        used: Shared<AtomicUsize>,
    }

    impl ResourceLimiter for ReservingLimiter {
        fn memory_growing(&self, current: usize, desired: usize, _maximum: Option<usize>) -> Result<bool, Trap> {
            self.used.fetch_add(desired - current, Ordering::SeqCst);
            Ok(true)
        }

        fn memory_grow_failed(&self, current: usize, desired: usize) {
            self.used.fetch_sub(desired - current, Ordering::SeqCst);
        }

        fn memory_dropped(&self, charged_bytes: usize) {
            self.used.fetch_sub(charged_bytes, Ordering::SeqCst);
        }
    }

    fn limiter(used: &Shared<AtomicUsize>) -> Shared<dyn ResourceLimiter> {
        Shared::from(Box::new(ReservingLimiter { used: used.clone() }) as Box<dyn ResourceLimiter>)
    }

    #[test]
    fn failed_initial_allocation_refunds_approved_reservation() {
        let used = Shared::new(AtomicUsize::new(0));
        let ty = MemoryType::new(MemoryArch::I32, 1, None, None);
        let result = MemoryInstance::new_with_storage(ty, Some(limiter(&used)), |_, _, _| Err(Trap::OutOfMemory));
        assert!(matches!(result, Err(Error::Trap(Trap::OutOfMemory))));
        assert_eq!(used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn failed_growth_refunds_only_the_attempted_delta() {
        let used = Shared::new(AtomicUsize::new(0));
        let ty = MemoryType::new(MemoryArch::I32, 1, None, None);
        let mut memory = MemoryInstance::new(ty, Some(limiter(&used))).unwrap();
        assert_eq!(used.load(Ordering::SeqCst), 65_536);
        let result = MemoryInstance::grow_storage_with(
            memory.kind,
            &mut memory.inner,
            &mut memory.page_count,
            1,
            memory.charge.as_ref().map(MemoryCharge::limiter),
            |_, _| Err(Trap::OutOfMemory),
        );
        assert_eq!(result.unwrap(), None);
        assert_eq!(memory.page_count, 1);
        assert_eq!(used.load(Ordering::SeqCst), 65_536);
        drop(memory);
        assert_eq!(used.load(Ordering::SeqCst), 0);
    }
}

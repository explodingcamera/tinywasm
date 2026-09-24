use crate::std::sync::{Condvar, Mutex, MutexGuard};
use crate::std::time::Duration;
use alloc::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    vec::Vec,
};
use core::sync::atomic::{AtomicUsize, Ordering};
use tinywasm_types::MemoryType;

use super::{MemoryInstance, MemoryStorage, memory_oob};
use crate::{ResourceLimiter, Result, Trap};

/// A cloneable handle to one shared WebAssembly memory, importable into multiple stores.
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct MemoryShared(Arc<MemorySharedInstance>);

struct MemorySharedInstance {
    kind: MemoryType,
    pages: AtomicUsize,
    bytes: Mutex<MemoryStorage>,
    // Lock order: bytes, waiters, then an individual waiter's notified flag.
    waiters: Mutex<BTreeMap<usize, VecDeque<Arc<Waiter>>>>,
}

struct Waiter {
    notified: Mutex<bool>,
    wake: Condvar,
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for MemorySharedInstance {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MemorySharedInstance").field("kind", &self.kind).field("pages", &self.pages).finish()
    }
}

/// Holds the memory lock while accessing its bytes.
pub struct MemorySharedGuard<'a> {
    pub(crate) kind: MemoryType,
    pub(crate) inner: MutexGuard<'a, MemoryStorage>,
}

impl MemoryShared {
    pub(crate) fn from_instance(instance: MemoryInstance) -> Self {
        let kind = instance.kind;
        let pages = AtomicUsize::new(instance.page_count);
        let bytes = Mutex::new(instance.inner);
        Self(Arc::new(MemorySharedInstance { kind, pages, bytes, waiters: Mutex::new(BTreeMap::new()) }))
    }

    /// Whether two handles reference the same instance, even across store slots.
    pub(crate) fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Creates shared memory with a declared maximum size.
    ///
    /// The memory type is marked shared automatically.
    pub fn try_new(ty: MemoryType) -> Result<Self> {
        Ok(Self::from_instance(MemoryInstance::new(ty.with_shared(true), None)?))
    }

    /// Locks the memory for scoped access to its bytes and page count.
    pub fn lock(&self) -> MemorySharedGuard<'_> {
        MemorySharedGuard { kind: self.ty(), inner: self.0.bytes.lock().unwrap_or_else(|poison| poison.into_inner()) }
    }

    /// Reads the published size without locking the byte storage.
    pub fn page_count(&self) -> usize {
        self.0.pages.load(Ordering::Acquire)
    }

    /// Reads the immutable memory type without locking.
    pub fn ty(&self) -> MemoryType {
        self.0.kind
    }

    /// Returns the current byte length without locking.
    pub fn len(&self) -> usize {
        MemoryInstance::host_size(self.ty(), self.page_count() as u64).expect("published memory size fits on this host")
    }

    /// Returns whether the memory has no allocated bytes.
    pub fn is_empty(&self) -> bool {
        self.page_count() == 0
    }

    /// Reads up to `dst.len()` bytes, returning the count read.
    pub fn read(&self, offset: usize, dst: &mut [u8]) -> Result<usize> {
        Ok(self.lock().inner.read(offset, dst))
    }

    /// Writes up to `src.len()` bytes, returning the count written.
    pub fn write(&self, offset: usize, src: &[u8]) -> Result<usize> {
        Ok(self.lock().inner.write(offset, src))
    }

    /// Reads an exact range or returns an out-of-bounds trap.
    pub fn read_exact(&self, offset: usize, dst: &mut [u8]) -> Result<()> {
        let guard = self.lock();
        guard.inner.read_exact(offset, dst).ok_or_else(|| memory_oob(offset, dst.len(), guard.inner.len()).into())
    }

    /// Copies an exact range out of the memory.
    pub fn read_vec(&self, offset: usize, len: usize) -> Result<Vec<u8>> {
        let guard = self.lock();
        guard.inner.read_vec(offset, len).ok_or_else(|| memory_oob(offset, len, guard.inner.len()).into())
    }

    /// Copies an entire slice into the memory.
    pub fn copy_from_slice(&self, offset: usize, bytes: &[u8]) -> Result<()> {
        let mut guard = self.lock();
        guard.inner.write_all(offset, bytes).ok_or_else(|| memory_oob(offset, bytes.len(), guard.inner.len()).into())
    }

    /// Copies a range within the same memory.
    pub fn copy_within(&self, src: usize, dst: usize, len: usize) -> Result<()> {
        let mut guard = self.lock();
        guard.inner.copy_within(dst, src, len).ok_or_else(|| memory_oob(dst, len, guard.inner.len()).into())
    }

    /// Fills a range with a byte value.
    pub fn fill(&self, offset: usize, len: usize, val: u8) -> Result<()> {
        let mut guard = self.lock();
        guard.inner.fill(offset, len, val).ok_or_else(|| memory_oob(offset, len, guard.inner.len()).into())
    }

    /// Waits until notified or the signed nanosecond timeout expires.
    pub(crate) fn wait<const N: usize>(
        &self,
        addr: usize,
        expected: u64,
        timeout: i64,
    ) -> core::result::Result<u32, Trap> {
        let guard = self.lock();
        let bytes = guard.inner.read_fixed::<N>(addr)?;
        let mut value = [0u8; 8];
        value[..N].copy_from_slice(&bytes);
        if u64::from_le_bytes(value) != expected {
            return Ok(1);
        }
        if timeout == 0 {
            return Ok(2);
        }
        let waiter = Arc::new(Waiter { notified: Mutex::new(false), wake: Condvar::new() });
        self.0
            .waiters
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .entry(addr)
            .or_default()
            .push_back(waiter.clone());
        drop(guard);

        let notified = waiter.notified.lock().unwrap_or_else(|poison| poison.into_inner());
        if timeout < 0 {
            drop(waiter.wake.wait_while(notified, |ready| !*ready).unwrap_or_else(|poison| poison.into_inner()));
        } else {
            drop(
                waiter
                    .wake
                    .wait_timeout_while(notified, Duration::from_nanos(timeout as u64), |ready| !*ready)
                    .unwrap_or_else(|poison| poison.into_inner())
                    .0,
            );
        }
        // The condvar guard was dropped before taking waiters, preserving lock order.
        let mut waiters = self.0.waiters.lock().unwrap_or_else(|poison| poison.into_inner());
        let notified = *waiter.notified.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(queue) = waiters.get_mut(&addr) {
            queue.retain(|entry| !Arc::ptr_eq(entry, &waiter));
            if queue.is_empty() {
                waiters.remove(&addr);
            }
        }
        Ok(if notified { 0 } else { 2 })
    }

    /// Wakes at most `count` waiters at the given byte address.
    pub(crate) fn notify(&self, addr: usize, count: u32) -> u32 {
        let _guard = self.lock();
        let mut waiters = self.0.waiters.lock().unwrap_or_else(|poison| poison.into_inner());
        let mut woken = 0;
        if let Some(queue) = waiters.get_mut(&addr) {
            while woken < count {
                let Some(waiter) = queue.pop_front() else { break };
                *waiter.notified.lock().unwrap_or_else(|poison| poison.into_inner()) = true;
                waiter.wake.notify_one();
                woken += 1;
            }
            if queue.is_empty() {
                waiters.remove(&addr);
            }
        }
        woken
    }

    /// Grows the memory by the given number of pages, returning its previous size.
    pub fn grow(&self, pages: i64) -> Result<Option<i64>> {
        Ok(self.grow_with_limiter(pages, None)?)
    }

    /// Calls host limiters outside the memory lock, then rechecks concurrent growth.
    pub(crate) fn grow_with_limiter(
        &self,
        pages: i64,
        limiter: Option<&dyn ResourceLimiter>,
    ) -> core::result::Result<Option<i64>, Trap> {
        let Some(limiter) = limiter else {
            let mut guard = self.lock();
            return self.grow_locked(pages, &mut guard.inner);
        };
        loop {
            let guard = self.lock();
            let kind = self.ty();
            let current_pages = self.page_count();
            let Some(new_pages) = usize::try_from(pages).ok().and_then(|delta| current_pages.checked_add(delta)) else {
                return Ok(None);
            };
            if new_pages as u64 > MemoryInstance::page_count_max(kind) {
                return Ok(None);
            }
            let Some(desired) = MemoryInstance::host_size(kind, new_pages as u64) else {
                return Ok(None);
            };
            let current = guard.inner.len();
            let maximum =
                kind.page_count_max_declared().map(|max| MemoryInstance::host_size(kind, max).unwrap_or(usize::MAX));
            drop(guard);

            let allowed = desired == current || limiter.memory_growing(current, desired, maximum)?;

            let mut guard = self.lock();
            if self.page_count() != current_pages {
                continue;
            }
            if !allowed {
                return Ok(None);
            }
            return self.grow_locked(pages, &mut guard.inner);
        }
    }

    fn grow_locked(&self, pages: i64, bytes: &mut MemoryStorage) -> core::result::Result<Option<i64>, Trap> {
        let mut current_pages = self.page_count();
        let result = MemoryInstance::grow_storage(self.ty(), bytes, &mut current_pages, pages, None);
        if result.as_ref().is_ok_and(Option::is_some) {
            self.0.pages.store(current_pages, Ordering::Release);
        }
        result
    }
}

impl MemorySharedGuard<'_> {
    /// Returns the current memory type.
    pub fn ty(&self) -> MemoryType {
        self.kind
    }

    /// Returns the current page count.
    pub fn page_count(&self) -> usize {
        (self.inner.len() as u64 / self.kind.page_size()) as usize
    }

    /// Borrows the bytes while this guard holds the lock.
    pub fn data(&self) -> &[u8] {
        self.inner.data()
    }

    /// Mutably borrows the bytes while this guard holds the lock.
    pub fn data_mut(&mut self) -> &mut [u8] {
        self.inner.data_mut()
    }
}

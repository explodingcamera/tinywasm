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
///
/// # Synchronization
///
/// A single mutex protects the memory's bytes. Both host and Wasm code take this
/// lock to access the bytes or grow the memory. This keeps concurrent accesses safe
/// under Rust's memory model and lets host code borrow slices through [`Self::lock`].
///
/// Wasm atomics use the same mutex. An atomic read-modify-write holds the lock for
/// the whole operation, while separate loads and stores can interleave with other
/// threads. Atomic wait releases the lock while sleeping.
///
/// A lock-free implementation would require a different storage and host-access design,
/// with particular care for overlapping accesses and WebAssembly's memory model.
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
///
/// Until this guard is dropped, other threads cannot access the memory's bytes or grow it.
pub struct MemorySharedGuard<'a> {
    pub(crate) kind: MemoryType,
    pub(crate) inner: MutexGuard<'a, MemoryStorage>,
    pages: &'a AtomicUsize,
}

impl MemoryShared {
    /// Moves a newly allocated memory into a shared backing.
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

    /// Locks the memory for scoped byte access, size queries, and growth.
    ///
    /// Drop the guard before calling other locking methods or running Wasm that accesses
    /// this memory, including through another handle. The lock is not reentrant.
    pub fn lock(&self) -> MemorySharedGuard<'_> {
        // A host panic cannot invalidate the byte storage, so poisoned locks remain usable.
        let inner = self.0.bytes.lock().unwrap_or_else(|poison| poison.into_inner());
        MemorySharedGuard { kind: self.ty(), inner, pages: &self.0.pages }
    }

    /// Reads the published size without locking the byte storage.
    pub(crate) fn page_count(&self) -> usize {
        self.0.pages.load(Ordering::Acquire)
    }

    /// Reads the immutable memory type without locking.
    pub(crate) fn ty(&self) -> MemoryType {
        self.0.kind
    }

    /// Waits until notified or the signed nanosecond timeout expires.
    pub(crate) fn wait<const N: usize>(&self, addr: usize, expected: u64, timeout: i64) -> Result<u32, Trap> {
        let waiter = {
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
            // Keep the value check and registration under the byte lock so a store followed
            // by notify cannot slip between them and leave this waiter asleep.
            let mut waiters = self.0.waiters.lock().unwrap_or_else(|poison| poison.into_inner());
            waiters.entry(addr).or_default().push_back(waiter.clone());
            waiter
        };

        {
            let notified = waiter.notified.lock().unwrap_or_else(|poison| poison.into_inner());
            let _notified = if timeout < 0 {
                let result = waiter.wake.wait_while(notified, |ready| !*ready);
                result.unwrap_or_else(|poison| poison.into_inner())
            } else {
                let timeout = Duration::from_nanos(timeout as u64);
                let result = waiter.wake.wait_timeout_while(notified, timeout, |ready| !*ready);
                result.unwrap_or_else(|poison| poison.into_inner()).0
            };
        }

        // Release the condvar guard before taking waiters, preserving lock order.
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

    /// Calls host limiters outside the memory lock, then rechecks concurrent growth.
    pub(crate) fn grow_with_limiter(
        &self,
        pages: i64,
        limiter: Option<&dyn ResourceLimiter>,
    ) -> Result<Option<i64>, Trap> {
        let Some(limiter) = limiter else {
            return self.lock().grow_inner(pages);
        };
        let kind = self.ty();
        let maximum = MemoryInstance::maximum_size(kind);
        loop {
            let (current_pages, current) = {
                let guard = self.lock();
                (self.page_count(), guard.inner.len())
            };
            let Some(new_pages) = usize::try_from(pages).ok().and_then(|delta| current_pages.checked_add(delta)) else {
                return Ok(None);
            };
            if new_pages as u64 > MemoryInstance::page_count_max(kind) {
                return Ok(None);
            }
            let Some(desired) = MemoryInstance::host_size(kind, new_pages as u64) else {
                return Ok(None);
            };
            let allowed = desired == current || limiter.memory_growing(current, desired, maximum)?;

            let mut guard = self.lock();
            if self.page_count() != current_pages {
                // The limiter approved a different size transition. Ask again with the new size.
                continue;
            }
            if !allowed {
                return Ok(None);
            }
            return guard.grow_inner(pages);
        }
    }
}

impl MemorySharedGuard<'_> {
    /// Returns the memory byte length.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns whether the memory has no allocated bytes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Reads up to `dst.len()` bytes, returning the count read.
    ///
    /// Use [`Self::read_exact`] or [`Self::read_vec`] when you need a full range.
    pub fn read(&self, offset: usize, dst: &mut [u8]) -> Result<usize> {
        Ok(self.inner.read(offset, dst))
    }

    /// Writes up to `src.len()` bytes, returning the count written.
    ///
    /// Use [`Self::copy_from_slice`] when you need the full slice written.
    pub fn write(&mut self, offset: usize, src: &[u8]) -> Result<usize> {
        Ok(self.inner.write(offset, src))
    }

    /// Reads exactly `dst.len()` bytes or returns an out-of-bounds trap.
    pub fn read_exact(&self, offset: usize, dst: &mut [u8]) -> Result<()> {
        self.inner.read_exact(offset, dst).ok_or_else(|| memory_oob(offset, dst.len(), self.len()).into())
    }

    /// Reads `len` bytes into a newly allocated buffer.
    pub fn read_vec(&self, offset: usize, len: usize) -> Result<Vec<u8>> {
        self.inner.read_vec(offset, len).ok_or_else(|| memory_oob(offset, len, self.len()).into())
    }

    /// Copies a full slice into memory.
    pub fn copy_from_slice(&mut self, offset: usize, data: &[u8]) -> Result<()> {
        self.inner.write_all(offset, data).ok_or_else(|| memory_oob(offset, data.len(), self.len()).into())
    }

    /// Copies a range within the same memory.
    pub fn copy_within(&mut self, src: usize, dst: usize, len: usize) -> Result<()> {
        self.inner.copy_within(dst, src, len).ok_or_else(|| memory_oob(dst, len, self.len()).into())
    }

    /// Fills a range with a byte value.
    pub fn fill(&mut self, offset: usize, len: usize, val: u8) -> Result<()> {
        self.inner.fill(offset, len, val).ok_or_else(|| memory_oob(offset, len, self.len()).into())
    }

    /// Grows the memory by the given number of pages, returning its previous page count.
    ///
    /// Returns `None` if the memory cannot grow. New bytes are zero-initialized.
    /// Host growth does not consult a store's resource limiter.
    pub fn grow(&mut self, pages: i64) -> Result<Option<i64>> {
        Ok(self.grow_inner(pages)?)
    }

    fn grow_inner(&mut self, pages: i64) -> Result<Option<i64>, Trap> {
        let mut current_pages = self.page_count();
        let result = MemoryInstance::grow_storage(self.kind, &mut self.inner, &mut current_pages, pages, None)?;
        if result.is_some() {
            self.pages.store(current_pages, Ordering::Release);
        }
        Ok(result)
    }

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

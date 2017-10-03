//! Atomic counting semaphore that can help you control access to a common resource
//! by multiple processes in a concurrent system.
//!
//! ## Features
//!
//! - Provides RAII-style atomic acquire and release
//! - Implements `Send`, `Sync` and `Clone`
//! - Can block until count to drops to zero (useful for implementing shutdown)

extern crate parking_lot;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::{Condvar, Mutex};

#[derive(Clone)]
/// An atomic counter which can be shared across processes.
pub struct Semaphore {
    inner: Arc<Inner>
}

impl Semaphore {
    /// Create a new semaphore with the given limit.
    pub fn new(limit: usize) -> Self {
        Semaphore { inner: Arc::new(Inner::new(limit)) }
    }

    #[inline]
    /// Attempt to access a resource of this semaphore.
    ///
    /// This function will first acquire a resource and then return an RAII
    /// guard structure which will release the resource when it falls out of scope.
    ///
    /// If the semaphore is at limit, `None` will be returned.
    pub fn try_access(&self) -> Option<Guard> {
        if self.inner.try_acquire() {
            Some(Guard { inner: self.inner.clone() })
        } else {
            None
        }
    }

    #[inline]
    /// Block until the resource is not being accessed anymore.
    ///
    /// This can be used to determine whether it is safe to free
    /// or shutdown the resource.
    pub fn wait_until_all_released(&self) {
        self.inner.wait_until_all_released()
    }
}

/// An RAII guard used to release a semaphore automatically when it falls out of scope.
pub struct Guard {
    inner: Arc<Inner>
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.inner.release()
    }
}

struct Inner {
    active: AtomicUsize,
    limit: usize,
    lock: Mutex<()>,
    cond: Condvar
}

impl Inner {
    fn new(limit: usize) -> Inner {
        assert!(limit > 0);
        Inner {
            active: AtomicUsize::default(),
            limit: limit,
            lock: Mutex::new(()),
            cond: Condvar::new()
        }
    }

    #[inline]
    fn try_acquire(&self) -> bool {
        loop {
            let current_active = self.active.load(Ordering::SeqCst);
            assert!(current_active <= self.limit);
            if current_active == self.limit {
                return false;
            }
            let previous_active = self.active.compare_and_swap(
                current_active,
                current_active + 1,
                Ordering::SeqCst
            );
            if previous_active == current_active {
                return true;
            }
        }
    }

    #[inline]
    fn release(&self) {
        let previous_active = self.active.fetch_sub(1, Ordering::SeqCst);
        if previous_active == 1 {
            let guard = self.lock.lock();
            self.cond.notify_all();
            drop(guard)
        }
    }

    #[inline]
    fn wait_until_all_released(&self) {
        let mut lock = self.lock.lock();

        while self.active.load(Ordering::SeqCst) > 0 {
            self.cond.wait(&mut lock);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Semaphore;

    #[test]
    fn succeeds_to_acquire_when_empty() {
        let sema = Semaphore::new(1);
        assert!(sema.try_access().is_some());
    }

    #[test]
    fn fails_to_acquire_when_full() {
        let sema = Semaphore::new(4);
        let guards = (0..4).map(|_| {
            sema.try_access().expect("guard acquisition failed")
        }).collect::<Vec<_>>();
        assert!(sema.try_access().is_none());
        drop(guards);
    }

    #[test]
    fn dropping_guard_frees_capacity() {
        let sema = Semaphore::new(1);
        let guard = sema.try_access().expect("guard acquisition failed");
        drop(guard);
        assert!(sema.try_access().is_some());
    }
}

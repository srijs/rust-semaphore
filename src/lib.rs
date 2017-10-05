//! Atomic counting semaphore that can help you control access to a common resource
//! by multiple processes in a concurrent system.
//!
//! ## Features
//!
//! - Fully lock-free* semantics
//! - Provides RAII-style acquire/release API
//! - Implements `Send`, `Sync` and `Clone`
//!
//! _* when not using the `shutdown` API_

extern crate parking_lot;

use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::RwLock;

mod raw;
use raw::RawSemaphore;

/// Result returned from `Semaphore::try_access`.
pub type TryAccessResult<T> = Result<SemaphoreGuard<T>, TryAccessError>;

#[derive(Copy, Clone, Debug, PartialEq)]
/// Error indicating a failure to acquire access to the resource
/// behind the semaphore.
///
/// Returned from `Semaphore::try_access`.
pub enum TryAccessError {
    /// This semaphore has shut down and will no longer grant access to the underlying resource.
    Shutdown,
    /// This semaphore has no more capacity to grant further access.
    /// Other access needs to be released before this semaphore can grant more.
    NoCapacity
}

/// Atomic counter that can help you control shared access to a resource.
pub struct Semaphore<T> {
    raw: Arc<RawSemaphore>,
    resource: Arc<RwLock<Option<Arc<T>>>>
}

impl<T> Clone for Semaphore<T> {
    fn clone(&self) -> Semaphore<T> {
        Semaphore {
            raw: self.raw.clone(),
            resource: self.resource.clone()
        }
    }
}

impl<T> Semaphore<T> {
    /// Create a new semaphore around a resource with the given limit.
    pub fn new(limit: usize, resource: T) -> Self {
        Semaphore {
            raw: Arc::new(RawSemaphore::new(limit)),
            resource: Arc::new(RwLock::new(Some(Arc::new(resource))))
        }
    }

    #[inline]
    /// Attempt to access the underlying resource of this semaphore.
    ///
    /// This function will try to acquire access, and then return an RAII
    /// guard structure which will release the access when it falls out of scope.
    ///
    /// If the semaphore is out of capacity or shut down, a `TryAccessError` will be returned.
    pub fn try_access(&self) -> TryAccessResult<T> {
        if let Some(ref resource) = *self.resource.read() {
            if self.raw.try_acquire() {
                Ok(SemaphoreGuard {
                    raw: self.raw.clone(),
                    counter: Arc::new(AtomicUsize::new(1)),
                    resource: resource.clone()
                })
            } else {
                Err(TryAccessError::NoCapacity)
            }
        } else {
            Err(TryAccessError::Shutdown)
        }
    }

    /// Shut down the semaphore.
    ///
    /// This prevents any further access from being granted to the underlying resource.
    ///
    /// As soon as the last access is released and the returned handle goes out of scope,
    /// the resource will be dropped.
    ///
    /// Does _not_ block until the resource is no longer in use. If you would like to do that,
    /// you can call `wait` on the returned handle.
    pub fn shutdown(&self) -> ShutdownHandle<T> {
        ShutdownHandle {
            raw: self.raw.clone(),
            resource: self.resource.write().take()
        }
    }
}

/// Handle representing the shutdown process of a semaphore,
/// allowing for extraction of the underlying resource.
///
/// Returned from `Semaphore::shutdown`. 
pub struct ShutdownHandle<T> {
    raw: Arc<RawSemaphore>,
    resource: Option<Arc<T>>
}

impl<T> ShutdownHandle<T> {
    /// Block until all access has been released to the semaphore,
    /// and extract the underlying resource.
    ///
    /// When `Semaphore::shutdown` has been called multiple times,
    /// only the first shutdown handle will return the resource.
    /// All others will return `None`.
    pub fn wait(self) -> Option<T> {
        self.raw.wait_until_inactive();
        self.resource.map(|arc| {
            let mut local_arc = arc;
            loop {
                match Arc::try_unwrap(local_arc) {
                    Ok(resource) => {
                        return resource;
                    },
                    Err(arc) => {
                        local_arc = arc;
                    }
                }
            }
        })
    }

    #[doc(hidden)]
    pub fn is_complete(&self) -> bool {
        !self.raw.is_active()
    }
}

/// RAII guard used to release access to the semaphore automatically when it falls out of scope.
///
/// Returned from `Semaphore::try_access`. 
///
/// Guards can be cloned, in which case the original guard and all descendent guards need
/// to go out of scope for the single access to be released on the semaphore.
pub struct SemaphoreGuard<T> {
    raw: Arc<RawSemaphore>,
    counter: Arc<AtomicUsize>,
    resource: Arc<T>
}

impl<T> Clone for SemaphoreGuard<T> {
    #[inline]
    fn clone(&self) -> SemaphoreGuard<T> {
        self.counter.fetch_add(1, Ordering::SeqCst);
        SemaphoreGuard {
            raw: self.raw.clone(),
            counter: self.counter.clone(),
            resource: self.resource.clone()
        }
    }
}

impl<T> Drop for SemaphoreGuard<T> {
    fn drop(&mut self) {
        let previous_count = self.counter.fetch_sub(1, Ordering::SeqCst);
        if previous_count == 1 {
            self.raw.release();
        }
    }
}

impl<T: Sized> Deref for SemaphoreGuard<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.resource.deref()
    }
}

#[cfg(test)]
mod tests {
    use super::{Semaphore, TryAccessError};

    #[test]
    fn succeeds_to_acquire_when_empty() {
        let sema = Semaphore::new(1, ());
        assert!(sema.try_access().ok().is_some());
    }

    #[test]
    fn fails_to_acquire_when_full() {
        let sema = Semaphore::new(4, ());
        let guards = (0..4).map(|_| {
            sema.try_access().expect("guard acquisition failed")
        }).collect::<Vec<_>>();
        assert_eq!(sema.try_access().err().unwrap(),
            TryAccessError::NoCapacity);
        drop(guards);
    }

    #[test]
    fn dropping_guard_frees_capacity() {
        let sema = Semaphore::new(1, ());
        let guard = sema.try_access().expect("guard acquisition failed");
        drop(guard);
        assert!(sema.try_access().ok().is_some());
    }

    #[test]
    fn fails_to_acquire_when_shut_down() {
        let sema = Semaphore::new(4, ());
        sema.shutdown();
        assert_eq!(sema.try_access().err().unwrap(),
            TryAccessError::Shutdown);
    }

    #[test]
    fn shutdown_complete_when_empty() {
        let sema = Semaphore::new(1, ());
        let handle = sema.shutdown();
        assert_eq!(true, handle.is_complete());
    }

    #[test]
    fn shutdown_complete_when_guard_drops() {
        let sema = Semaphore::new(1, ());
        let guard = sema.try_access().expect("guard acquisition failed");
        let handle = sema.shutdown();
        assert_eq!(false, handle.is_complete());
        drop(guard);
        assert_eq!(true, handle.is_complete());
    }

    #[test]
    fn shutdown_complete_when_parent_and_child_guards_drop()  {
        let sema = Semaphore::new(1, ());
        let parent_guard = sema.try_access().expect("guard acquisition failed");
        let child_guard = parent_guard.clone();
        let handle = sema.shutdown();
        assert_eq!(false, handle.is_complete());
        drop(parent_guard);
        assert_eq!(false, handle.is_complete());
        drop(child_guard);
        assert_eq!(true, handle.is_complete());
    }
}

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

use parking_lot::RwLock;

mod raw;
use raw::RawSemaphore;

#[derive(Debug)]
/// An error indicating a failure to acquire access to the resource
/// behind the semaphore.
///
/// Returned from `Semaphore::try_access`.
pub enum TryAccessError {
    /// The capacity of the semaphore was exceeded.
    CapacityExceeded,
    /// The semaphore has been shut down.
    Shutdown
}

/// An atomic counter which can be shared across processes.
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
    /// Create a new semaphore with the given limit.
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
    /// If the semaphore is at limit or currently shutting down,
    /// a `TryAccessError` will be returned.
    pub fn try_access(&self) -> Result<Guard<T>, TryAccessError> {
        if let Some(ref resource) = *self.resource.read() {
            if self.raw.try_acquire() {
                Ok(Guard { raw: self.raw.clone(), resource: resource.clone() })
            } else {
                Err(TryAccessError::CapacityExceeded)
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
            _resource: self.resource.write().take()
        }
    }
}

/// A handle representing the shutdown process of a semaphore. 
pub struct ShutdownHandle<T> {
    raw: Arc<RawSemaphore>,
    _resource: Option<Arc<T>>
}

impl<T> ShutdownHandle<T> {
    /// Block until the resource is not being accessed anymore.
    ///
    /// Please note that this does not take into account any unguarded
    /// references. This means that after the method returned the resource
    /// could still be kept alive by one or more unguarded references.
    pub fn wait(&self) {
        self.raw.wait_until_all_released()
    }
}

/// An RAII guard used to release a semaphore automatically when it falls out of scope.
pub struct Guard<T> {
    raw: Arc<RawSemaphore>,
    resource: Arc<T>
}

impl<T> Guard<T> {
    #[inline]
    /// Spawns an unguarded reference to the resource.
    pub fn as_unguarded(&self) -> UnguardedRef<T> {
        UnguardedRef { resource: self.resource.clone() }
    }
}

impl<T> Drop for Guard<T> {
    #[inline]
    fn drop(&mut self) {
        self.raw.release()
    }
}

impl<T: Sized> Deref for Guard<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.resource.deref()
    }
}

/// An unguarded reference to a resource.
///
/// Can be created via `Guard::unguard`.
///
/// This reference is not tracked by the semaphore around the resource.
/// It can therefore be used in situations where after acquiring access
/// you want to split access to the resource.
///
/// Caution is advised as the existence of unguarded references will cause
/// the resource to be retained, even when the semaphore has fully shut down.
pub struct UnguardedRef<T> {
    resource: Arc<T>
}

impl<T: Sized> Deref for UnguardedRef<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.resource.deref()
    }
}

#[cfg(test)]
mod tests {
    use super::Semaphore;

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
        assert!(sema.try_access().err().is_some());
        drop(guards);
    }

    #[test]
    fn dropping_guard_frees_capacity() {
        let sema = Semaphore::new(1, ());
        let guard = sema.try_access().expect("guard acquisition failed");
        drop(guard);
        assert!(sema.try_access().ok().is_some());
    }
}

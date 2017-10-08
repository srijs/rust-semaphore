//! Atomic counting semaphore that can help you control access to a common resource
//! by multiple processes in a concurrent system.
//!
//! ## Features
//!
//! - Effectively lock-free* semantics
//! - Provides RAII-style acquire/release API
//! - Implements `Send`, `Sync` and `Clone`
//!
//! _* lock-free when not using the `shutdown` API_

extern crate parking_lot;

use std::sync::Arc;

use parking_lot::RwLock;

mod raw;
use raw::RawSemaphore;

mod guard;
pub use guard::SemaphoreGuard;

mod shutdown;
pub use shutdown::ShutdownHandle;

#[cfg(test)]
mod tests;

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

/// Counting semaphore to control concurrent access to a common resource.
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
    /// Create a new semaphore around a resource.
    ///
    /// The semaphore will limit the number of processes that can access
    /// the underlying resource at every point in time to the specified capacity.
    pub fn new(capacity: usize, resource: T) -> Self {
        Semaphore {
            raw: Arc::new(RawSemaphore::new(capacity)),
            resource: Arc::new(RwLock::new(Some(Arc::new(resource))))
        }
    }

    #[inline]
    /// Attempt to access the underlying resource of this semaphore.
    ///
    /// This function will try to acquire access, and then return an RAII
    /// guard structure which will release the access when it falls out of scope.
    /// If the semaphore is out of capacity or shut down, a `TryAccessError` will be returned.
    pub fn try_access(&self) -> TryAccessResult<T> {
        if let Some(ref resource) = *self.resource.read() {
            if self.raw.try_acquire() {
                Ok(guard::new(&self.raw, resource))
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
    /// As soon as the last access is released and the returned handle goes out of scope,
    /// the resource will be dropped.
    ///
    /// Does _not_ block until the resource is no longer in use. If you would like to do that,
    /// you can call `wait` on the returned handle.
    pub fn shutdown(&self) -> ShutdownHandle<T> {
        shutdown::new(&self.raw, self.resource.write().take())
    }
}

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

use std::ops::Deref;
use std::sync::Arc;

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
                Ok(SemaphoreGuard {
                    raw: self.raw.clone(),
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
        self.resource.map(|mut arc| {
            loop {
                match Arc::try_unwrap(arc) {
                    Ok(resource) => {
                        return resource;
                    },
                    Err(returned_arc) => {
                        arc = returned_arc;
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
/// ## Sharing guards
///
/// There are cases where, once acquired, you want to share a guard between multiple threads
/// of execution. This pattern can be implemented by wrapping the acquired guard into an [`Rc`][1]
/// or [`Arc`][2] reference.
///
/// [1]: https://doc.rust-lang.org/std/rc/struct.Rc.html
/// [2]: https://doc.rust-lang.org/std/sync/struct.Arc.html
pub struct SemaphoreGuard<T> {
    raw: Arc<RawSemaphore>,
    resource: Arc<T>
}

impl<T> Drop for SemaphoreGuard<T> {
    #[inline]
    fn drop(&mut self) {
        self.raw.release()
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
        assert_eq!(Some(()), handle.wait());
    }

    #[test]
    fn shutdown_complete_when_guard_drops() {
        let sema = Semaphore::new(1, ());
        let guard = sema.try_access().expect("guard acquisition failed");
        let handle = sema.shutdown();
        assert_eq!(false, handle.is_complete());
        drop(guard);
        assert_eq!(true, handle.is_complete());
        assert_eq!(Some(()), handle.wait());
    }

    #[test]
    fn first_shutdown_can_extract_resource() {
        let sema = Semaphore::new(1, ());
        let first_handle = sema.shutdown();
        let second_handle = sema.shutdown();
        let third_handle = sema.shutdown();
        assert_eq!(None, second_handle.wait());
        assert_eq!(Some(()), first_handle.wait());
        assert_eq!(None, third_handle.wait());
    }
}

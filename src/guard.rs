use std::ops::Deref;
use std::sync::Arc;

use raw::RawSemaphore;

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

pub fn new<T>(raw: &Arc<RawSemaphore>, resource: &Arc<T>) -> SemaphoreGuard<T> {
    SemaphoreGuard {
        raw: raw.clone(),
        resource: resource.clone()
    }
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

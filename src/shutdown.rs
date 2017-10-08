use std::sync::Arc;

use raw::RawSemaphore;

/// Handle representing the shutdown process of a semaphore,
/// allowing for extraction of the underlying resource.
///
/// Returned from `Semaphore::shutdown`. 
pub struct ShutdownHandle<T> {
    raw: Arc<RawSemaphore>,
    resource: Option<Arc<T>>
}

pub fn new<T>(raw: &Arc<RawSemaphore>, resource: Option<Arc<T>>) -> ShutdownHandle<T> {
    ShutdownHandle {
        raw: raw.clone(),
        resource: resource
    }
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

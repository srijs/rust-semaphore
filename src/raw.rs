use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::{Condvar, Mutex};

pub struct RawSemaphore {
    active: AtomicUsize,
    limit: usize,
    lock: Mutex<()>,
    cond: Condvar
}

impl RawSemaphore {
    pub fn new(limit: usize) -> RawSemaphore {
        RawSemaphore {
            active: AtomicUsize::default(),
            limit: limit,
            lock: Mutex::new(()),
            cond: Condvar::new()
        }
    }

    #[inline]
    pub fn try_acquire(&self) -> bool {
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
    pub fn release(&self) {
        let previous_active = self.active.fetch_sub(1, Ordering::SeqCst);
        if previous_active == 1 {
            let guard = self.lock.lock();
            self.cond.notify_all();
            drop(guard)
        }
    }

    #[inline]
    pub fn wait_until_all_released(&self) {
        let mut lock = self.lock.lock();

        while self.active.load(Ordering::SeqCst) > 0 {
            self.cond.wait(&mut lock);
        }
    }
}

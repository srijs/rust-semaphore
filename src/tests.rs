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

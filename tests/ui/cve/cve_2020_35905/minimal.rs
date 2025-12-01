use std::cell::UnsafeCell;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicUsize;
use std::task::Waker;

/// A futures-aware mutex.
///
/// # Fairness
///
/// This mutex provides no fairness guarantees. Tasks may not acquire the mutex
/// in the order that they requested the lock, and it's possible for a single task
/// which repeatedly takes the lock to starve other tasks, which may be left waiting
/// indefinitely.
pub struct Mutex<T: ?Sized> {
    state: AtomicUsize,
    waiters: StdMutex<Vec<Waiter>>,
    value: UnsafeCell<T>,
}

/// An RAII guard returned by the `MutexGuard::map` and `MappedMutexGuard::map` methods.
/// When this structure is dropped (falls out of scope), the lock will be unlocked.
pub struct MappedMutexGuard<'a, T: ?Sized, U: ?Sized> {
    mutex: &'a Mutex<T>,
    value: *mut U,
}

unsafe impl<T: ?Sized + Send, U: ?Sized> Send for MappedMutexGuard<'_, T, U> {}
unsafe impl<T: ?Sized + Sync, U: ?Sized> Sync for MappedMutexGuard<'_, T, U> {}

enum Waiter {
    Waiting(Waker),
    Woken,
}

fn main() {}

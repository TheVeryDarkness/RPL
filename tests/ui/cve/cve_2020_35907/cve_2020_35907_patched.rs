//@revisions: inline regular inline100
//@[inline] compile-flags: -Z inline-mir=true
//@[inline100] compile-flags: -Z inline-mir=true -Z inline-mir-threshold=100
//@[regular] compile-flags: -Z inline-mir=false
//@check-pass

use std::cell::UnsafeCell;
use std::sync::LazyLock;
use std::task::{RawWaker, RawWakerVTable, Waker};

fn noop_waker() -> Waker {
    unsafe fn clone(_data: *const ()) -> RawWaker {
        RawWaker::new(std::ptr::null(), &NOOP_WAKER_VTABLE)
    }

    unsafe fn wake(_data: *const ()) {}

    unsafe fn wake_by_ref(_data: *const ()) {}

    unsafe fn drop(_data: *const ()) {}

    static NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &NOOP_WAKER_VTABLE)) }
}

pub fn noop_waker_ref() -> &'static Waker {
    static STATIC_INSTANCE: LazyLock<Waker> = LazyLock::new(|| noop_waker());
    &*STATIC_INSTANCE
}

pub fn static_ref() -> &'static i32 {
    static STATIC_INSTANCE: LazyLock<i32> = LazyLock::new(|| 0);
    &*STATIC_INSTANCE
}

fn main() {}

//@compile-flags: -Z inline-mir=false
use std::cell::UnsafeCell;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

#[repr(transparent)]
pub struct AtomicCell<T: ?Sized> {
    /// The inner value.
    ///
    /// If this value can be transmuted into a primitive atomic type, it will be treated as such.
    /// Otherwise, all potentially concurrent operations on this data will be protected by a global
    /// lock.
    value: UnsafeCell<T>,
}

impl<T> AtomicCell<T> {
    /// Creates a new atomic cell initialized with `val`.
    ///
    /// # Examples
    ///
    /// ```
    /// use crossbeam_utils::atomic::AtomicCell;
    ///
    /// let a = AtomicCell::new(7);
    /// ```
    pub const fn new(val: T) -> AtomicCell<T> {
        AtomicCell {
            value: UnsafeCell::new(val),
        }
    }
}

macro_rules! impl_arithmetic {
    ($t:ty, $atomic:ty, $example:tt) => {
        impl AtomicCell<$t> {
            /// Increments the current value by `val` and returns the previous value.
            ///
            /// The addition wraps on overflow.
            ///
            /// # Examples
            ///
            /// ```
            /// use crossbeam_utils::atomic::AtomicCell;
            ///
            #[doc = $example]
            ///
            /// assert_eq!(a.fetch_add(3), 7);
            /// assert_eq!(a.load(), 10);
            /// ```
            #[inline]
            pub fn fetch_add(&self, val: $t) -> $t {
                let a = self.value.get();
                let a = a as *const $atomic;
                //~^ ERROR: it is unsound to cast between `u64` and `AtomicU64`
                let a = unsafe { &*a };
                a.fetch_add(val, Ordering::AcqRel)
            }
        }
    };
}

impl_arithmetic!(u64, AtomicU64, "let a = AtomicCell::new(7u64);");

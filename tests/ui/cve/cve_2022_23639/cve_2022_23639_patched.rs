//@compile-flags: -Z inline-mir=false
use std::cell::UnsafeCell;
use std::mem;
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
                if can_transmute::<$t, $atomic>() {
                    let a = unsafe { &*(self.value.get() as *const $atomic) };
                    //~^unsound_cast_between_u64_and_atomic_u64
                    // False positive, the cast is sound because of the `can_transmute` check
                    a.fetch_add(val, Ordering::AcqRel)
                } else {
                    // #[cfg(crossbeam_loom)]
                    {
                        let _ = val;
                        unimplemented!("loom does not support non-atomic atomic ops");
                    }
                    // #[cfg(not(crossbeam_loom))]
                    // {
                    //     let _guard = lock(self.value.get() as usize).write();
                    //     let value = unsafe { &mut *(self.value.get()) };
                    //     let old = *value;
                    //     *value = value.wrapping_add(val);
                    //     old
                    // }
                }
            }
        }
    };
}

impl_arithmetic!(u64, AtomicU64, "let a = AtomicCell::new(7u64);");

/// Returns `true` if values of type `A` can be transmuted into values of type `B`.
const fn can_transmute<A, B>() -> bool {
    // Sizes must be equal, but alignment of `A` must be greater or equal than that of `B`.
    (mem::size_of::<A>() == mem::size_of::<B>()) & (mem::align_of::<A>() >= mem::align_of::<B>())
}

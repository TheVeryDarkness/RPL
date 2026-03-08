//@ revisions: inline regular
//@[inline] compile-flags: -Z inline-mir=true
//@[regular] compile-flags: -Z inline-mir=false
//@[inline] check-pass
// FIXME: write a non-inline pattern
use std::alloc::{Layout, alloc, alloc_zeroed, dealloc};
use std::ops::{Index, IndexMut, Range};

pub struct Array<T> {
    size: usize,
    ptr: *mut T,
}

unsafe impl<T> Sync for Array<T> {}
unsafe impl<T> Send for Array<T> {}

impl<T> Array<T>
where
    T: Zeroable,
{
    /// Extremely fast initialization if all you want is 0's. Note that your type must be Zeroable.
    /// The auto-Zeroable types are u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, f32, f64.
    /// `std::Array`s also implement Zeroable allowing for types like `[u8; 1 << 25]`.
    pub fn zero(size: usize) -> Self {
        let objsize = std::mem::size_of::<T>();
        let layout = Layout::from_size_align(size * objsize, 8).unwrap();
        let ptr = unsafe { alloc_zeroed(layout) as *mut T };
        //~[regular]^ERROR: public function `zero` allocates a pointer that may be zero-sized, which is an undefined behavior
        Self { size, ptr }
    }
}

impl<T> Array<T> {
    /// Convert to slice
    pub fn to_slice<'a>(&'a self) -> &'a [T] {
        unsafe { std::slice::from_raw_parts(self.ptr as *const T, self.size) }
    }

    /// Convert to mutable slice
    pub fn to_slice_mut<'a>(&'a mut self) -> &'a mut [T] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.size) }
    }

    /// The length of the array (number of elements T)
    pub fn len(&self) -> usize {
        self.size
    }
}

/// Marker trait to determine if a type is auto-zeroable. This allows the initialization to simply
/// zero out the buffer on initialization.
pub trait Zeroable {}

impl Zeroable for u8 {}
impl Zeroable for i8 {}
impl Zeroable for u16 {}
impl Zeroable for i16 {}
impl Zeroable for u32 {}
impl Zeroable for i32 {}
impl Zeroable for u64 {}
impl Zeroable for i64 {}
impl Zeroable for usize {}
impl Zeroable for isize {}
impl Zeroable for f32 {}
impl Zeroable for f64 {}

impl<T, const N: usize> Zeroable for [T; N] where T: Zeroable {}

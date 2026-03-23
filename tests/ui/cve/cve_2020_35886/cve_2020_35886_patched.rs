//@ revisions: inline regular
//@[inline] compile-flags: -Z inline-mir=true
//@[regular] compile-flags: -Z inline-mir=false
//@check-pass
use std::alloc::{Layout, alloc, alloc_zeroed, dealloc};
use std::ops::{Index, IndexMut, Range};

pub struct Array<T> {
    size: usize,
    ptr: *mut T,
}

unsafe impl<T: Sync> Sync for Array<T> {}
unsafe impl<T: Send> Send for Array<T> {}

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

impl<T> Drop for Array<T> {
    fn drop(&mut self) {
        let objsize = std::mem::size_of::<T>();
        let layout = Layout::from_size_align(self.size * objsize, 8).unwrap();
        unsafe {
            dealloc(self.ptr as *mut u8, layout);
        }
    }
}

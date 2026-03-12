extern crate libc;

use std::ops::Index;
use std::ptr;

pub struct Slab<T> {
    capacity: usize,
    len: usize,
    mem: *mut T,
}

impl<T> Drop for Slab<T> {
    fn drop(&mut self) {
        for x in 0..self.len {
            unsafe {
                let elem_ptr = self.mem.offset(x as isize);
                //~^ ptr_offset_with_cast
                //~| HELP: if you’re always increasing the pointer address, you can avoid the numeric cast by using the `add` method instead.
                //~| HELP: to override `-D warnings` add `#[allow(rpl::ptr_offset_with_cast)]`
                ptr::drop_in_place(elem_ptr);
                std::hint::black_box(elem_ptr);
            }
        }
        unsafe { libc::free(self.mem as *mut _ as *mut libc::c_void) };
    }
}

impl<T> Index<usize> for Slab<T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        unsafe { &(*(self.mem.offset(index as isize))) }
        //~^ERROR: it is an undefined behavior to offset a pointer using an unchecked integer
        //~| HELP:  check whether it's in bound before offsetting
        //~| HELP:  to override `-D warnings` add `#[allow(rpl::unchecked_pointer_offset)]`
        //~| ptr_offset_with_cast
        //~| HELP: if you’re always increasing the pointer address, you can avoid the numeric cast by using the `add` method instead.
    }
}

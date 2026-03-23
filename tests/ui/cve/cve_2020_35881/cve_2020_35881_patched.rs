//@check-pass
use std::mem;

pub unsafe fn get_data<T: ?Sized>(val: *const T) -> *const () {
    val as *const ()
}

pub unsafe fn get_data_mut<T: ?Sized>(mut val: *mut T) -> *mut () {
    val as *mut ()
}

fn main() {}

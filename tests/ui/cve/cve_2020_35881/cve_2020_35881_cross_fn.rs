use std::mem;

unsafe fn transmute_ptr<T: ?Sized, U: ?Sized>(ptr: *const T) -> *const U {
    unsafe { *mem::transmute::<*const *const T, *const *const U>(&ptr) }
}

unsafe fn transmute_mut_ptr<T: ?Sized, U: ?Sized>(mut ptr: *mut T) -> *mut U {
    unsafe { *mem::transmute::<*mut *mut T, *mut *mut U>(&mut ptr) }
}

pub unsafe fn get_data<T: ?Sized>(val: *const T) -> *const () {
    unsafe { transmute_ptr(val) }
    //~^ ERROR: wrong assumption of fat pointer layout
}

pub unsafe fn get_data_mut<T: ?Sized>(mut val: *mut T) -> *mut () {
    unsafe { transmute_mut_ptr(val) }
    //~^ ERROR: wrong assumption of fat pointer layout
}

fn main() {}

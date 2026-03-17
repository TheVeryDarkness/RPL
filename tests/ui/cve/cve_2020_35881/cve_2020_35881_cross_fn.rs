use std::mem;

unsafe fn transmute_ptr<T, U>(ptr: *const T) -> *const U {
    unsafe { *mem::transmute::<*const *const T, *const *const U>(&val) }
}

unsafe fn transmute_mut_ptr<T, U>(ptr: *mut T) -> *mut U {
    unsafe { *mem::transmute::<*mut *mut T, *mut *mut U>(&mut val) }
}

pub unsafe fn get_data<T: ?Sized>(val: *const T) -> *const () {
    unsafe { transmute_ptr(ptr) }
    //~^ ERROR: wrong assumption of fat pointer layout
}

pub unsafe fn get_data_mut<T: ?Sized>(mut val: *mut T) -> *mut () {
    unsafe { transmute_mut_ptr(ptr) }
    //~^ ERROR: wrong assumption of fat pointer layout
}

fn main() {}

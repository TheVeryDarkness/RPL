use std::mem;

pub unsafe fn get_data<T: ?Sized>(val: *const T) -> *const () {
    let reference = &val;
    unsafe { *mem::transmute::<*const *const T, *const *const ()>(reference) }
    //~^ ERROR: wrong assumption of fat pointer layout
}

pub unsafe fn get_data_mut<T: ?Sized>(mut val: *mut T) -> *mut () {
    let reference = &mut val;
    unsafe { *mem::transmute::<*mut *mut T, *mut *mut ()>(reference) }
    //~^ ERROR: wrong assumption of fat pointer layout
}

fn main() {}

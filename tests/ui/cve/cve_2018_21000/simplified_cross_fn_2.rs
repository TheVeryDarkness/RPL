//@check-pass: FN
// https://github.com/nabijaczleweli/safe-transmute-rs/commit/c79ebfdb5858982af59a78df471c7cad7a78fd23
use std::mem::{forget, size_of};
use std::vec::Vec;

unsafe fn from_raw_parts<T, U>(ptr: *mut T, capacity: usize, len: usize) -> Vec<U> {
    unsafe {
        Vec::from_raw_parts(
            ptr as *mut U,
            capacity * size_of::<T>() / size_of::<U>(),
            len * size_of::<T>() / size_of::<U>(),
        )
    }
}

//#[rpl::dump_mir(dump_cfg, dump_ddg)]
pub unsafe fn guarded_transmute_vec_permissive<T>(mut bytes: Vec<u8>) -> Vec<T> {
    // PermissiveGuard::check::<T>(&bytes).unwrap();
    let ptr = bytes.as_mut_ptr();
    let capacity = bytes.capacity();
    let len = bytes.len();
    forget(bytes);
    unsafe { from_raw_parts(ptr, capacity, len) }
    //FN: ~^ ERROR: misordered parameters `len` and `cap` in `Vec::from_raw_parts`
}

// #[rpl::dump_mir(dump_cfg, dump_ddg)]
pub unsafe fn guarded_transmute_to_bytes_vec<T>(mut from: Vec<T>) -> Vec<u8> {
    // PermissiveGuard::check::<T>(&bytes).unwrap();
    let ptr = from.as_mut_ptr();
    let capacity = from.capacity();
    let len = from.len();
    forget(from);
    unsafe { from_raw_parts(ptr, capacity, len) }
    //FN: ~^ ERROR: misordered parameters `len` and `cap` in `Vec::from_raw_parts`
}

fn main() {}

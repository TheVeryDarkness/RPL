//@check-pass: FN
// https://github.com/nabijaczleweli/safe-transmute-rs/commit/c79ebfdb5858982af59a78df471c7cad7a78fd23
use std::mem::{forget, size_of};
use std::vec::Vec;

unsafe fn cast_vec<T, U>(mut vec: Vec<T>) -> Vec<U> {
    let ptr = vec.as_mut_ptr() as *mut U;
    let capacity = vec.capacity() * size_of::<T>() / size_of::<U>();
    let len = vec.len() * size_of::<T>() / size_of::<U>();
    forget(vec);
    unsafe { Vec::from_raw_parts(ptr, capacity, len) }
}

//#[rpl::dump_mir(dump_cfg, dump_ddg)]
pub unsafe fn guarded_transmute_vec_permissive<T>(mut bytes: Vec<u8>) -> Vec<T> {
    // PermissiveGuard::check::<T>(&bytes).unwrap();
    unsafe { cast_vec(bytes) }
    //FN: ~^ ERROR: misordered parameters `len` and `cap` in `Vec::from_raw_parts`
}

// #[rpl::dump_mir(dump_cfg, dump_ddg)]
pub unsafe fn guarded_transmute_to_bytes_vec<T>(mut from: Vec<T>) -> Vec<u8> {
    // PermissiveGuard::check::<T>(&bytes).unwrap();
    unsafe { cast_vec(from) }
    //FN: ~^ ERROR: misordered parameters `len` and `cap` in `Vec::from_raw_parts`
}

fn main() {}

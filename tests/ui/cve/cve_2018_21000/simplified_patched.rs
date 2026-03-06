//@check-pass

// https://github.com/nabijaczleweli/safe-transmute-rs/commit/a134e06d740f9d7c287f74c0af2cd06206774364
#![allow(unsafe_op_in_unsafe_fn)]

use std::mem::{forget, size_of};
use std::vec::Vec;

// #[rpl::dump_mir(dump_cfg, dump_ddg)]
pub unsafe fn guarded_transmute_vec_permissive<T>(mut bytes: Vec<u8>) -> Vec<T> {
    // PermissiveGuard::check::<T>(&bytes).unwrap();
    let ptr = bytes.as_mut_ptr();
    let capacity = bytes.capacity() / size_of::<T>();
    let len = bytes.len() / size_of::<T>();
    forget(bytes);
    Vec::from_raw_parts(ptr as *mut T, len, capacity)
}

// #[rpl::dump_mir(dump_cfg, dump_ddg)]
pub unsafe fn guarded_transmute_to_bytes_vec<T>(mut from: Vec<T>) -> Vec<u8> {
    let capacity = from.capacity() * size_of::<T>();
    let len = from.len() * size_of::<T>();
    let ptr = from.as_mut_ptr();
    forget(from);
    Vec::from_raw_parts(ptr as *mut u8, len, capacity)
}

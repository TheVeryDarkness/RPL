//@rustc-env: RPL_PATS=tests/ui/features/flows_to/flows_to.rpl
//@compile-flags: -Z inline-mir=false

#![allow(dead_code)]
#![allow(forgetting_copy_types)]

use std::ptr;

/// TP for `flows_to`: read then forget the same local.
fn flows_to_tp<T>(p: *const T) {
    let v = unsafe { ptr::read(p) };
    let _x = 0usize;
    std::mem::forget(v);
    //~^ ERROR: ownership duplicated by `ptr::read` then consumed (flows_to check)
}

/// TN for `flows_to`: forget a different value (no DDG edge from `$v`).
fn flows_to_tn<T: Default>(p: *const T) {
    let v = unsafe { ptr::read(p) };
    std::mem::forget(T::default());
    std::mem::drop(v);
}

fn main() {
    let x = String::from("x");
    flows_to_tp(&x as *const String);
    flows_to_tn(&x as *const String);
}

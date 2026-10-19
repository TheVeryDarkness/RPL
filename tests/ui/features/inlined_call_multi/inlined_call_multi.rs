//@rustc-env: RPL_PATS=tests/ui/features/inlined_call_multi/inlined_call_multi.rpl

//! Two inlined `Vec::set_len` calls in one function: the matcher should find both.

#![allow(dead_code)]

#[inline(never)]
fn foo(v: &mut Vec<u8>) {
    unsafe {
        v.set_len(1);
        //~^ ERROR: inlined `Vec::set_len` detected
        v.set_len(2);
        //~^ ERROR: inlined `Vec::set_len` detected
    }
}

fn main() {
    let mut v = vec![1u8, 2, 3];
    foo(&mut v);
}

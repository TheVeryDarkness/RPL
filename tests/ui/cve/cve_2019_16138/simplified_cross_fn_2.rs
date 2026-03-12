//@revisions: inline regular
//@[inline]compile-flags: -Z inline-mir=true
//@[regular]compile-flags: -Z inline-mir=false
//@[inline]check-pass: we don't have a pattern for this case
//@[regular]check-pass: FN

// #[rpl::dump_mir(dump_cfg, dump_ddg)]
fn foo() {
    let pixel_count = 1920 * 1080;
    let mut ret: Vec<(u8, u8, u8)> = Vec::with_capacity(pixel_count);
    //FN: ~[regular]^ uninit_vec
    unsafe {
        set_len(&mut ret, pixel_count);
        //FN: ~[regular]^ERROR: it violates the precondition of `Vec::set_len` to extend a `Vec`'s length without initializing its content in advance
    }
}

unsafe fn set_len<T>(v: &mut Vec<T>, n: usize) {
    unsafe {
        v.set_len(n);
    }
}

fn main() {
    foo()
}

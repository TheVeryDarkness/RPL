//@revisions: inline regular
//@[inline]compile-flags: -Z inline-mir=true
//@[regular]compile-flags: -Z inline-mir=false
//@[inline]check-pass: we don't have a pattern for this case
//@[regular]check-pass: FN

// #[rpl::dump_mir(dump_cfg, dump_ddg)]
fn foo() {
    let pixel_count = 1920 * 1080;
    let mut ret: Vec<(u8, u8, u8)> = new_vec(pixel_count);
    //FN: ~[regular]^ uninit_vec
    unsafe {
        ret.set_len(pixel_count);
        //FN: ~[regular]^ERROR: it violates the precondition of `Vec::set_len` to extend a `Vec`'s length without initializing its content in advance
    }
}

fn new_vec<T>(n: usize) -> Vec<T> {
    Vec::with_capacity(n)
}

fn main() {
    foo()
}

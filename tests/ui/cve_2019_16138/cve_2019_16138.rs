//@compile-flags: -Z inline-mir=false

// #[rpl::dump_mir(dump_cfg, dump_ddg)]
fn foo() {
    let pixel_count = 1920 * 1080;
    let mut ret: Vec<(u8, u8, u8)> = Vec::with_capacity(pixel_count);
    unsafe {
        ret.set_len(pixel_count);
        //~^ERROR: it violates the precondition of `std::vec::Vec::set_len` without initializing its content
    }
}

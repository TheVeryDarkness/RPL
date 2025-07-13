//@revisions: inline normal
//@compile-flags: -Z mir-opt-level=0
//@[inline]compile-flags: -Z inline-mir=true
//@[normal]compile-flags: -Z inline-mir=false

// #[rpl::dump_mir(dump_cfg, dump_ddg)]
fn main() {
    unsafe {
        let m = &mut () as *mut ();
        m.offset(0);
        //~^ zst_offset

        m.wrapping_add(0);
        //~^ zst_offset

        m.sub(0);
        //~^ zst_offset

        m.wrapping_sub(0);
        //~^ zst_offset

        let c = &() as *const ();
        c.offset(0);
        //~^ zst_offset
        //~[inline]| unchecked_pointer_offset

        c.wrapping_add(0);
        //~^ zst_offset

        c.sub(0);
        //~^ zst_offset
        //~[inline]| unchecked_pointer_offset

        c.wrapping_sub(0);
        //~^ zst_offset

        let sized = &1 as *const i32;
        sized.offset(0);
        //~[inline]^ unchecked_pointer_offset
    }
}

//! Minimal CVE-2020-25795 / sized-chunks `insert_from` panic-safety motif:
//! relocate elements (holes), then user/generic code may panic before fill.

#![allow(dead_code)]

use std::ptr;

/// Stand-in for `Iterator::next` (MIR uses Qself for the trait method directly).
#[inline(never)]
fn take_next<A, I: Iterator<Item = A>>(iter: &mut I) -> Option<A> {
    iter.next()
}

/// Simplified `Chunk::insert_from` shape: copy, then consume iterator into holes.
pub fn insert_from<A, I>(dst: *mut A, src: *const A, count: usize, iter: &mut I)
where
    I: Iterator<Item = A>,
{
    unsafe {
        ptr::copy_nonoverlapping(src, dst, count);
        let mut write = dst;
        loop {
            let opt = take_next(iter);
            //~^ ERROR: calling a potentially panicking iterator after relocating elements may leave chunk holes on unwind
            match opt {
                Some(value) => {
                    ptr::write(write, value);
                    write = write.add(1);
                }
                None => break,
            }
        }
    }
}

fn main() {
    let mut buf = [0u8; 4];
    let src = [1u8, 2];
    let mut it = [9u8, 8].into_iter();
    insert_from(buf.as_mut_ptr(), src.as_ptr(), 2, &mut it);
}

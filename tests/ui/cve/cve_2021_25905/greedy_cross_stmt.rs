//@ revisions: inline regular
//@[inline] compile-flags: -Z inline-mir=true
//@[regular] compile-flags: -Z inline-mir=false
//@[inline] check-pass: no pattern yet
//@[regular] check-pass: FN
use std::io::{Read, Result as IoResult};

pub struct GreedyAccessReader<R> {
    inner: R,
    buf: Vec<u8>,
    consumed: usize,
}

impl<R> GreedyAccessReader<R>
where
    R: Read,
{
    fn reserve_up_to(&mut self, index: usize) {
        let mut new_size = 16;
        while new_size < index || new_size < self.buf.capacity() {
            new_size *= 2;
        }
        let additional = new_size - self.buf.capacity();
        if additional > 0 {
            self.buf.reserve(additional);
        }
    }

    pub fn fill_buf(&mut self) -> IoResult<&[u8]> {
        if self.buf.capacity() == self.consumed {
            self.reserve_up_to(self.buf.capacity() + 16);
        }

        let b = self.buf.len();
        let buf = &mut self.buf;
        let buf = unsafe {
            // safe because it's within the buffer's limits
            // and we won't be reading uninitialized memory
            std::slice::from_raw_parts_mut(buf.as_mut_ptr().add(b), buf.capacity() - b)
            //FN: ~[regular]^ERROR: it violates the precondition of `std::slice::from_raw_parts_mut` to create a slice from uninitialized part of a `Vec`
        };

        match self.inner.read(buf) {
            Ok(o) => {
                unsafe {
                    // reset the size to include the written portion,
                    // safe because the extra data is initialized
                    self.buf.set_len(b + o);
                }

                Ok(&self.buf[self.consumed..])
            }
            Err(e) => Err(e),
        }
    }
}

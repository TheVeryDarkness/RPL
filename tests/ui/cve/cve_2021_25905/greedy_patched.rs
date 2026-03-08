//@ revisions: inline regular
//@[inline] compile-flags: -Z inline-mir=true
//@[regular] compile-flags: -Z inline-mir=false
//@check-pass
use std::io::{Read, Result as IoResult};

#[derive(Debug, Clone)]
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

    fn fill_buf(&mut self) -> IoResult<&[u8]> {
        if self.buf.capacity() == self.consumed {
            self.reserve_up_to(self.buf.capacity() + 16);
        }

        let b = self.buf.len();
        self.buf.resize(self.buf.capacity(), 0);
        let buf = &mut self.buf[b..];
        let o = self.inner.read(buf)?;

        // truncate to exclude non-written portion
        self.buf.truncate(b + o);

        Ok(&self.buf[self.consumed..])
    }
}

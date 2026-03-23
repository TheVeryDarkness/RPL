//@check-pass
extern crate bytes;
extern crate tokio;
extern crate tokio_util;

use std::pin::Pin;
use std::task::{Context, Poll, ready};
use std::{fmt, io};

use bytes::{Buf, BytesMut};
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder};

const LW: usize = 1024;
const HW: usize = 8 * 1024;

struct Flags(u8);

impl Flags {
    const EOF: Flags = Flags(0b0001);
    const READABLE: Flags = Flags(0b0010);

    fn contains(&self, other: Flags) -> bool {
        (self.0 & other.0) == other.0
    }

    fn insert(&mut self, other: Flags) {
        self.0 |= other.0;
    }

    fn remove(&mut self, other: Flags) {
        self.0 &= !other.0;
    }
}

/// A unified `Stream` and `Sink` interface to an underlying I/O object, using
/// the `Encoder` and `Decoder` traits to encode and decode frames.
#[pin_project]
pub struct Framed<T, U> {
    #[pin]
    io: T,
    codec: U,
    flags: Flags,
    read_buf: BytesMut,
    write_buf: BytesMut,
}

impl<T, U> Framed<T, U> {
    /// Try to read underlying I/O stream and decode item.
    // #[rpl::dump_mir(dump_cfg, dump_ddg)]
    pub fn next_item(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<U::Item, U::Error>>>
    where
        T: AsyncRead,
        U: Decoder,
    {
        loop {
            let mut this = self.as_mut().project();
            // Repeatedly call `decode` or `decode_eof` as long as it is
            // "readable". Readable is defined as not having returned `None`. If
            // the upstream has returned EOF, and the decoder is no longer
            // readable, it can be assumed that the decoder will never become
            // readable again, at which point the stream is terminated.

            if this.flags.contains(Flags::READABLE) {
                if this.flags.contains(Flags::EOF) {
                    match this.codec.decode_eof(&mut this.read_buf) {
                        Ok(Some(frame)) => return Poll::Ready(Some(Ok(frame))),
                        Ok(None) => return Poll::Ready(None),
                        Err(e) => return Poll::Ready(Some(Err(e))),
                    }
                }

                log::trace!("attempting to decode a frame");

                match this.codec.decode(&mut this.read_buf) {
                    Ok(Some(frame)) => {
                        log::trace!("frame decoded from buffer");
                        return Poll::Ready(Some(Ok(frame)));
                    }
                    Err(e) => return Poll::Ready(Some(Err(e))),
                    _ => (), // Need more data
                }

                this.flags.remove(Flags::READABLE);
            }

            debug_assert!(!this.flags.contains(Flags::EOF));

            // Otherwise, try to read more data and try again. Make sure we've got room
            let remaining = this.read_buf.capacity() - this.read_buf.len();
            if remaining < LW {
                this.read_buf.reserve(HW - remaining)
            }
            let cnt = match this.io.poll_read_buf(cx, &mut this.read_buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(e.into()))),
                Poll::Ready(Ok(cnt)) => cnt,
            };

            if cnt == 0 {
                this.flags.insert(Flags::EOF);
            }
            this.flags.insert(Flags::READABLE);
        }
    }

    /// Flush write buffer to underlying I/O stream.
    pub fn flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), U::Error>>
    where
        T: AsyncWrite,
        U: Encoder,
    {
        let mut this = self.as_mut().project();
        log::trace!("flushing framed transport");

        while !this.write_buf.is_empty() {
            log::trace!("writing; remaining={}", this.write_buf.len());

            let n = ready!(this.io.as_mut().poll_write(cx, this.write_buf))?;

            if n == 0 {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write frame to transport",
                )
                .into()));
            }

            // remove written data
            this.write_buf.advance(n);
        }

        // Try flushing the underlying IO
        ready!(this.io.poll_flush(cx))?;

        log::trace!("framed transport flushed");
        Poll::Ready(Ok(()))
    }

    /// Flush write buffer and shutdown underlying I/O stream.
    // #[rpl::dump_mir(dump_cfg, dump_ddg)]
    pub fn close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), U::Error>>
    where
        T: AsyncWrite,
        U: Encoder,
    {
        let mut this = self.as_mut().project();
        ready!(this.io.as_mut().poll_flush(cx))?;
        ready!(this.io.as_mut().poll_shutdown(cx))?;

        Poll::Ready(Ok(()))
    }
}

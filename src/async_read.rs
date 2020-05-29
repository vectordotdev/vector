use bytes::BufMut;
use futures01::{Async, Future, IntoFuture, Poll};
use std::io::Read;
use tokio01::io::AsyncRead;

/// A AsyncRead combinator which reads from a reader until a future resolves.
///
/// This structure is produced by the [`AsyncReadExt::read_until`] method.
#[derive(Clone, Debug)]
pub struct AllowReadUntil<S, F, O> {
    reader: S,
    until: F,
    until_res: Option<O>,
    free: bool,
}

/// This `AsyncRead` extension trait provides a `read_until` method that terminates the reader once
/// the given future resolves.
pub trait AsyncAllowReadExt: AsyncRead {
    /// Read data from this reader until the given future resolves.
    ///
    /// If the future produces an error, the read will be allowed to continue indefinitely.
    fn allow_read_until<U, O>(self, until: U) -> AllowReadUntil<Self, U::Future, O>
    where
        U: IntoFuture<Item = O, Error = ()>,
        Self: Sized,
    {
        AllowReadUntil {
            reader: self,
            until: until.into_future(),
            until_res: None,
            free: false,
        }
    }
}

impl<S> AsyncAllowReadExt for S where S: AsyncRead {}

impl<S, F, O> Read for AllowReadUntil<S, F, O>
where
    S: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        self.reader.read(buf)
    }
}

impl<S, F, O> AsyncRead for AllowReadUntil<S, F, O>
where
    S: AsyncRead,
    F: Future<Item = O, Error = ()>,
{
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.reader.prepare_uninitialized_buffer(buf)
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, std::io::Error>
    where
        Self: Sized,
    {
        if !self.free {
            match self.until.poll() {
                Ok(Async::Ready(res)) => {
                    // future resolved -- terminate reader
                    self.until_res = Some(res);
                    return Ok(Async::Ready(0));
                }
                Err(_) => {
                    // future failed -- unclear whether we should stop or continue?
                    // to provide a mechanism for the creator to let the stream run forever,
                    // we interpret this as "run forever".
                    self.free = true;
                }
                Ok(Async::NotReady) => {}
            }
        }

        self.reader.read_buf(buf)
    }
}

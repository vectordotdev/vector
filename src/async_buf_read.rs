use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf, Result as IoResult};

pub trait VecAsyncBufReadExt: AsyncRead + AsyncBufRead {
    /// Allow reading data from this reader until the given future resolves.
    fn allow_read_until<F>(self, until: F) -> AllowReadUntil<Self, F>
    where
        Self: Sized,
        F: Future<Output = ()>,
    {
        AllowReadUntil {
            reader: self,
            until,
        }
    }
}

impl<S> VecAsyncBufReadExt for S where S: AsyncRead + AsyncBufRead {}

/// A AsyncRead combinator which reads from a reader until a future resolves.
#[pin_project]
#[derive(Clone, Debug)]
pub struct AllowReadUntil<S, F> {
    #[pin]
    reader: S,
    #[pin]
    until: F,
}

impl<S, F> AllowReadUntil<S, F> {
    pub fn get_ref(&self) -> &S {
        &self.reader
    }

    pub fn get_mut(&mut self) -> &mut S {
        &mut self.reader
    }
}

impl<S, F> AsyncRead for AllowReadUntil<S, F>
where
    S: AsyncRead + AsyncBufRead,
    F: Future<Output = ()>,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<IoResult<()>> {
        let this = self.project();
        match this.until.poll(cx) {
            Poll::Ready(_) => Poll::Ready(Ok(())),
            Poll::Pending => this.reader.poll_read(cx, buf),
        }
    }
}

impl<S, F> AsyncBufRead for AllowReadUntil<S, F>
where
    S: AsyncRead + AsyncBufRead,
    F: Future<Output = ()>,
{
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<&[u8]>> {
        let this = self.project();
        this.reader.poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let this = self.project();
        this.reader.consume(amt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shutdown::ShutdownSignal;
    use futures::FutureExt;
    use std::io::Cursor;
    use tokio::io::{AsyncBufReadExt, BufReader};

    #[tokio::test]
    async fn test_read_line_without_shutdown() {
        let buf = Cursor::new("First line\nSecond line\n");
        let reader = BufReader::new(buf);
        let shutdown = ShutdownSignal::noop();

        let mut reader = reader.allow_read_until(shutdown.clone().map(|_| ()));

        // Test one of the AsyncBufRead extension functions
        let mut line_one = String::new();
        let _ = reader.read_line(&mut line_one).await;

        assert_eq!("First line\n", line_one);

        let mut line_two = String::new();
        let _ = reader.read_line(&mut line_two).await;

        assert_eq!("Second line\n", line_two);
    }
}

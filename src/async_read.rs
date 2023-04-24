#![allow(missing_docs)]
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project::pin_project;
use tokio::io::{AsyncRead, ReadBuf, Result as IoResult};

pub trait VecAsyncReadExt: AsyncRead {
    /// Read data from this reader until the given future resolves.
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

impl<S> VecAsyncReadExt for S where S: AsyncRead {}

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
    pub const fn get_ref(&self) -> &S {
        &self.reader
    }

    pub fn get_mut(&mut self) -> &mut S {
        &mut self.reader
    }
}

impl<S, F> AsyncRead for AllowReadUntil<S, F>
where
    S: AsyncRead,
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

#[cfg(test)]
mod tests {
    use futures::FutureExt;
    use tokio::{
        fs::{remove_file, File},
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    };

    use super::*;
    use crate::{shutdown::ShutdownSignal, test_util::temp_file};

    #[tokio::test]
    async fn test_read_line_without_shutdown() {
        let shutdown = ShutdownSignal::noop();
        let temp_path = temp_file();
        let write_file = File::create(temp_path.clone()).await.unwrap();
        let read_file = File::open(temp_path.clone()).await.unwrap();

        // Wrapper AsyncRead
        let read_file = read_file.allow_read_until(shutdown.clone().map(|_| ()));

        let mut reader = BufReader::new(read_file);
        let mut writer = BufWriter::new(write_file);

        writer.write_all(b"First line\n").await.unwrap();
        writer.flush().await.unwrap();

        // Test one of the AsyncBufRead extension functions
        let mut line_one = String::new();
        _ = reader.read_line(&mut line_one).await;

        assert_eq!("First line\n", line_one);

        writer.write_all(b"Second line\n").await.unwrap();
        writer.flush().await.unwrap();

        let mut line_two = String::new();
        _ = reader.read_line(&mut line_two).await;

        assert_eq!("Second line\n", line_two);

        remove_file(temp_path).await.unwrap();
    }

    #[tokio::test]
    async fn test_read_line_with_shutdown() {
        let (trigger_shutdown, shutdown, _) = ShutdownSignal::new_wired();
        let temp_path = temp_file();
        let write_file = File::create(temp_path.clone()).await.unwrap();
        let read_file = File::open(temp_path.clone()).await.unwrap();

        // Wrapper AsyncRead
        let read_file = read_file.allow_read_until(shutdown.clone().map(|_| ()));

        let mut reader = BufReader::new(read_file);
        let mut writer = BufWriter::new(write_file);

        writer.write_all(b"First line\n").await.unwrap();
        writer.flush().await.unwrap();

        // Test one of the AsyncBufRead extension functions
        let mut line_one = String::new();
        _ = reader.read_line(&mut line_one).await;

        assert_eq!("First line\n", line_one);

        drop(trigger_shutdown);

        writer.write_all(b"Second line\n").await.unwrap();
        writer.flush().await.unwrap();

        let mut line_two = String::new();
        _ = reader.read_line(&mut line_two).await;

        assert_eq!("", line_two);

        remove_file(temp_path).await.unwrap();
    }
}

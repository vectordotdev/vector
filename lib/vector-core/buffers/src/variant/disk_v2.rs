use std::error::Error;
use std::mem;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::{ready, Sink, Stream};
use pin_project::pin_project;
use tokio_util::sync::ReusableBoxFuture;

use crate::buffer_usage_data::BufferUsageHandle;
use crate::disk_v2::{Buffer, DiskBufferConfig, Reader, Writer, WriterError};
use crate::topology::channel::{ReceiverAdapter, SenderAdapter};
use crate::{topology::builder::IntoBuffer, Acker, Bufferable};

pub struct DiskV2Buffer {
    id: String,
    data_dir: PathBuf,
    max_size: usize,
}

impl DiskV2Buffer {
    pub fn new(id: String, data_dir: PathBuf, max_size: usize) -> Self {
        Self {
            id,
            data_dir,
            max_size,
        }
    }
}

#[async_trait]
impl<T> IntoBuffer<T> for DiskV2Buffer
where
    T: Bufferable + Clone,
{
    async fn into_buffer_parts(
        self: Box<Self>,
        usage_handle: &BufferUsageHandle,
    ) -> Result<(SenderAdapter<T>, ReceiverAdapter<T>, Option<Acker>), Box<dyn Error + Send + Sync>>
    {
        usage_handle.set_buffer_limits(Some(self.max_size), None);

        // Create the actual buffer subcomponents.
        let buffer_path = self.data_dir.join(self.id);
        let config = DiskBufferConfig::from_path(buffer_path)
            .max_buffer_size(self.max_size as u64)
            .build();
        let (writer, reader, acker) = Buffer::from_config(config).await?;

        let wrapped_reader = WrappedReader::new(reader);
        let wrapped_writer = WrappedWriter::new(writer);

        Ok((
            SenderAdapter::opaque(wrapped_writer),
            ReceiverAdapter::opaque(wrapped_reader),
            Some(acker),
        ))
    }
}

#[pin_project]
struct WrappedReader<T> {
    #[pin]
    reader: Option<Reader<T>>,
    read_future: ReusableBoxFuture<(Reader<T>, Option<T>)>,
}

impl<T> WrappedReader<T>
where
    T: Bufferable,
{
    pub fn new(reader: Reader<T>) -> Self {
        Self {
            reader: Some(reader),
            read_future: ReusableBoxFuture::new(make_read_future(None)),
        }
    }
}

impl<T> Stream for WrappedReader<T>
where
    T: Bufferable,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        loop {
            match this.reader.as_mut().get_mut().take() {
                None => {
                    let (reader, result) = ready!(this.read_future.poll(cx));
                    this.reader.set(Some(reader));
                    return Poll::Ready(result);
                }
                Some(reader) => this.read_future.set(make_read_future(Some(reader))),
            }
        }
    }
}

enum WriterState<T> {
    Inconsistent,
    Idle(Writer<T>),
    Writing,
    Flushing,
}

impl<T> WriterState<T> {
    fn is_idle(&self) -> bool {
        matches!(self, WriterState::Idle(..))
    }
}

// TODO: it's even less likely that we need to truly kill the writer task for an error
// unless it's a very specific type of error... we already distinguish failed
// encoding/serialization which occurs before any actual bytes hit the file at all, or
// before we update the ledger or any of that.
//
// so really we'd be down to like... certain I/O errors that we know we can't recover
// from.
//
// where this could really get tricky is like, if we try to write a record here and the
// permissions got messed up, so we couldn't write to the file, we could _theoretically_
// loop and try it again until it works, or we could just drop the event and move on...
// not sure which one is better.
#[pin_project]
struct WrappedWriter<T>
where
    T: Bufferable,
{
    state: WriterState<T>,
    write_future: ReusableBoxFuture<(Writer<T>, Result<usize, WriterError<T>>)>,
    flush_future: ReusableBoxFuture<(Writer<T>, Result<(), WriterError<T>>)>,
}

impl<T> WrappedWriter<T>
where
    T: Bufferable,
{
    pub fn new(writer: Writer<T>) -> Self {
        Self {
            state: WriterState::Idle(writer),
            write_future: ReusableBoxFuture::new(make_write_future(None, None)),
            flush_future: ReusableBoxFuture::new(make_flush_future(None)),
        }
    }

    fn try_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), WriterError<T>>> {
        loop {
            if self.state.is_idle() {
                return Poll::Ready(Ok(()));
            }

            if let Err(e) = ready!(self.drive_pending_operation(cx)) {
                return Poll::Ready(Err(e));
            }
        }
    }

    fn drive_pending_operation(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), WriterError<T>>> {
        match self.state {
            WriterState::Inconsistent | WriterState::Idle(..) => {
                unreachable!("writer state not expected")
            }
            WriterState::Writing => self.drive_write_operation(cx),
            WriterState::Flushing => self.drive_flush_operation(cx),
        }
    }

    fn drive_write_operation(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), WriterError<T>>> {
        match self.state {
            WriterState::Writing => {
                let (writer, result) = ready!(self.write_future.poll(cx));
                self.state = WriterState::Idle(writer);
                // TODO: do we _need_ to reset the future here?
                Poll::Ready(result.map(|_| ()))
            }
            _ => unreachable!("writer state not expected"),
        }
    }

    fn drive_flush_operation(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), WriterError<T>>> {
        match self.state {
            WriterState::Flushing => {
                let (writer, result) = ready!(self.flush_future.poll(cx));
                self.state = WriterState::Idle(writer);
                // TODO: do we _need_ to reset the future here?
                Poll::Ready(result.map(|_| ()))
            }
            _ => unreachable!("writer state not expected"),
        }
    }
}

impl<T> Sink<T> for WrappedWriter<T>
where
    T: Bufferable,
{
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_mut().try_ready(cx).map_err(|e| {
            error!("{}", e);
            ()
        })
    }

    fn start_send(mut self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        if let WriterState::Idle(writer) = mem::replace(&mut self.state, WriterState::Inconsistent)
        {
            self.write_future
                .set(make_write_future(Some(writer), Some(item)));
            self.state = WriterState::Writing;
            Ok(())
        } else {
            unreachable!("writer state not expected")
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            match mem::replace(&mut self.state, WriterState::Inconsistent) {
                WriterState::Inconsistent => unreachable!("writer state not expected"),
                WriterState::Idle(writer) => {
                    self.flush_future.set(make_flush_future(Some(writer)));
                    self.state = WriterState::Flushing;
                }
                // `drive_write_operation` updates the state for us, so we'll be in a
                // consistent state even with the early return here
                WriterState::Writing => match self.drive_write_operation(cx) {
                    // not done yet, maintain original state
                    Poll::Pending => {
                        self.state = WriterState::Writing;
                        return Poll::Pending;
                    }
                    // done with write, only return if error encountered
                    Poll::Ready(result) => {
                        if let Err(e) = result {
                            error!("{}", e);
                            return Poll::Ready(Err(()));
                        }
                    }
                },
                // `drive_flush_operation` updates the state for us, so we'll be in a
                // consistent state even with the early return here
                WriterState::Flushing => match self.drive_flush_operation(cx) {
                    // not done yet, maintain original state
                    Poll::Pending => {
                        self.state = WriterState::Flushing;
                        return Poll::Pending;
                    }
                    // all done with flush, return result regardless
                    Poll::Ready(result) => {
                        return Poll::Ready(result.map_err(|e| {
                            error!("{}", e);
                            ()
                        }))
                    }
                },
            }
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // try to finish any in-flight write, and ensure we complete one last flush
        if let Err(e) = ready!(self.as_mut().poll_flush(cx)) {
            return Poll::Ready(Err(e));
        }

        // now actually close the writer
        match mem::replace(&mut self.state, WriterState::Inconsistent) {
            WriterState::Idle(mut writer) => {
                writer.close();
                Poll::Ready(Ok(()))
            }
            _ => unreachable!("writer state not expected"),
        }
    }
}

async fn make_read_future<T>(reader: Option<Reader<T>>) -> (Reader<T>, Option<T>)
where
    T: Bufferable,
{
    match reader {
        None => unreachable!("future should not be called in this state"),
        Some(mut reader) => {
            let result = match reader.next().await {
                Ok(result) => result,
                Err(e) => {
                    // TODO: we can _probably_ avoid having to actually kill the task here,
                    // because the reader will recover from read errors, but, things it won't
                    // automagically recover from:
                    // - if it rolls to the next data file mid-data file, the writer might still be
                    //   writing more records to the current data file, which means we might stall
                    //   reads until the writer needs to roll to the next data file:
                    //
                    //   maybe there's an easy way we could propagate the rollover events to the
                    //   writer to also get it to rollover?  again, more of a technique to minimize
                    //   the number of records we throw away by rolling over.  this could be tricky
                    //   to accomplish, though, for in-flight readers, but it's just a thought in a
                    //   code comment for now.
                    //
                    // - actual I/O errors like a failed read or permissions or whatever:
                    //
                    //   we haven't fully quantified what it means for the reader to get an
                    //   I/O error during a read, since we could end up in an inconsistent state if
                    //   the I/O error came mid-record read, after already reading some amount of
                    //   data and then losing our place by having the "wait for the data" code break
                    //   out with the I/O error.
                    //
                    //   this could be a potential enhancement to the reader where we also use the
                    //   "bytes read" value as the position in the data file, and track error state
                    //   internally, such that any read that was interrupted by a true I/O error
                    //   will set the error state and inform the next call to `try_read_record` to
                    //   seek back to the position prior to the read and to clear the read buffers,
                    //   enabling a clean-slate attempt.
                    //
                    //   regardless, such an approach might only be acheivable for specific I/O
                    //   errors and we could _potentially_ end up spamming the logs i.e. if a file
                    //   has its permissions modified and it just keeps absolutely blasting the logs
                    //   with the above error that we got from the reader.. maybe it's better to
                    //   spam the logs to indicate an error if it's possible to fix it? the reader
                    //   _could_ pick back up if permissions were fixed, etc...
                    error!("error during disk buffer read: {}", e);
                    None
                }
            };

            (reader, result)
        }
    }
}

async fn make_write_future<T>(
    writer: Option<Writer<T>>,
    record: Option<T>,
) -> (Writer<T>, Result<usize, WriterError<T>>)
where
    T: Bufferable,
{
    // TODO: it's even less likely that we need to truly kill the writer task for an error
    // unless it's a very specific type of error... we already distinguish failed
    // encoding/serialization which occurs before any actual bytes hit the file at all, or
    // before we update the ledger or any of that.
    //
    // so really we'd be down to like... certain I/O errors that we know we can't recover
    // from.
    //
    // where this could really get tricky is like, if we try to write a record here and the
    // permissions got messed up, so we couldn't write to the file, we could _theoretically_
    // loop and try it again until it works, or we could just drop the event and move on...
    // not sure which one is better.
    match (writer, record) {
        (Some(mut writer), Some(record)) => {
            let result = writer.write_record(record).await;
            (writer, result)
        }
        _ => unreachable!("future should not be called in this state"),
    }
}

async fn make_flush_future<T>(writer: Option<Writer<T>>) -> (Writer<T>, Result<(), WriterError<T>>)
where
    T: Bufferable,
{
    match writer {
        Some(mut writer) => {
            let result = writer.flush().await;
            (writer, result.map_err(Into::into))
        }
        None => unreachable!("future should not be called in this state"),
    }
}

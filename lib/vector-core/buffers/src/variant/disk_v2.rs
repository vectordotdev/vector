use std::collections::VecDeque;
use std::error::Error;
use std::mem;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::{ready, Sink, Stream};
use pin_project::pin_project;
use tokio::sync::mpsc::{channel, Receiver};
use tokio_util::sync::ReusableBoxFuture;

use crate::buffer_usage_data::BufferUsageHandle;
use crate::disk_v2::{Buffer, DiskBufferConfig, Reader, Writer, WriterError};
use crate::topology::channel::{ReceiverAdapter, SenderAdapter};
use crate::{topology::builder::IntoBuffer, Acker, Bufferable};

const MAX_BUFFERED_ITEMS: usize = 128;

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
        //let wrapped_writer = WrappedWriter::new(writer);

        let (input_tx, input_rx) = channel(1024);
        tokio::spawn(drive_disk_v2_writer(writer, input_rx));

        Ok((
            SenderAdapter::channel(input_tx),
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

#[derive(Debug)]
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
    buffered: VecDeque<T>,
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
            buffered: VecDeque::new(),
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
        let (writer, result) = match &self.state {
            WriterState::Writing => ready!(self.drive_write_operation(cx)),
            WriterState::Flushing => ready!(self.drive_flush_operation(cx)),
            s => unreachable!("writer state not expected: {:?}", s),
        };

        self.state = WriterState::Idle(writer);
        Poll::Ready(result)
    }

    fn drive_write_operation(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<(Writer<T>, Result<(), WriterError<T>>)> {
        match &self.state {
            WriterState::Writing => {
                let (writer, result) = ready!(self.write_future.poll(cx));
                // TODO: do we _need_ to reset the future here?
                Poll::Ready((writer, result.map(|_| ())))
            }
            s => unreachable!("writer state not expected: {:?}", s),
        }
    }

    fn drive_flush_operation(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<(Writer<T>, Result<(), WriterError<T>>)> {
        match &self.state {
            WriterState::Flushing => {
                let (writer, result) = ready!(self.flush_future.poll(cx));
                // TODO: do we _need_ to reset the future here?
                Poll::Ready((writer, result.map(|_| ())))
            }
            s => unreachable!("writer state not expected: {:?}", s),
        }
    }

    fn enqueue_write_operation(&mut self, record: T) {
        match mem::replace(&mut self.state, WriterState::Inconsistent) {
            WriterState::Idle(writer) => {
                self.write_future
                    .set(make_write_future(Some(writer), Some(record)));
                self.state = WriterState::Writing;
            }
            s => unreachable!("writer state not expected: {:?}", s),
        }
    }

    fn enqueue_flush_operation(&mut self) {
        match mem::replace(&mut self.state, WriterState::Inconsistent) {
            WriterState::Idle(writer) => {
                self.flush_future.set(make_flush_future(Some(writer)));
                self.state = WriterState::Flushing;
            }
            s => unreachable!("writer state not expected: {:?}", s),
        }
    }
}

impl<T> Sink<T> for WrappedWriter<T>
where
    T: Bufferable,
{
    type Error = ();

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Make sure any in-flight operation is completed first.
        if let Err(e) = ready!(self.as_mut().get_mut().try_ready(cx)) {
            error!("{}", e);
            return Poll::Ready(Err(()));
        }

        // Now make sure our internal buffer isn't too large before accepting another item.
        if self.buffered.len() < MAX_BUFFERED_ITEMS {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        // Make sure the caller isn't dodging `poll_ready`.
        if self.buffered.len() >= MAX_BUFFERED_ITEMS {
            error!(
                "`start_send` called without getting a successful result from `poll_ready` first"
            );
            return Err(());
        }

        self.buffered.push_back(item);
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Logic:
        //
        // loop:
        //   if !self.buffered.is_empty():
        //     - drive in-flight operation until ready
        //     - enqueue write operation
        //   else:
        //     - drive in-flight operation
        //     -- if operation was flush, return when fully driven
        //     - enqueue flush operation
        //
        // Any error at any point gets returned immediately.

        loop {
            if !self.buffered.is_empty() {
                // Drive any in-flight operation that we have going on.
                if !self.state.is_idle() {
                    if let Err(e) = ready!(self.drive_pending_operation(cx)) {
                        error!("{}", e);
                        return Poll::Ready(Err(()));
                    }
                }

                // Now we're idle, so enqueue another write operation.
                let record = self
                    .buffered
                    .pop_front()
                    .expect("buffered items should not be empty");
                self.enqueue_write_operation(record);
            } else {
                // We're all out of items to send, so now we need to flush the writer.  We're still
                // driving the last write operation, though, so we need to make sure that's cleared out.
                if let WriterState::Writing = &self.state {
                    if let Err(e) = ready!(self.drive_pending_operation(cx)) {
                        error!("{}", e);
                        return Poll::Ready(Err(()));
                    }
                }

                // We've got any in-flight write operation out of the way, now it's time to flush.
                if self.state.is_idle() {
                    self.enqueue_flush_operation();
                } else {
                    let result = ready!(self.drive_pending_operation(cx));
                    return Poll::Ready(result.map_err(|e| {
                        error!("{}", e);
                    }));
                }
            }
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Try to drive any remaining items into the writer, as well as one final flush.
        if let Err(e) = ready!(self.as_mut().poll_flush(cx)) {
            return Poll::Ready(Err(e));
        }

        // Now we can actually close the writer for real.  We leave the state as Inconsistent here
        // so that any future calls will panic and let us know there's some bad juju happening with
        // the caller, trying to use a previously-closed writer.
        match mem::replace(&mut self.state, WriterState::Inconsistent) {
            WriterState::Idle(mut writer) => {
                writer.close();
                Poll::Ready(Ok(()))
            }
            s => unreachable!("writer state not expected: {:?}", s),
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

async fn drive_disk_v2_writer<T>(mut writer: Writer<T>, mut input: Receiver<T>)
where
    T: Bufferable,
{
    // TODO: use a control message struct so callers can send both items to write and flush
    // requests, facilitating the ability to allow for `send_all` at the frontend
    while let Some(record) = input.recv().await {
        if let Err(e) = writer.write_record(record).await {
            error!("failed to write record to the buffer: {}", e);
        }

        if let Err(e) = writer.flush().await {
            error!("failed to flush the buffer: {}", e);
        }
    }
}

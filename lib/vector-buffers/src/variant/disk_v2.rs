use std::{
    error::Error,
    path::PathBuf,
    pin::Pin,
    task::{Context, Poll},
};

use async_trait::async_trait;
use futures::{ready, Stream};
use pin_project::pin_project;
use tokio::sync::mpsc::{channel, Receiver};
use tokio_util::sync::ReusableBoxFuture;

use crate::{
    buffer_usage_data::BufferUsageHandle,
    disk_v2::{Buffer, DiskBufferConfig, Reader, Writer},
    topology::{
        builder::IntoBuffer,
        channel::{ReceiverAdapter, SenderAdapter},
    },
    Acker, Bufferable,
};

pub struct DiskV2Buffer {
    id: String,
    data_dir: PathBuf,
    max_size: u64,
}

impl DiskV2Buffer {
    pub fn new(id: String, data_dir: PathBuf, max_size: u64) -> Self {
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
    fn provides_instrumentation(&self) -> bool {
        true
    }

    async fn into_buffer_parts(
        self: Box<Self>,
        usage_handle: BufferUsageHandle,
    ) -> Result<(SenderAdapter<T>, ReceiverAdapter<T>, Option<Acker>), Box<dyn Error + Send + Sync>>
    {
        usage_handle.set_buffer_limits(Some(self.max_size), None);

        // Create the actual buffer subcomponents.
        let buffer_path = self.data_dir.join("buffer").join("v2").join(self.id);
        let config = DiskBufferConfig::from_path(buffer_path)
            .max_buffer_size(self.max_size as u64)
            .build();
        let (writer, reader, acker) = Buffer::from_config(config, usage_handle).await?;

        let wrapped_reader = WrappedReader::new(reader);

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

    trace!("diskv2 writer task finished");
}

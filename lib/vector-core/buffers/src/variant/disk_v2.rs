use std::error::Error;
use std::path::PathBuf;

use async_trait::async_trait;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio_stream::wrappers::ReceiverStream;

use crate::buffer_usage_data::BufferUsageHandle;
use crate::disk_v2::{Buffer, DiskBufferConfig, Reader, Writer};
use crate::{
    topology::{builder::IntoBuffer, poll_sender::PollSender},
    Acker, Bufferable,
};

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
    ) -> Result<(PollSender<T>, ReceiverStream<T>, Option<Acker>), Box<dyn Error + Send + Sync>>
    {
        usage_handle.set_buffer_limits(Some(self.max_size), None);

        // Fixed-sized buffers for the reader/writer frontends.  These feed the actual reader/writer
        // tasks themselves, and we bound them for predictability, rather than simply relying on
        // everything slamming into the writer and limiting themselves.
        let (writer_tx, writer_rx) = channel(512);
        let (reader_tx, reader_rx) = channel(512);

        // Create the actual buffer subcomponents.
        let buffer_path = self.data_dir.join(self.id);
        let config = DiskBufferConfig::from_path(buffer_path)
            .max_buffer_size(self.max_size as u64)
            .build();
        let (writer, reader, acker) = Buffer::from_config(config).await?;

        spawn_disk_v2_reader(reader, reader_tx);
        spawn_disk_v2_writer(writer, writer_rx);

        Ok((
            PollSender::new(writer_tx),
            ReceiverStream::new(reader_rx),
            Some(acker),
        ))
    }
}

fn spawn_disk_v2_reader<T>(mut reader: Reader<T>, reader_tx: Sender<T>)
where
    T: Bufferable,
{
    tokio::spawn(async move {
        loop {
            match reader.next().await {
                Ok(None) => break,
                Ok(Some(event)) => {
                    if let Err(_) = reader_tx.send(event).await {
                        error!("disk buffer receiver unexpectedly closed!");
                        break;
                    }
                }
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
                    break;
                }
            }
        }
    });
}

fn spawn_disk_v2_writer<T>(mut writer: Writer<T>, mut writer_rx: Receiver<T>)
where
    T: Bufferable,
{
    tokio::spawn(async move {
        while let Some(event) = writer_rx.recv().await {
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
            if let Err(e) = writer.write_record(event).await {
                error!(
                    "disk buffer writer unexpectedly returned error during write: {}",
                    e
                );
                break;
            }

            if let Err(e) = writer.flush().await {
                error!(
                    "disk buffer writer unexpectedly returned error during flush: {}",
                    e
                );
                break;
            }
        }
    });
}

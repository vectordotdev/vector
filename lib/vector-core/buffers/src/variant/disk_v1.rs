use std::error::Error;
use std::path::PathBuf;

use async_trait::async_trait;
use futures::SinkExt;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::buffer_usage_data::BufferUsageHandle;
use crate::disk::leveldb_buffer::{Reader, Writer};
use crate::{
    disk::open,
    topology::{builder::IntoBuffer, poll_sender::PollSender},
    Acker, Bufferable,
};

pub struct DiskV1Buffer {
    id: String,
    data_dir: PathBuf,
    max_size: usize,
}

impl DiskV1Buffer {
    pub fn new(id: String, data_dir: PathBuf, max_size: usize) -> Self {
        DiskV1Buffer {
            id,
            data_dir,
            max_size,
        }
    }
}

#[async_trait]
impl<T> IntoBuffer<T> for DiskV1Buffer
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
        let (writer, reader, acker) = open(
            &self.data_dir,
            &self.id,
            self.max_size,
            usage_handle.clone(),
        )?;

        spawn_disk_v1_reader(reader, reader_tx);
        spawn_disk_v1_writer(writer, writer_rx);

        Ok((
            PollSender::new(writer_tx),
            ReceiverStream::new(reader_rx),
            Some(acker),
        ))
    }
}

fn spawn_disk_v1_reader<T>(mut reader: Reader<T>, reader_tx: Sender<T>)
where
    T: Bufferable,
{
    tokio::spawn(async move {
        while let Some(event) = reader.next().await {
            if let Err(_) = reader_tx.send(event).await {
                error!("disk buffer receiver unexpectedly closed!");
                break;
            }
        }
    });
}

fn spawn_disk_v1_writer<T>(mut writer: Writer<T>, mut writer_rx: Receiver<T>)
where
    T: Bufferable,
{
    tokio::spawn(async move {
        while let Some(event) = writer_rx.recv().await {
            if let Err(()) = writer.send(event).await {
                error!("disk buffer writer unexpectedly returned error during send");
                break;
            }
        }
    });
}

use std::error::Error;
use std::path::PathBuf;

use async_trait::async_trait;
use futures::SinkExt;
use tokio::sync::mpsc::{channel, Sender, Receiver};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::buffer_usage_data::BufferUsageHandle;
use crate::disk::leveldb_buffer::{Reader, Writer};
use crate::{
    topology::{builder::IntoBuffer, poll_sender::PollSender},
    Bufferable, Acker, disk::open,
};

pub struct DiskV2Buffer {
	id: String,
	data_dir: PathBuf,
	max_size: usize,
}

impl DiskV2Buffer {
    pub fn new(id: String, data_dir: PathBuf, max_size: usize) -> Self {
        Self { id, data_dir, max_size }
    }
}

#[async_trait]
impl<T> IntoBuffer<T> for DiskV2Buffer
where
    T: Bufferable + Clone,
{
	async fn into_buffer_parts(self: Box<Self>, usage_handle: &BufferUsageHandle) -> Result<(PollSender<T>, ReceiverStream<T>, Option<Acker>), Box<dyn Error + Send + Sync>> {
		usage_handle.set_buffer_limits(Some(self.max_size), None);
	
		// Fixed-sized buffers for the reader/writer frontends.  These feed the actual reader/writer
		// tasks themselves, and we bound them for predictability, rather than simply relying on
		// everything slamming into the writer and limiting themselves.
        let (writer_tx, writer_rx) = channel(512);
		let (reader_tx, reader_rx) = channel(512);

		// Create the actual buffer subcomponents.
		let (writer, reader, acker) = open(&self.data_dir, &self.id, self.max_size, usage_handle)?; 

		let (reader, writer, acker) = Buffer::
	
		spawn_disk_v2_reader(reader, reader_tx);
		spawn_disk_v2_writer(writer, writer_rx);

        Ok((PollSender::new(writer_tx), ReceiverStream::new(reader_rx), Some(acker)))
    }
}

fn spawn_disk_v1_reader<T>(mut reader: Reader<T>, reader_tx: Sender<T>)
where
	T: Bufferable,
{
	tokio::spawn(async move {
		while let Some(event) = reader.next().await {
			if let Err(e) = reader_tx.send(event).await {
				error!("disk buffer receiver unexpectedly closed!");
				break
			}
		}
	});
}

fn spawn_disk_v1_writer<T>(writer: Writer<T>, writer_rx: Receiver<T>)
where
	T: Bufferable,
{
	tokio::spawn(async move {
		while let Some(event) = writer_rx.recv().await {
			if let Err(e) = writer.send(event).await {
				error!("disk buffer writer unexpectedly returned error during send");
				break
			}
		}
	});
}

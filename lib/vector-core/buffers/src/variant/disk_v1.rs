use std::{error::Error, path::PathBuf};

use async_trait::async_trait;

use crate::{
    buffer_usage_data::BufferUsageHandle,
    disk::open,
    topology::{
        builder::IntoBuffer,
        channel::{ReceiverAdapter, SenderAdapter},
    },
    Acker, Bufferable,
};

pub struct DiskV1Buffer {
    id: String,
    data_dir: PathBuf,
    max_size: u64,
}

impl DiskV1Buffer {
    pub fn new(id: String, data_dir: PathBuf, max_size: u64) -> Self {
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
        let (writer, reader, acker) = open(&self.data_dir, &self.id, self.max_size, usage_handle)?;

        Ok((
            SenderAdapter::opaque(writer),
            ReceiverAdapter::opaque(reader),
            Some(acker),
        ))
    }
}

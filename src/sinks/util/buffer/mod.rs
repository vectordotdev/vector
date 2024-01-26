use std::io::Write;

use bytes::{BufMut, BytesMut};
use flate2::write::{GzEncoder, ZlibEncoder};

use super::{
    batch::{err_event_too_large, Batch, BatchSize, PushResult},
    snappy::SnappyEncoder,
    zstd::ZstdEncoder,
};

pub mod compression;
pub mod json;
pub mod metrics;
pub mod partition;
pub mod vec;

pub use compression::Compression;
pub use partition::{Partition, PartitionBuffer, PartitionInnerBuffer};

#[derive(Debug)]
pub struct Buffer {
    inner: Option<InnerBuffer>,
    num_items: usize,
    num_bytes: usize,
    settings: BatchSize<Self>,
    compression: Compression,
}

#[derive(Debug)]
pub enum InnerBuffer {
    Plain(bytes::buf::Writer<BytesMut>),
    Gzip(GzEncoder<bytes::buf::Writer<BytesMut>>),
    Zlib(ZlibEncoder<bytes::buf::Writer<BytesMut>>),
    Zstd(ZstdEncoder<bytes::buf::Writer<BytesMut>>),
    Snappy(SnappyEncoder<bytes::buf::Writer<BytesMut>>),
}

impl Buffer {
    pub const fn new(settings: BatchSize<Self>, compression: Compression) -> Self {
        Self {
            inner: None,
            num_items: 0,
            num_bytes: 0,
            settings,
            compression,
        }
    }

    fn buffer(&mut self) -> &mut InnerBuffer {
        let bytes = self.settings.bytes;
        let compression = self.compression;
        self.inner.get_or_insert_with(|| {
            let writer = BytesMut::with_capacity(bytes).writer();
            match compression {
                Compression::None => InnerBuffer::Plain(writer),
                Compression::Gzip(level) => {
                    InnerBuffer::Gzip(GzEncoder::new(writer, level.as_flate2()))
                }
                Compression::Zlib(level) => {
                    InnerBuffer::Zlib(ZlibEncoder::new(writer, level.as_flate2()))
                }
                Compression::Zstd(level) => InnerBuffer::Zstd(
                    ZstdEncoder::new(writer, level.into())
                        .expect("Zstd encoder should not fail on init."),
                ),
                Compression::Snappy => InnerBuffer::Snappy(SnappyEncoder::new(writer)),
            }
        })
    }

    pub fn push(&mut self, input: &[u8]) {
        self.num_items += 1;
        match self.buffer() {
            InnerBuffer::Plain(inner) => {
                inner.write_all(input).unwrap();
            }
            InnerBuffer::Gzip(inner) => {
                inner.write_all(input).unwrap();
            }
            InnerBuffer::Zlib(inner) => {
                inner.write_all(input).unwrap();
            }
            InnerBuffer::Zstd(inner) => {
                inner.write_all(input).unwrap();
            }
            InnerBuffer::Snappy(inner) => inner.write_all(input).unwrap(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .as_ref()
            .map(|inner| match inner {
                InnerBuffer::Plain(inner) => inner.get_ref().is_empty(),
                InnerBuffer::Gzip(inner) => inner.get_ref().get_ref().is_empty(),
                InnerBuffer::Zlib(inner) => inner.get_ref().get_ref().is_empty(),
                InnerBuffer::Zstd(inner) => inner.get_ref().get_ref().is_empty(),
                InnerBuffer::Snappy(inner) => inner.is_empty(),
            })
            .unwrap_or(true)
    }
}

impl Batch for Buffer {
    type Input = BytesMut;
    type Output = BytesMut;

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        // The compressed encoders don't flush bytes immediately, so we
        // can't track compressed sizes. Keep a running count of the
        // number of bytes written instead.
        let new_bytes = self.num_bytes + item.len();
        if self.is_empty() && item.len() > self.settings.bytes {
            err_event_too_large(item.len(), self.settings.bytes)
        } else if self.num_items >= self.settings.events || new_bytes > self.settings.bytes {
            PushResult::Overflow(item)
        } else {
            self.push(&item);
            self.num_bytes = new_bytes;
            PushResult::Ok(
                self.num_items >= self.settings.events || new_bytes >= self.settings.bytes,
            )
        }
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn fresh(&self) -> Self {
        Self::new(self.settings, self.compression)
    }

    fn finish(self) -> Self::Output {
        match self.inner {
            Some(InnerBuffer::Plain(inner)) => inner.into_inner(),
            Some(InnerBuffer::Gzip(inner)) => inner
                .finish()
                .expect("This can't fail because the inner writer is a Vec")
                .into_inner(),
            Some(InnerBuffer::Zlib(inner)) => inner
                .finish()
                .expect("This can't fail because the inner writer is a Vec")
                .into_inner(),
            Some(InnerBuffer::Zstd(inner)) => inner
                .finish()
                .expect("This can't fail because the inner writer is a Vec")
                .into_inner(),
            Some(InnerBuffer::Snappy(inner)) => inner
                .finish()
                .expect("This can't fail because the inner writer is a Vec")
                .into_inner(),
            None => BytesMut::new(),
        }
    }

    fn num_items(&self) -> usize {
        self.num_items
    }
}

#[cfg(test)]
mod test {
    use std::{
        io::Read,
        sync::{Arc, Mutex},
    };

    use bytes::{Buf, BytesMut};
    use futures::{future, stream, SinkExt, StreamExt};
    use tokio::time::Duration;
    use vector_lib::json_size::JsonSize;

    use super::{Buffer, Compression};
    use crate::sinks::util::{BatchSettings, BatchSink, EncodedEvent};

    #[tokio::test]
    async fn gzip() {
        use flate2::read::MultiGzDecoder;

        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 100_000;
        batch_settings.size.events = 1_000;
        batch_settings.timeout = Duration::from_secs(0);

        let buffered = BatchSink::new(
            svc,
            Buffer::new(batch_settings.size, Compression::gzip_default()),
            batch_settings.timeout,
        );

        let input = std::iter::repeat(BytesMut::from(
            "It's going down, I'm yelling timber, You better move, you better dance",
        ))
        .take(100_000);

        buffered
            .sink_map_err(drop)
            .send_all(
                &mut stream::iter(input)
                    .map(|item| Ok(EncodedEvent::new(item, 0, JsonSize::zero()))),
            )
            .await
            .unwrap();

        let output = Arc::try_unwrap(sent_requests)
            .unwrap()
            .into_inner()
            .unwrap();

        assert!(output.len() > 1);
        assert!(output.iter().map(|o| o.len()).sum::<usize>() < 80_000);

        let decompressed = output.into_iter().flat_map(|batch| {
            let mut decompressed = vec![];
            MultiGzDecoder::new(batch.reader())
                .read_to_end(&mut decompressed)
                .unwrap();
            decompressed
        });

        assert!(decompressed.eq(std::iter::repeat(
            b"It's going down, I'm yelling timber, You better move, you better dance".to_vec()
        )
        .take(100_000)
        .flatten()));
    }
}

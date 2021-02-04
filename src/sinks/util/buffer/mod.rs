use super::batch::{
    err_event_too_large, Batch, BatchConfig, BatchError, BatchSettings, BatchSize, PushResult,
};
use flate2::write::GzEncoder;
use std::io::Write;

pub mod compression;
pub mod json;
#[cfg(feature = "sinks-loki")]
pub mod loki;
pub mod metrics;
pub mod partition;
pub mod vec;

pub use compression::{Compression, GZIP_FAST};
pub use partition::{Partition, PartitionBuffer, PartitionInnerBuffer};

#[derive(Debug)]
pub struct Buffer {
    inner: InnerBuffer,
    num_items: usize,
    num_bytes: usize,
    settings: BatchSize<Self>,
    compression: Compression,
}

#[derive(Debug)]
pub enum InnerBuffer {
    Plain(Vec<u8>),
    Gzip(GzEncoder<Vec<u8>>),
}

impl Buffer {
    pub fn new(settings: BatchSize<Self>, compression: Compression) -> Self {
        let buffer = Vec::with_capacity(settings.bytes);
        let inner = match compression {
            Compression::None => InnerBuffer::Plain(buffer),
            Compression::Gzip(level) => {
                let level = level.unwrap_or(GZIP_FAST);
                InnerBuffer::Gzip(GzEncoder::new(
                    buffer,
                    flate2::Compression::new(level as u32),
                ))
            }
        };
        Self {
            inner,
            num_items: 0,
            num_bytes: 0,
            settings,
            compression,
        }
    }

    pub fn push(&mut self, input: &[u8]) {
        self.num_items += 1;
        match &mut self.inner {
            InnerBuffer::Plain(inner) => {
                inner.extend_from_slice(input);
            }
            InnerBuffer::Gzip(inner) => {
                inner.write_all(input).unwrap();
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        match &self.inner {
            InnerBuffer::Plain(inner) => inner.is_empty(),
            InnerBuffer::Gzip(inner) => inner.get_ref().is_empty(),
        }
    }
}

impl Batch for Buffer {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn get_settings_defaults(
        config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(config
            .use_size_as_bytes()?
            .get_settings_or_default(defaults))
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        // The compressed encoders don't flush bytes immediately, so we
        // can't track compressed sizes. Keep a running count of the
        // number of bytes written instead.
        let new_bytes = self.num_bytes + item.len();
        if self.is_empty() && item.len() > self.settings.bytes {
            err_event_too_large(item.len())
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
            InnerBuffer::Plain(inner) => inner,
            InnerBuffer::Gzip(inner) => inner
                .finish()
                .expect("This can't fail because the inner writer is a Vec"),
        }
    }

    fn num_items(&self) -> usize {
        self.num_items
    }
}

#[cfg(test)]
mod test {
    use super::{Buffer, Compression};
    use crate::{
        buffers::Acker,
        sinks::util::{BatchSettings, BatchSink},
    };
    use futures::{future, stream, SinkExt, StreamExt};
    use std::{
        io::Read,
        sync::{Arc, Mutex},
    };
    use tokio::time::Duration;

    #[tokio::test]
    async fn gzip() {
        use flate2::read::GzDecoder;

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });
        let batch_size = BatchSettings::default().bytes(100_000).events(1_000).size;
        let timeout = Duration::from_secs(0);

        let buffered = BatchSink::new(
            svc,
            Buffer::new(batch_size, Compression::gzip_default()),
            timeout,
            acker,
        );

        let input = std::iter::repeat(
            b"It's going down, I'm yelling timber, You better move, you better dance".to_vec(),
        )
        .take(100_000);

        let _ = buffered
            .sink_map_err(drop)
            .send_all(&mut stream::iter(input).map(Ok))
            .await
            .unwrap();

        let output = Arc::try_unwrap(sent_requests)
            .unwrap()
            .into_inner()
            .unwrap();

        let output = output.into_iter().collect::<Vec<Vec<u8>>>();

        assert!(output.len() > 1);
        assert!(dbg!(output.iter().map(|o| o.len()).sum::<usize>()) < 80_000);

        let decompressed = output.into_iter().flat_map(|batch| {
            let mut decompressed = vec![];
            GzDecoder::new(batch.as_slice())
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

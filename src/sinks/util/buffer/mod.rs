use super::batch::{err_event_too_large, Batch, BatchSize, PushResult};
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use std::io::Write;

pub mod json;
pub mod metrics;
pub mod partition;
pub mod vec;
pub mod vec2;

pub use partition::{Partition, PartitionBuffer, PartitionInnerBuffer};

#[derive(Serialize, Deserialize, Debug, Derivative, Copy, Clone, Eq, PartialEq)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    #[derivative(Default)]
    None,
    Gzip,
}

impl Compression {
    pub fn default_gzip() -> Compression {
        Compression::Gzip
    }

    pub fn content_encoding(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Gzip => Some("gzip"),
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::None => "log",
            Self::Gzip => "log.gz",
        }
    }
}

#[cfg(feature = "rusoto_core")]
impl From<Compression> for rusoto_core::encoding::ContentEncoding {
    fn from(compression: Compression) -> Self {
        match compression {
            Compression::None => rusoto_core::encoding::ContentEncoding::Identity,
            // 6 is default, add Gzip level support to vector in future
            Compression::Gzip => rusoto_core::encoding::ContentEncoding::Gzip(None, 6),
        }
    }
}

#[derive(Debug)]
pub struct Buffer {
    inner: InnerBuffer,
    num_items: usize,
    num_bytes: usize,
    settings: BatchSize,
    compression: Compression,
}

#[derive(Debug)]
pub enum InnerBuffer {
    Plain(Vec<u8>),
    Gzip(GzEncoder<Vec<u8>>),
}

impl Buffer {
    pub fn new(settings: BatchSize, compression: Compression) -> Self {
        let buffer = Vec::with_capacity(settings.bytes);
        let inner = match compression {
            Compression::None => InnerBuffer::Plain(buffer),
            Compression::Gzip => {
                InnerBuffer::Gzip(GzEncoder::new(buffer, flate2::Compression::fast()))
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

    // This is not guaranteed to be completely accurate as the gzip library does
    // some internal buffering.
    pub fn size(&self) -> usize {
        match &self.inner {
            InnerBuffer::Plain(inner) => inner.len(),
            InnerBuffer::Gzip(inner) => inner.get_ref().len(),
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
    use crate::buffers::Acker;
    use crate::sinks::util::{BatchSink, BatchSize};
    use crate::test_util::runtime;
    use futures01::{future, Future, Sink};
    use std::io::Read;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio01_test::clock::MockClock;

    #[test]
    fn gzip() {
        use flate2::read::GzDecoder;

        let rt = runtime();
        let mut clock = MockClock::new();

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let batch_size = BatchSize {
            bytes: 100_000,
            events: 1_000,
        };
        let timeout = Duration::from_secs(0);

        let buffered = BatchSink::with_executor(
            svc,
            Buffer::new(batch_size, Compression::Gzip),
            timeout,
            acker,
            rt.executor(),
        );

        let input = std::iter::repeat(
            b"It's going down, I'm yelling timber, You better move, you better dance".to_vec(),
        )
        .take(100_000);

        let (sink, _) = clock.enter(|_| {
            buffered
                .sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(input))
                .wait()
                .unwrap()
        });

        drop(sink);

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

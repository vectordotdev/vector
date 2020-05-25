use super::batch::Batch;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use std::io::Write;

pub mod json;
pub mod metrics;
pub mod partition;

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

#[derive(Debug)]
pub struct Buffer {
    inner: InnerBuffer,
    num_items: usize,
}

#[derive(Debug)]
pub enum InnerBuffer {
    Plain(Vec<u8>),
    Gzip(GzEncoder<Vec<u8>>),
}

impl Buffer {
    pub fn new(compression: Compression) -> Self {
        let inner = match compression {
            Compression::None => InnerBuffer::Plain(Vec::new()),
            Compression::Gzip => {
                InnerBuffer::Gzip(GzEncoder::new(Vec::new(), flate2::Compression::fast()))
            }
        };
        Self {
            inner,
            num_items: 0,
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

    fn len(&self) -> usize {
        self.size()
    }

    fn push(&mut self, item: Self::Input) {
        self.push(&item)
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn fresh(&self) -> Self {
        let inner = match &self.inner {
            InnerBuffer::Plain(_) => InnerBuffer::Plain(Vec::new()),
            InnerBuffer::Gzip(_) => {
                InnerBuffer::Gzip(GzEncoder::new(Vec::new(), flate2::Compression::default()))
            }
        };
        Self {
            inner,
            num_items: 0,
        }
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
    use crate::sinks::util::{BatchSettings, BatchSink};
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
        let buffered = BatchSink::with_executor(
            svc,
            Buffer::new(Compression::Gzip),
            BatchSettings {
                timeout: Duration::from_secs(0),
                size: 1000,
            },
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
        assert!(dbg!(output.iter().map(|o| o.len()).sum::<usize>()) < 51_000);

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

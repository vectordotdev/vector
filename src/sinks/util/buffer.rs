use super::{batch::Batch, partition::Partition};
use bytes::Bytes;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    None,
    Gzip,
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
    pub fn new(gzip: bool) -> Self {
        let inner = if gzip {
            InnerBuffer::Gzip(GzEncoder::new(Vec::new(), flate2::Compression::default()))
        } else {
            InnerBuffer::Plain(Vec::new())
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

#[derive(Debug)]
pub struct PartitionBuffer<T> {
    inner: T,
    key: Option<Bytes>,
}

#[derive(Debug, Clone)]
pub struct PartitionInnerBuffer<T> {
    pub(self) inner: T,
    key: Bytes,
}

impl<T> PartitionBuffer<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, key: None }
    }
}

impl<T> Batch for PartitionBuffer<T>
where
    T: Batch,
{
    type Input = PartitionInnerBuffer<T::Input>;
    type Output = PartitionInnerBuffer<T::Output>;

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn push(&mut self, item: Self::Input) {
        let partition = item.partition();
        self.key = Some(partition);
        self.inner.push(item.inner)
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn fresh(&self) -> Self {
        Self {
            inner: self.inner.fresh(),
            key: None,
        }
    }

    fn finish(mut self) -> Self::Output {
        let key = self.key.take().unwrap();
        let inner = self.inner.finish();
        PartitionInnerBuffer { inner, key }
    }

    fn num_items(&self) -> usize {
        self.inner.num_items()
    }
}

impl<T> PartitionInnerBuffer<T> {
    pub fn new(inner: T, key: Bytes) -> Self {
        Self { inner, key }
    }

    pub fn into_parts(self) -> (T, Bytes) {
        (self.inner, self.key)
    }
}

impl<T> Partition for PartitionInnerBuffer<T> {
    fn partition(&self) -> Bytes {
        self.key.clone()
    }
}

#[cfg(test)]
mod test {
    use super::Buffer;
    use crate::sinks::util::batch::{Batch, BatchSink};
    use futures::{Future, Sink};
    use std::io::Read;

    #[test]
    fn gzip() {
        use flate2::read::GzDecoder;

        let buffered = BatchSink::new(vec![], Buffer::new(true), 1000);

        let input = std::iter::repeat(
            b"It's going down, I'm yelling timber, You better move, you better dance".to_vec(),
        )
        .take(100_000);

        let (buffered, _) = buffered
            .send_all(futures::stream::iter_ok(input))
            .wait()
            .unwrap();

        let output = buffered
            .into_inner()
            .into_iter()
            .map(|buf| buf.finish())
            .collect::<Vec<Vec<u8>>>();

        assert!(output.len() > 1);
        assert!(output.iter().map(|o| o.len()).sum::<usize>() < 50_000);

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

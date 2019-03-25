use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::mem;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    None,
    Gzip,
}

pub enum Buffer {
    Plain(Vec<u8>),
    Gzip(GzEncoder<Vec<u8>>),
}

impl Buffer {
    pub fn new(gzip: bool) -> Self {
        if gzip {
            Buffer::Gzip(GzEncoder::new(Vec::new(), flate2::Compression::default()))
        } else {
            Buffer::Plain(Vec::new())
        }
    }

    pub fn get_and_reset(&mut self) -> Vec<u8> {
        match self {
            Buffer::Plain(ref mut inner) => mem::replace(inner, Vec::new()),
            Buffer::Gzip(ref mut inner) => {
                let inner = mem::replace(
                    inner,
                    GzEncoder::new(Vec::new(), flate2::Compression::default()),
                );
                inner
                    .finish()
                    .expect("This can't fail because the inner writer is a Vec")
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) {
        match self {
            Buffer::Plain(inner) => {
                inner.extend_from_slice(input);
            }
            Buffer::Gzip(inner) => {
                inner.write_all(input).unwrap();
            }
        }
    }

    // This is not guaranteed to be completely accurate as the gzip library does
    // some internal buffering.
    pub fn size(&self) -> usize {
        match self {
            Buffer::Plain(inner) => inner.len(),
            Buffer::Gzip(inner) => inner.get_ref().len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Buffer::Plain(inner) => inner.is_empty(),
            Buffer::Gzip(inner) => inner.get_ref().is_empty(),
        }
    }
}

impl super::batch::Batch for Buffer {
    type Item = Vec<u8>;

    fn len(&self) -> usize {
        self.size()
    }

    fn push(&mut self, item: Self::Item) {
        self.push(&item)
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn fresh(&self) -> Self {
        match self {
            Buffer::Plain(_) => Buffer::Plain(Vec::new()),
            Buffer::Gzip(_) => {
                Buffer::Gzip(GzEncoder::new(Vec::new(), flate2::Compression::default()))
            }
        }
    }
}

impl From<Buffer> for Vec<u8> {
    fn from(buffer: Buffer) -> Self {
        match buffer {
            Buffer::Plain(inner) => inner,
            Buffer::Gzip(inner) => inner
                .finish()
                .expect("This can't fail because the inner writer is a Vec"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::Buffer;
    use crate::sinks::util::batch::BatchSink;
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
            .map(|buf| buf.into())
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

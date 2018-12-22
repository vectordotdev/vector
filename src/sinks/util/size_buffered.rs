use futures::{try_ready, Async, AsyncSink, Sink};
use std::mem;
use flate2::write::GzEncoder;
use std::io::Write;

#[cfg(test)]
mod test {
    use super::SizeBuffered;
    use futures::{Future, Sink};
    use std::io::Read;

    #[test]
    fn size_buffered_buffers_messages_until_limit() {
        let buffered = SizeBuffered::new(vec![], 10, false);

        let input = (0..22).map(|i| vec![i]).collect::<Vec<_>>();
        let (buffered, _) = buffered
            .send_all(futures::stream::iter_ok(input))
            .wait()
            .unwrap();

        let output = buffered.into_inner();
        assert_eq!(
            output,
            vec![
                vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
                vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 19],
                vec![20, 21]
            ]
        );
    }

    #[test]
    fn size_buffered_doesnt_buffer_if_its_flushed() {
        let buffered = SizeBuffered::new(vec![], 10, false);

        let buffered = buffered.send(vec![0]).wait().unwrap();
        let buffered = buffered.send(vec![1]).wait().unwrap();

        let output = buffered.into_inner();
        assert_eq!(output, vec![vec![0], vec![1],]);
    }

    #[test]
    fn size_buffered_allows_the_final_item_to_exceed_the_buffer_size() {
        let buffered = SizeBuffered::new(vec![], 10, false);

        let input = vec![
            vec![0, 1, 2],
            vec![3, 4, 5],
            vec![6, 7, 8],
            vec![9, 10, 11],
            vec![12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23],
            vec![24],
        ];
        let (buffered, _) = buffered
            .send_all(futures::stream::iter_ok(input))
            .wait()
            .unwrap();

        let output = buffered.into_inner();
        assert_eq!(
            output,
            vec![
                vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                vec![12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23],
                vec![24],
            ]
        );
    }

    #[test]
    fn gzip() {
        use flate2::read::GzDecoder;

        let buffered = SizeBuffered::new(vec![], 1000, true);

        let input = std::iter::repeat(b"It's going down, I'm yelling timber, You better move, you better dance".to_vec()).take(100_000);

        let (buffered, _) = buffered
            .send_all(futures::stream::iter_ok(input))
            .wait()
            .unwrap();

        let output = buffered.into_inner();

        assert!(output.len() > 1);
        assert!(output.iter().map(|o| o.len()).sum::<usize>() < 50_000);

        let decompressed = output.into_iter().flat_map(|batch| {
            let mut decompressed = vec![];
            GzDecoder::new(batch.as_slice()).read_to_end(&mut decompressed).unwrap();
            decompressed
        });

        assert!(
            decompressed.eq(std::iter::repeat(b"It's going down, I'm yelling timber, You better move, you better dance".to_vec()).take(100_000).flatten())
        );
    }
}

pub struct SizeBuffered<S: Sink<SinkItem = Vec<u8>>> {
    inner: S,
    buffer: Buffer,
    buffer_limit: usize,
}

impl<S: Sink<SinkItem = Vec<u8>>> SizeBuffered<S> {
    pub fn new(inner: S, limit: usize, gzip: bool) -> Self {
        Self {
            inner,
            buffer: Buffer::new(gzip),
            buffer_limit: limit,
        }
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: Sink<SinkItem = Vec<u8>>> Sink for SizeBuffered<S> {
    type SinkItem = Vec<u8>;
    type SinkError = S::SinkError;

    fn start_send(
        &mut self,
        mut item: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        if self.buffer.size() >= self.buffer_limit {
            self.poll_complete()?;

            if self.buffer.size() >= self.buffer_limit {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        self.buffer.push(&mut item);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        loop {
            try_ready!(self.inner.poll_complete());

            if self.buffer.is_empty() {
                return Ok(Async::Ready(()));
            } else {
                let buffer = self.buffer.get_and_reset();

                match self.inner.start_send(buffer)? {
                    AsyncSink::Ready => {}
                    AsyncSink::NotReady(_item) => {
                        unreachable!("Will only get here if inner.poll_complete() returned Ready")
                    }
                }
            }
        }
    }
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
                let inner = mem::replace(inner, GzEncoder::new(Vec::new(), flate2::Compression::default()));
                inner.finish().expect("This can't fail because the inner writer is a Vec")
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
                // inner.flush().unwrap();
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

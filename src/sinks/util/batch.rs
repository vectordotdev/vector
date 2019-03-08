use futures::{try_ready, Async, AsyncSink, Poll, Sink, StartSend};

pub trait Batch {
    type Item;
    fn len(&self) -> usize;
    fn push(&mut self, item: Self::Item);
    fn is_empty(&self) -> bool;
    fn fresh(&self) -> Self;
}

impl<T> Batch for Vec<T> {
    type Item = T;

    fn len(&self) -> usize {
        self.len()
    }

    fn push(&mut self, item: Self::Item) {
        self.push(item)
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn fresh(&self) -> Self {
        Self::new()
    }
}

pub struct BatchSink<B, S> {
    batch: B,
    inner: S,
    max_size: usize,
    min_size: usize,
    closing: bool,
}

impl<B, S> BatchSink<B, S>
where
    B: Batch,
    S: Sink<SinkItem = B>,
{
    pub fn new(inner: S, batch: B, max_size: usize) -> Self {
        let min_size = 0; // TODO: more patterns

        assert!(max_size >= min_size);
        BatchSink {
            batch,
            inner,
            max_size,
            min_size,
            closing: false,
        }
    }

    pub fn into_inner(self) -> S {
        self.inner
    }

    fn should_send(&self) -> bool {
        self.closing || self.batch.len() >= self.min_size
    }

    fn poll_send(&mut self) -> Poll<(), S::SinkError> {
        let fresh = self.batch.fresh();
        let batch = std::mem::replace(&mut self.batch, fresh);
        if let AsyncSink::NotReady(batch) = self.inner.start_send(batch)? {
            self.batch = batch;
            Ok(Async::NotReady)
        } else {
            Ok(Async::Ready(()))
        }
    }
}

impl<B, E, S> Sink for BatchSink<B, S>
where
    B: Batch,
    S: Sink<SinkItem = B, SinkError = E>,
{
    type SinkItem = B::Item;
    type SinkError = E;

    // When used with Stream::forward, a successful call to start_send will always be followed
    // immediately by another call to start_send or a call to poll_complete. This means that
    // start_send only needs to concern itself with the case where we've hit our batch's capacity
    // and need to push it down to the inner sink. The other case, when our batch is not full but
    // we want to push it to the inner sink anyway, can be detected and handled by poll_complete.
    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.batch.len() >= self.max_size {
            self.poll_complete()?;

            if self.batch.len() > self.max_size {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        self.batch.push(item.into());

        Ok(AsyncSink::Ready)
    }

    // When used with Stream::forward, poll_complete will be called in a few different
    // circumstances:
    //
    //   1. internally via start_send when our batch is full
    //   2. externally from Forward when the stream returns NotReady
    //   3. internally via close from Forward when the stream returns Ready(None)
    //
    // In (1), we always want to attempt to push the current batch down into the inner sink.
    //
    // For (2), our behavior depends on configuration. If we have a minimum batch size that
    // hasn't yet been met, we'll want to wait for additional items before pushing the current
    // batch down. If there is no minimum or we've already met it, we will try to push the current
    // batch down. If the inner sink is not ready, we'll keep that batch and continue appending
    // to it.
    //
    // Finally, for (3), our behavior is essentially the same as for (2), except that we'll try to
    // send our existing batch whether or not it has met the minimum batch size.
    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        loop {
            if self.batch.is_empty() {
                // We have no data to send, so forward to inner
                if self.closing {
                    return self.inner.close();
                } else {
                    return self.inner.poll_complete();
                }
            } else {
                // We have data to send, so check if we should send it and either attempt the send
                // or return that we're not ready to send. If we send and it works, loop to poll or
                // close inner instead of prematurely returning Ready
                if self.should_send() {
                    try_ready!(self.poll_send());
                } else {
                    return Ok(Async::NotReady);
                }
            }
        }
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        self.closing = true;
        self.poll_complete()
    }
}

#[cfg(test)]
mod test {
    use super::BatchSink;
    use crate::sinks::util::Buffer;
    use futures::{Future, Sink};

    #[test]
    fn batch_sink_buffers_messages_until_limit() {
        let buffered = BatchSink::new(vec![], Vec::new(), 10);

        let (buffered, _) = buffered
            .send_all(futures::stream::iter_ok(0..22))
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
    fn batch_sink_doesnt_buffer_if_its_flushed() {
        let buffered = BatchSink::new(vec![], Vec::new(), 10);

        let buffered = buffered.send(0).wait().unwrap();
        let buffered = buffered.send(1).wait().unwrap();

        let output = buffered.into_inner();
        assert_eq!(output, vec![vec![0], vec![1],]);
    }

    #[test]
    fn batch_sink_allows_the_final_item_to_exceed_the_buffer_size() {
        let buffered = BatchSink::new(vec![], Buffer::new(false), 10);

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

        let output = buffered
            .into_inner()
            .into_iter()
            .map(|buf| buf.into())
            .collect::<Vec<Vec<u8>>>();

        assert_eq!(
            output,
            vec![
                vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                vec![12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23],
                vec![24],
            ]
        );
    }

}

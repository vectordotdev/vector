use futures::{try_ready, Async, AsyncSink, Poll, Sink, StartSend};

pub trait SinkExt<Input, B>: Sink<SinkItem = B> + Sized
where
    B: Batch<Item = Input>,
{
    fn batched(self, limit: usize) -> BatchSink<Input, B, Self> {
        BatchSink::new(self, limit)
    }
}

impl<Input, B, S> SinkExt<Input, B> for S
where
    B: Batch<Item = Input>,
    S: Sink<SinkItem = B> + Sized,
{
}

pub trait Batch: Default {
    type Item;
    fn len(&self) -> usize;
    fn push(&mut self, item: Self::Item);
    fn is_empty(&self) -> bool;
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
}

pub struct BatchSink<Item, B: Batch, S: Sink> {
    batch: B,
    inner: S,
    max_size: usize,
    min_size: usize,
    closing: bool,
    _pd: std::marker::PhantomData<Item>,
}

impl<Input, B, S> BatchSink<Input, B, S>
where
    B: Batch<Item = Input>,
    S: Sink<SinkItem = B>,
{
    pub fn new(inner: S, max_size: usize) -> Self {
        BatchSink {
            batch: Default::default(),
            inner,
            max_size,
            min_size: max_size, // TODO: more patterns
            closing: false,
            _pd: std::marker::PhantomData,
        }
    }

    fn should_send(&self) -> bool {
        self.closing || self.batch.len() > self.min_size || self.batch.len() >= self.max_size
    }

    fn try_send(&mut self) -> Poll<(), S::SinkError> {
        let batch = std::mem::replace(&mut self.batch, Default::default());
        if let AsyncSink::NotReady(batch) = self.inner.start_send(batch)? {
            self.batch = batch;
            Ok(Async::NotReady)
        } else {
            Ok(Async::Ready(()))
        }
    }
}

impl<Input, B, Error, S> Sink for BatchSink<Input, B, S>
where
    B: Batch<Item = Input>,
    S: Sink<SinkItem = B, SinkError = Error>,
{
    type SinkItem = Input;
    type SinkError = Error;

    // When used with Stream::forward, a successful call to start_send will always be followed
    // immediately by another call to start_send or a call to poll_complete. This means that
    // start_send only needs to concern itself with the case where we've hit our batch's capacity
    // and need to push it down to the inner sink. The other case, when our batch is not full but
    // we want to push it to the inner sink anyway, can be detected and handled by poll_complete.
    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.batch.len() > self.max_size {
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
                    try_ready!(self.try_send());
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

use bytes::{Bytes, BytesMut};
use futures::{Async, Poll, Stream};
use regex::bytes::Regex;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;
use tokio::timer::DelayQueue;

pub(super) struct LineAgg<T> {
    inner: T,
    marker: Regex,
    timeout: u64,
    buffers: HashMap<String, BytesMut>,
    draining: Option<Vec<(Bytes, String)>>,
    timeouts: DelayQueue<String>,
    expired: VecDeque<String>,
}

impl<T> LineAgg<T> {
    pub(super) fn new(inner: T, marker: Regex, timeout: u64) -> Self {
        Self {
            inner,
            marker,
            timeout,
            draining: None,
            buffers: HashMap::new(),
            timeouts: DelayQueue::new(),
            expired: VecDeque::new(),
        }
    }
}

impl<T: Stream<Item = (Bytes, String), Error = ()>> Stream for LineAgg<T> {
    type Item = (Bytes, String);
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            if let Some(to_drain) = &mut self.draining {
                if let Some((data, key)) = to_drain.pop() {
                    return Ok(Async::Ready(Some((data, key))));
                } else {
                    return Ok(Async::Ready(None));
                }
            }

            // check for keys that have hit their timeout
            while let Ok(Async::Ready(Some(expired_key))) = self.timeouts.poll() {
                self.expired.push_back(expired_key.into_inner());
            }

            match self.inner.poll() {
                Ok(Async::Ready(Some((line, src)))) => {
                    // look for buffered content from same source
                    if self.buffers.contains_key(&src) {
                        if self.marker.is_match(line.as_ref()) {
                            // buffer the incoming line and flush the existing data
                            let buffered = self
                                .buffers
                                .insert(src.clone(), line.into())
                                .expect("already asserted key is present");
                            return Ok(Async::Ready(Some((buffered.freeze(), src))));
                        } else {
                            // append new line to the buffered data
                            let buffered = self
                                .buffers
                                .get_mut(&src)
                                .expect("already asserted key is present");
                            buffered.extend_from_slice(b"\n");
                            buffered.extend_from_slice(&line);
                        }
                    } else {
                        // no existing data for this source so buffer it with timeout
                        self.timeouts
                            .insert(src.clone(), Duration::from_millis(self.timeout));
                        self.buffers.insert(src, line.into());
                    }
                }
                Ok(Async::Ready(None)) => {
                    // start flushing all existing data, stop polling inner
                    self.draining =
                        Some(self.buffers.drain().map(|(k, v)| (v.into(), k)).collect());
                }
                Ok(Async::NotReady) => {
                    if let Some(key) = self.expired.pop_front() {
                        if let Some(buffered) = self.buffers.remove(&key) {
                            return Ok(Async::Ready(Some((buffered.freeze(), key))));
                        }
                    }

                    return Ok(Async::NotReady);
                }
                Err(()) => return Err(()),
            };
        }
    }
}

use crate::sinks::util::Batch;
use futures::{
    future::Either, stream::FuturesUnordered, sync::oneshot, Async, AsyncSink, Future, Poll, Sink,
    StartSend, Stream,
};
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    time::{Duration, Instant},
};
use tokio::timer::Delay;

pub trait Partition {
    type Item;

    fn partition(&self, event: &Self::Item) -> String;
}

// TODO: Make this a concrete type
type LingerDelay = Box<dyn Future<Item = LingerState, Error = ()> + Send + 'static>;

pub struct PartitionedBatchSink<B, S, P> {
    batch: B,
    sink: S,
    partitioner: P,
    partitions: HashMap<String, B>,
    config: Config,
    closing: bool,
    sending: VecDeque<B>,
    lingers: FuturesUnordered<LingerDelay>,
    linger_handles: HashMap<String, oneshot::Sender<String>>,
}

#[derive(Copy, Debug, Clone)]
struct Config {
    max_linger: Option<Duration>,
    max_size: usize,
    min_size: usize,
}

enum LingerState {
    Elapsed(String),
    Canceled,
}

impl<B, S, P> PartitionedBatchSink<B, S, P> {
    pub fn new(sink: S, batch: B, partitioner: P, max_size: usize) -> Self {
        let config = Config {
            max_linger: None,
            max_size: max_size,
            min_size: 0,
        };

        Self {
            batch,
            sink,
            partitioner,
            partitions: HashMap::new(),
            config,
            closing: false,
            sending: VecDeque::new(),
            lingers: FuturesUnordered::new(),
            linger_handles: HashMap::new(),
        }
    }

    pub fn with_linger(
        sink: S,
        batch: B,
        partitioner: P,
        max_size: usize,
        min_size: usize,
        linger: Duration,
    ) -> Self {
        let config = Config {
            max_linger: Some(linger),
            max_size,
            min_size,
        };

        Self {
            batch,
            sink,
            partitioner,
            partitions: HashMap::new(),
            config,
            closing: false,
            sending: VecDeque::new(),
            lingers: FuturesUnordered::new(),
            linger_handles: HashMap::new(),
        }
    }

    pub fn into_inner_sink(self) -> S {
        self.sink
    }

    pub fn set_linger(&mut self, partition: String) {
        if let Some(max_linger) = self.config.max_linger {
            let (tx, rx) = oneshot::channel();
            let partition_clone = partition.clone();

            let delay = Delay::new(Instant::now() + max_linger)
                .map(move |_| LingerState::Elapsed(partition_clone))
                .map_err(|_| ());

            let cancel = rx.map(|_| LingerState::Canceled).map_err(|_| ());

            let fut = cancel
                .select2(delay)
                .map(|state| match state {
                    Either::A((state, _)) => state,
                    Either::B((state, _)) => state,
                })
                .map_err(|_| ());

            self.linger_handles.insert(partition, tx);
            self.lingers.push(Box::new(fut));
        }
    }
}

impl<B, S, P> Sink for PartitionedBatchSink<B, S, P>
where
    B: Batch,
    S: Sink<SinkItem = B>,
    P: Partition<Item = B::Input>,
{
    type SinkItem = B::Input;
    type SinkError = S::SinkError;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        // Apply back pressure if we are buffering more than
        // 5 batches, this should only happen if the inner sink
        // is apply back pressure.
        if self.sending.len() > 5 {
            self.poll_complete()?;

            if self.sending.len() > 5 {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        let partition = self.partitioner.partition(&item);

        if let Some(batch) = self.partitions.get_mut(&partition) {
            if batch.len() >= self.config.max_size {
                self.poll_complete()?;

                if let Some(batch) = self.partitions.get_mut(&partition) {
                    if batch.len() >= self.config.max_size {
                        return Ok(AsyncSink::NotReady(item));
                    } else {
                        batch.push(item);
                        return Ok(AsyncSink::Ready);
                    }
                }
            } else {
                batch.push(item);
                return Ok(AsyncSink::Ready);
            }
        }

        // We fall through to this case, when there is no batch already
        // or the batch got submitted by polling_complete above.
        let mut batch = self.batch.fresh();

        batch.push(item.into());
        self.set_linger(partition.clone());

        self.partitions.insert(partition, batch);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.sink.poll_complete()?;

        while let Some(batch) = self.sending.pop_front() {
            if let AsyncSink::NotReady(batch) = self.sink.start_send(batch)? {
                self.sending.push_front(batch);
                return Ok(Async::NotReady);
            } else {
                self.sink.poll_complete()?;
            }
        }

        let closing = self.closing;
        let max_size = self.config.max_size;
        let min_size = self.config.min_size;

        let mut partitions = Vec::new();
        while let Ok(Async::Ready(Some(linger))) = self.lingers.poll() {
            // Only if the linger has elapsed trigger the removal
            if let LingerState::Elapsed(partition) = linger {
                self.linger_handles.remove(&partition);

                if let Some(batch) = self.partitions.remove(&partition) {
                    partitions.push(batch);
                }
            }
        }

        let ready = self
            .partitions
            .iter()
            .filter(|(_, b)| closing || (b.len() >= max_size || b.len() >= min_size))
            .map(|(p, _)| p.clone())
            .collect::<Vec<_>>();

        let mut ready_batches = Vec::new();
        for partition in ready {
            if let Some(batch) = self.partitions.remove(&partition) {
                if let Some(linger_cancel) = self.linger_handles.remove(&partition) {
                    linger_cancel
                        .send(partition.clone())
                        .expect("Linger deadline should be removed on elapsed.");
                }

                ready_batches.push(batch);
            }
        }

        for batch in ready_batches.into_iter().chain(partitions) {
            if let AsyncSink::NotReady(batch) = self.sink.start_send(batch)? {
                self.sending.push_front(batch);
                return Ok(Async::NotReady);
            } else {
                self.sink.poll_complete()?;
            }
        }

        self.sink.poll_complete()
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        self.closing = true;
        self.poll_complete()
    }
}

impl<B, S, P> fmt::Debug for PartitionedBatchSink<B, S, P> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("PartitionedBatchSink")
            .field("max_linger", &self.config.max_linger)
            .field("max_size", &self.config.max_size)
            .field("min_size", &self.config.min_size)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::Buffer;
    use futures::{Future, Sink};
    use std::time::Duration;
    use tokio_test::clock;

    #[test]
    fn batch_sink_buffers_messages_until_limit() {
        let buffered = PartitionedBatchSink::new(Vec::new(), Vec::new(), StaticPartitioner, 10);

        let (buffered, _) = buffered
            .send_all(futures::stream::iter_ok(0..22))
            .wait()
            .unwrap();

        let output = buffered.into_inner_sink();
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
        let buffered = PartitionedBatchSink::new(Vec::new(), Vec::new(), StaticPartitioner, 10);

        let buffered = buffered.send(0).wait().unwrap();
        let buffered = buffered.send(1).wait().unwrap();

        let output = buffered.into_inner_sink();
        assert_eq!(output, vec![vec![0], vec![1],]);
    }

    #[test]
    fn batch_sink_allows_the_final_item_to_exceed_the_buffer_size() {
        let buffered =
            PartitionedBatchSink::new(Vec::new(), Buffer::new(false), StaticVecPartitioner, 10);

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
            .into_inner_sink()
            .into_iter()
            .map(|buf| buf.finish())
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

    #[test]
    fn batch_sink_buffers_by_partition_buffer_size_one() {
        let buffered = PartitionedBatchSink::new(Vec::new(), Vec::new(), DynamicPartitioner, 1);

        let input = vec![(Partitions::A, 0), (Partitions::B, 1)];

        let (buffered, _) = buffered
            .send_all(futures::stream::iter_ok(input))
            .wait()
            .unwrap();

        let mut output = buffered.into_inner_sink();
        output[..].sort();
        assert_eq!(
            output,
            vec![vec![(Partitions::A, 0)], vec![(Partitions::B, 1)]]
        );
    }

    #[test]
    fn batch_sink_buffers_by_partition_buffer_size_two() {
        let buffered = PartitionedBatchSink::new(Vec::new(), Vec::new(), DynamicPartitioner, 2);

        let input = vec![
            (Partitions::A, 0),
            (Partitions::B, 1),
            (Partitions::A, 2),
            (Partitions::B, 3),
        ];

        let (buffered, _) = buffered
            .send_all(futures::stream::iter_ok(input))
            .wait()
            .unwrap();

        let mut output = buffered.into_inner_sink();
        output[..].sort();
        assert_eq!(
            output,
            vec![
                vec![(Partitions::A, 0), (Partitions::A, 2)],
                vec![(Partitions::B, 1), (Partitions::B, 3)]
            ]
        );
    }

    #[test]
    fn batch_sink_submits_after_linger() {
        let mut buffered = PartitionedBatchSink::with_linger(
            Vec::new(),
            Vec::new(),
            StaticPartitioner,
            10,
            2,
            Duration::from_secs(1),
        );

        clock::mock(|handle| {
            buffered.start_send(1 as usize).unwrap();
            buffered.poll_complete().unwrap();

            handle.advance(Duration::from_secs(2));

            buffered.poll_complete().unwrap();
        });

        let output = buffered.into_inner_sink();
        assert_eq!(output, vec![vec![1]]);
    }

    #[test]
    fn batch_sink_cancels_linger() {
        let mut buffered = PartitionedBatchSink::with_linger(
            Vec::new(),
            Vec::new(),
            StaticPartitioner,
            10,
            2,
            Duration::from_secs(1),
        );

        clock::mock(|handle| {
            buffered.start_send(1 as usize).unwrap();
            buffered.start_send(2 as usize).unwrap();
            buffered.poll_complete().unwrap();

            handle.advance(Duration::from_secs(2));
            std::thread::sleep(Duration::from_secs(2));

            buffered.start_send(3 as usize).unwrap();
            buffered.poll_complete().unwrap();
        });

        let output = buffered.into_inner_sink();
        assert_eq!(output, vec![vec![1, 2]]);
    }

    #[derive(Debug, PartialEq, Eq, Ord, PartialOrd)]
    enum Partitions {
        A,
        B,
    }

    struct DynamicPartitioner;

    impl Partition for DynamicPartitioner {
        type Item = (Partitions, usize);

        fn partition(&self, event: &Self::Item) -> String {
            format!("{:?}", event.0)
        }
    }

    struct StaticPartitioner;

    impl Partition for StaticPartitioner {
        type Item = usize;

        fn partition(&self, _event: &Self::Item) -> String {
            "key".into()
        }
    }

    struct StaticVecPartitioner;

    impl Partition for StaticVecPartitioner {
        type Item = Vec<u8>;

        fn partition(&self, _event: &Self::Item) -> String {
            "key".into()
        }
    }
}

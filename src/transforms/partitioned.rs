//! Runs several instances of a stateful task transform in parallel, partitioned by key.
//!
//! A task transform runs in a single task, so it uses at most one core. When its state splits by a
//! key (reduce's `Discriminant`, for example) and the events of different keys never share state,
//! [`Partitioned`] runs one instance per shard: a router task computes each event's key and sends
//! the event to the shard that owns that key. All events of a key reach the same shard in their
//! arrival order, so each instance behaves as it would alone. Only the order of output events
//! across different keys may change.

use std::{
    hash::{BuildHasher, Hash, RandomState},
    iter,
    num::NonZeroUsize,
    pin::Pin,
};

use async_stream::stream;
use futures::{
    Stream, StreamExt as _,
    stream::{self, BoxStream},
};
use metrics::Counter;
use tokio::{
    sync::mpsc,
    task::{JoinError, JoinSet},
};
use tokio_stream::wrappers::ReceiverStream;
use vector_lib::{cpu_time::SpawnTimedExt as _, source_sender::chunk_size_events};

use crate::{event::Event, transforms::TaskTransform};

/// How many batches can wait in one shard's queue before the router waits.
const SHARD_QUEUE_BATCHES: usize = 4;

/// How many output batches per shard the shared output queue holds. It only absorbs bursts.
const OUTPUT_QUEUE_BATCHES_PER_SHARD: usize = 1;

/// Events sent to one shard, each with its key.
type KeyedBatch<K> = Vec<(K, Event)>;

/// A task transform whose state splits by key: events with different keys never share state.
///
/// [`Partitioned`] computes each event's key once, in its router, and hands the events of a shard
/// to that shard's instance with their keys.
pub trait KeyedTransform: Send + 'static {
    /// The key that partitions the transform's state.
    type Key: Hash + Send + 'static;

    /// The key of `event`: two events share state only if they have the same key.
    ///
    /// [`Partitioned`] calls it in its router, for every event, so it must have no side effects
    /// (no internal events).
    fn key(&self, event: &Event) -> Self::Key;

    /// Transforms a stream of events whose keys the router already computed.
    fn transform_keyed(
        self: Box<Self>,
        input: BoxStream<'static, (Self::Key, Event)>,
    ) -> BoxStream<'static, Event>;
}

/// Runs `shards` instances of a [`KeyedTransform`]. Every event of a key goes to the same instance,
/// in arrival order.
pub struct Partitioned<T: KeyedTransform> {
    /// Builds a new, empty instance: one per shard, and one for the router to compute keys with.
    new_instance: Box<dyn Fn() -> T + Send>,
    shards: NonZeroUsize,
    /// The component's CPU-time counter, so that the router's and shards' CPU time counts for it.
    cpu_ns: Option<Counter>,
}

impl<T: KeyedTransform> Partitioned<T> {
    pub fn new(
        new_instance: impl Fn() -> T + Send + 'static,
        shards: NonZeroUsize,
        cpu_ns: Option<Counter>,
    ) -> Self {
        Self {
            new_instance: Box::new(new_instance),
            shards,
            cpu_ns,
        }
    }
}

impl<T: KeyedTransform> TaskTransform<Event> for Partitioned<T> {
    fn transform(
        self: Box<Self>,
        input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let Self {
            new_instance,
            shards,
            cpu_ns,
        } = *self;

        let mut tasks = JoinSet::new();
        let (output_tx, output_rx) = mpsc::channel(shards.get() * OUTPUT_QUEUE_BATCHES_PER_SHARD);

        let shard_senders = iter::repeat_with(|| {
            let (shard_tx, shard_rx) = mpsc::channel(SHARD_QUEUE_BATCHES);
            let shard = Box::new(new_instance());
            tasks.spawn_timed(
                run_shard(shard, shard_rx, output_tx.clone()),
                cpu_ns.clone(),
            );
            shard_tx
        })
        .take(shards.get())
        .collect();

        // The shards now hold the only senders, so the output ends once every shard has ended.
        drop(output_tx);

        tasks.spawn_timed(route(new_instance(), shard_senders, input_rx), cpu_ns);

        stream! {
            let mut output = ReceiverStream::new(output_rx);
            loop {
                let batch = tokio::select! {
                    batch = output.next() => batch,
                    // A shard that panics drops only its own output sender, so the output can stay
                    // open as long as the other shards run: re-raise the panic as soon as it happens.
                    Some(result) = tasks.join_next() => {
                        reraise_panic(result);
                        continue;
                    }
                };
                let Some(batch) = batch else { break };
                yield batch;
            }

            while let Some(result) = tasks.join_next().await {
                reraise_panic(result);
            }
        }
        .flat_map(stream::iter)
        .boxed()
    }
}

/// Re-raises the panic of a task that panicked.
///
/// A task fails only by panicking or by being aborted. Tokio only aborts them when their `JoinSet`
/// is dropped, and the output stream owns it, so a failure is always a panic.
fn reraise_panic(result: Result<(), JoinError>) {
    match result {
        Ok(()) => {}
        Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
        Err(_) => unreachable!("partitioned task cancelled while its JoinSet is still owned"),
    }
}

/// Runs one shard and forwards its output in batches.
async fn run_shard<T: KeyedTransform>(
    shard: Box<T>,
    shard_rx: mpsc::Receiver<KeyedBatch<T::Key>>,
    output_tx: mpsc::Sender<Vec<Event>>,
) {
    let input = ReceiverStream::new(shard_rx).flat_map(stream::iter).boxed();
    let mut output = shard
        .transform_keyed(input)
        .ready_chunks(chunk_size_events());

    while let Some(batch) = output.next().await {
        // The receiver is gone only when the output stream was dropped, which also aborts this
        // task: nothing is left to deliver.
        if output_tx.send(batch).await.is_err() {
            return;
        }
    }
}

/// Sends each event, with its key, to the shard that owns the key. `keys` only computes keys: its
/// state is never used.
async fn route<T: KeyedTransform>(
    keys: T,
    shard_senders: Vec<mpsc::Sender<KeyedBatch<T::Key>>>,
    input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
) {
    let hasher = RandomState::new();
    let mut input = input_rx.ready_chunks(chunk_size_events());

    while let Some(events) = input.next().await {
        // Keys usually spread evenly, so each shard gets about its share of the chunk.
        let per_shard = events.len().div_ceil(shard_senders.len());
        let mut batches: Vec<KeyedBatch<T::Key>> =
            iter::repeat_with(|| Vec::with_capacity(per_shard))
                .take(shard_senders.len())
                .collect();

        for event in events {
            let key = keys.key(&event);
            let shard = shard_index(hasher.hash_one(&key), shard_senders.len());
            batches[shard].push((key, event));
        }

        for (shard_tx, batch) in shard_senders.iter().zip(batches) {
            // A shard's receiver is gone only when that shard panicked: the output stream re-raises
            // the panic, so there is nothing left to route.
            if !batch.is_empty() && shard_tx.send(batch).await.is_err() {
                return;
            }
        }
    }
}

/// Maps a key's hash to a shard. The same hash always maps to the same shard.
const fn shard_index(hash: u64, shards: usize) -> usize {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the remainder is below `shards`, which is a `usize`"
    )]
    let index = (hash % shards as u64) as usize;
    index
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, panic::AssertUnwindSafe, time::Duration};

    use futures::FutureExt as _;
    use vrl::event_path;

    use super::*;
    use crate::event::LogEvent;

    const SHARDS: NonZeroUsize = NonZeroUsize::new(4).unwrap();

    /// Emits each event unchanged, and panics on an event that has a `panic` field.
    struct PassThrough;

    impl KeyedTransform for PassThrough {
        type Key = i64;

        fn key(&self, event: &Event) -> i64 {
            key_of(event)
        }

        fn transform_keyed(
            self: Box<Self>,
            input: BoxStream<'static, (i64, Event)>,
        ) -> BoxStream<'static, Event> {
            input
                .map(|(_key, event)| {
                    assert!(
                        !event.as_log().contains(event_path!("panic")),
                        "shard panicked on purpose"
                    );
                    event
                })
                .boxed()
        }
    }

    fn event(key: i64, sequence: i64) -> Event {
        let mut log = LogEvent::default();
        log.insert(event_path!("key"), key);
        log.insert(event_path!("sequence"), sequence);
        log.into()
    }

    fn key_of(event: &Event) -> i64 {
        event.as_log()["key"]
            .as_integer()
            .expect("test events have an integer key")
    }

    /// Runs `input` through `SHARDS` partitioned `PassThrough` instances.
    fn run(
        input: impl Stream<Item = Event> + Send + 'static,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        Box::new(Partitioned::new(|| PassThrough, SHARDS, None)).transform(input.boxed())
    }

    /// The events of each key, in output order.
    fn per_key(events: Vec<Event>) -> BTreeMap<i64, Vec<Event>> {
        let mut per_key = BTreeMap::<i64, Vec<Event>>::new();
        for event in events {
            per_key.entry(key_of(&event)).or_default().push(event);
        }
        per_key
    }

    #[tokio::test]
    async fn keeps_every_event_and_the_order_within_each_key() {
        let input: Vec<Event> = (0..1_000)
            .map(|sequence| event(sequence % 13, sequence))
            .collect();

        let output: Vec<Event> = run(stream::iter(input.clone())).collect().await;

        assert_eq!(per_key(output), per_key(input));
    }

    #[tokio::test]
    async fn reraises_a_shard_panic_while_the_other_shards_run() {
        let mut poison = event(1, 1);
        poison.as_mut_log().insert(event_path!("panic"), true);

        // The input never ends and the poisoned shard never receives another event, so the panic
        // can't surface through the router or the end of the output: only by watching the tasks.
        let input = stream::iter([event(2, 2), poison]).chain(stream::pending());
        let output = run(input).collect::<Vec<Event>>();

        let result = tokio::time::timeout(
            Duration::from_secs(10),
            AssertUnwindSafe(output).catch_unwind(),
        )
        .await
        .expect("the panic is re-raised without waiting for the input to end");
        assert!(result.is_err());
    }

    #[test]
    fn maps_a_hash_to_a_shard_below_the_shard_count() {
        assert_eq!(shard_index(0, 4), 0);
        assert_eq!(shard_index(7, 4), 3);
        assert_eq!(shard_index(u64::MAX, 3), (u64::MAX % 3) as usize);
    }
}

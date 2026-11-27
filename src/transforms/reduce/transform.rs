use std::{
    collections::{HashMap, hash_map::Entry},
    hash::{BuildHasher, Hash, Hasher, RandomState},
    num::NonZeroUsize,
    pin::Pin,
    time::{Duration, Instant},
};
use vector_lib::internal_event::emit;

use async_stream::stream;
use futures::{FutureExt as _, Stream, StreamExt, future, stream};
use indexmap::IndexMap;
use tokio::{sync::mpsc, task::JoinSet};
use tokio_stream::wrappers::ReceiverStream;
use vector_lib::{
    cpu_time::SpawnTimedExt,
    stream::expiration_map::{Emitter, map_with_expiration},
};
use vector_vrl_metrics::MetricsStorage;
use vrl::{
    path::{OwnedSegment, OwnedTargetPath, parse_target_path},
    prelude::KeyString,
};

use crate::{
    conditions::Condition,
    event::{Event, EventMetadata, LogEvent, discriminant::Discriminant},
    internal_events::{ReduceAddEventError, ReduceStaleEventFlushed},
    transforms::{
        TaskTransform,
        reduce::{
            config::ReduceConfig,
            merge_strategy::{MergeStrategy, ReduceValueMerger, get_value_merger},
        },
    },
};

#[derive(Clone, Debug)]
struct ReduceState {
    events: usize,
    fields: HashMap<OwnedTargetPath, Box<dyn ReduceValueMerger>>,
    stale_since: Instant,
    creation: Instant,
    metadata: EventMetadata,
}

fn is_covered_by_strategy(
    path: &OwnedTargetPath,
    strategies: &IndexMap<OwnedTargetPath, MergeStrategy>,
) -> bool {
    let mut current = OwnedTargetPath::event_root();
    for component in &path.path.segments {
        current = current.with_field_appended(&component.to_string());
        if strategies.contains_key(&current) {
            return true;
        }
    }
    false
}

impl ReduceState {
    fn new() -> Self {
        Self {
            events: 0,
            stale_since: Instant::now(),
            creation: Instant::now(),
            fields: HashMap::new(),
            metadata: EventMetadata::default(),
        }
    }

    fn add_event(&mut self, e: &LogEvent, strategies: &IndexMap<OwnedTargetPath, MergeStrategy>) {
        self.metadata.merge(e.metadata().clone());

        for (path, strategy) in strategies {
            if let Some(value) = e.get(path) {
                match self.fields.entry(path.clone()) {
                    Entry::Vacant(entry) => match get_value_merger(value.clone(), strategy) {
                        Ok(m) => {
                            entry.insert(m);
                        }
                        Err(error) => {
                            warn!(message = "Failed to create value merger.", %error, %path);
                        }
                    },
                    Entry::Occupied(mut entry) => {
                        if let Err(error) = entry.get_mut().add(value.clone()) {
                            warn!(message = "Failed to merge value.", %error);
                        }
                    }
                }
            }
        }

        if let Some(fields_iter) = e.all_event_fields_skip_array_elements() {
            for (path, value) in fields_iter {
                // This should not return an error, unless there is a bug in the event fields iterator.
                let parsed_path = match parse_target_path(&path) {
                    Ok(path) => path,
                    Err(error) => {
                        emit(ReduceAddEventError { error, path });
                        continue;
                    }
                };
                if is_covered_by_strategy(&parsed_path, strategies) {
                    continue;
                }

                match self.fields.entry(parsed_path) {
                    Entry::Vacant(entry) => {
                        entry.insert(value.clone().into());
                    }
                    Entry::Occupied(mut entry) => {
                        if let Err(error) = entry.get_mut().add(value.clone()) {
                            warn!(message = "Failed to merge value.", %error);
                        }
                    }
                }
            }
        }
        // else the event root is not an object (see https://github.com/vectordotdev/vector/issues/18219)

        self.events += 1;
        self.stale_since = Instant::now();
    }

    fn flush(mut self) -> LogEvent {
        let mut event = LogEvent::new_with_metadata(self.metadata);
        for (path, v) in self.fields.drain() {
            if let Err(error) = v.insert_into(&path, &mut event) {
                warn!(message = "Failed to merge values for field.", %error);
            }
        }
        self.events = 0;
        event
    }
}

#[derive(Clone, Debug)]
pub struct Reduce {
    expire_after: Duration,
    flush_period: Duration,
    end_every_period: Option<Duration>,
    group_by: Vec<String>,
    merge_strategies: IndexMap<OwnedTargetPath, MergeStrategy>,
    merge_states: HashMap<Discriminant, ReduceState>,
    ends_when: Option<Condition>,
    starts_when: Option<Condition>,
    max_events: Option<usize>,
}

fn validate_merge_strategies(strategies: &IndexMap<KeyString, MergeStrategy>) -> crate::Result<()> {
    for (path, _) in strategies {
        let contains_index = parse_target_path(path)
            .map_err(|_| format!("Could not parse path: `{path}`"))?
            .path
            .segments
            .iter()
            .any(OwnedSegment::is_index);
        if contains_index {
            return Err(format!(
                "Merge strategies with indexes are currently not supported. Path: `{path}`"
            )
            .into());
        }
    }

    Ok(())
}

impl Reduce {
    ///
    /// # Errors
    ///
    /// Returns an error for incompatible conditions, invalid merge paths, or a condition that cannot be built.
    #[expect(
        clippy::missing_panics_doc,
        reason = "merge strategy paths are validated by validate_merge_strategies"
    )]
    pub fn new(
        config: &ReduceConfig,
        enrichment_tables: &vector_lib::enrichment::TableRegistry,
        metrics_storage: &MetricsStorage,
    ) -> crate::Result<Self> {
        if config.ends_when.is_some() && config.starts_when.is_some() {
            return Err("only one of `ends_when` and `starts_when` can be provided".into());
        }

        let ends_when = config
            .ends_when
            .as_ref()
            .map(|c| c.build(enrichment_tables, metrics_storage))
            .transpose()?;
        let starts_when = config
            .starts_when
            .as_ref()
            .map(|c| c.build(enrichment_tables, metrics_storage))
            .transpose()?;
        let group_by = config.group_by.clone().into_iter().collect();
        let max_events = config.max_events.map(std::convert::Into::into);

        validate_merge_strategies(&config.merge_strategies)?;

        Ok(Reduce {
            expire_after: config.expire_after_ms,
            flush_period: config.flush_period_ms,
            end_every_period: config.end_every_period_ms,
            group_by,
            merge_strategies: config
                .merge_strategies
                .iter()
                .map(|(path, strategy)| {
                    // There are no invalid paths here: `validate_merge_strategies` above already
                    // returns a configuration error for any path that fails to parse.
                    let path =
                        parse_target_path(path).expect("validated by validate_merge_strategies");
                    (path, strategy.clone())
                })
                .collect(),
            merge_states: HashMap::new(),
            ends_when,
            starts_when,
            max_events,
        })
    }

    fn flush_into(&mut self, emitter: &mut Emitter<Event>) {
        let mut flush_discriminants = Vec::new();
        let now = Instant::now();
        for (k, t) in &self.merge_states {
            if let Some(period) = self.end_every_period
                && (now - t.creation) >= period
            {
                flush_discriminants.push(k.clone());
            }

            if (now - t.stale_since) >= self.expire_after {
                flush_discriminants.push(k.clone());
            }
        }
        for k in &flush_discriminants {
            if let Some(t) = self.merge_states.remove(k) {
                emit(ReduceStaleEventFlushed);
                emitter.emit(Event::from(t.flush()));
            }
        }
    }

    fn flush_all_into(&mut self, emitter: &mut Emitter<Event>) {
        self.merge_states
            .drain()
            .for_each(|(_, s)| emitter.emit(Event::from(s.flush())));
    }

    fn push_or_new_reduce_state(&mut self, event: &LogEvent, discriminant: Discriminant) {
        match self.merge_states.entry(discriminant) {
            Entry::Vacant(entry) => {
                let mut state = ReduceState::new();
                state.add_event(event, &self.merge_strategies);
                entry.insert(state);
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().add_event(event, &self.merge_strategies);
            }
        }
    }

    pub fn transform_one(&mut self, emitter: &mut Emitter<Event>, event: Event) {
        let (starts_here, event) = match &self.starts_when {
            Some(condition) => condition.check(event),
            None => (false, event),
        };

        let (mut ends_here, event) = match &self.ends_when {
            Some(condition) => condition.check(event),
            None => (false, event),
        };

        let event = event.into_log();
        let discriminant = Discriminant::from_log_event(&event, &self.group_by);

        if let Some(max_events) = self.max_events {
            if max_events == 1 {
                ends_here = true;
            } else if let Some(entry) = self.merge_states.get(&discriminant) {
                // The current event will finish this set
                if entry.events + 1 == max_events {
                    ends_here = true;
                }
            }
        }

        if starts_here {
            if let Some(state) = self.merge_states.remove(&discriminant) {
                emitter.emit(state.flush().into());
            }

            self.push_or_new_reduce_state(&event, discriminant);
        } else if ends_here {
            emitter.emit({
                let mut state = self
                    .merge_states
                    .remove(&discriminant)
                    .unwrap_or_else(ReduceState::new);
                state.add_event(&event, &self.merge_strategies);
                state.flush().into()
            });
        } else {
            self.push_or_new_reduce_state(&event, discriminant);
        }
    }
}

impl TaskTransform<Event> for Reduce {
    fn transform(
        self: Box<Self>,
        input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let transform_fn = move |me: &mut Box<Reduce>, event, emitter: &mut Emitter<Event>| {
            me.transform_one(emitter, event);
        };

        construct_output_stream(self, input_rx, transform_fn)
    }
}

pub fn construct_output_stream(
    reduce: Box<Reduce>,
    input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    mut transform_fn: impl FnMut(&mut Box<Reduce>, Event, &mut Emitter<Event>) + Send + Sync + 'static,
) -> Pin<Box<dyn Stream<Item = Event> + Send>>
where
    Reduce: 'static,
{
    let flush_period = reduce.flush_period;
    Box::pin(map_with_expiration(
        reduce,
        input_rx,
        flush_period,
        move |me, event, emitter| {
            transform_fn(me, event, emitter);
        },
        |me, emitter| {
            me.flush_into(emitter);
        },
        |me, emitter| {
            me.flush_all_into(emitter);
        },
    ))
}

/// The most shards a [`ShardedReduce`] should run.
///
/// From the reduce concurrency benchmarks, the worst case for a cap: with very few keys per CPU,
/// the best shard count is 8 x CPUs, and throughput still rose at 64 shards on 8 CPUs, the most
/// run. 128 allows 8 x CPUs up to 16 CPUs, past the 12 measured. Beyond it, extra shards only
/// add tasks, queues and CPU.
pub const MAX_SHARDS: usize = 128;

// How many batches can wait in one shard's queue before the router has to wait.
const SHARD_QUEUE_BATCHES: usize = 4;

/// Maximum number of events in a batch sent from the router to a shard, or from a shard to the
/// output. It bounds each shard queue to `SHARD_QUEUE_BATCHES * BATCH_EVENTS` events.
pub const BATCH_EVENTS: usize = 100;

/// The longest the router keeps an event in a partial batch while its input stays ready.
///
/// The router sends every partial batch as soon as its input has nothing ready. Under sustained
/// load the input may never pause, so this bounds the wait of the shards that get few events, such
/// as next to a hot key. `expire_after_ms` and `end_every_period_ms` cap it, so that shards don't
/// see events much later than a single task would.
const MAX_BATCH_DELAY: Duration = Duration::from_millis(5);

// All shards send into one shared output channel, which holds
// `shards * OUTPUT_QUEUE_BATCHES_PER_SHARD` batches. Only there to absorb bursts.
const OUTPUT_QUEUE_BATCHES_PER_SHARD: usize = 1;

#[derive(Debug)]
pub struct ShardedReduce {
    reduce: Reduce,
    shards: NonZeroUsize,
}

impl ShardedReduce {
    pub const fn new(reduce: Reduce, shards: NonZeroUsize) -> Self {
        Self { reduce, shards }
    }
}

impl TaskTransform<Event> for ShardedReduce {
    fn transform(
        self: Box<Self>,
        input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut tasks = JoinSet::new();

        let (output_tx, output_rx) =
            mpsc::channel(self.shards.get() * OUTPUT_QUEUE_BATCHES_PER_SHARD);
        let shards_senders = (0..self.shards.get())
            .map(|_| {
                let (tx, rx) = mpsc::channel::<Vec<Event>>(SHARD_QUEUE_BATCHES);
                tasks.spawn_timed(run_shard(
                    Box::new(self.reduce.clone()),
                    rx,
                    output_tx.clone(),
                ));
                tx
            })
            .collect();

        drop(output_tx);

        let max_batch_delay = [Some(self.reduce.expire_after), self.reduce.end_every_period]
            .into_iter()
            .flatten()
            .fold(MAX_BATCH_DELAY, Duration::min);

        tasks.spawn_timed(route(
            self.reduce.group_by,
            shards_senders,
            max_batch_delay,
            input_rx,
        ));

        stream! {
            let mut output = std::pin::pin!(ReceiverStream::new(output_rx));
            while let Some(item) = output.next().await {
                yield item;
            }

            // A task fails only by panicking or by being aborted. As we never abort a task ourselves,
            // and tokio only aborts them when the `JoinSet` is dropped, an error is always a panic.
            while let Some(result) = tasks.join_next().await {
                match result {
                    Ok(()) => {},
                    Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
                    Err(_) => {
                        unreachable!("reduce task cancelled while its JoinSet is still owned")
                    }
                }
            }
        }
        .flat_map(stream::iter)
        .boxed()
    }
}

async fn run_shard(
    reduce: Box<Reduce>,
    rx: mpsc::Receiver<Vec<Event>>,
    sender: mpsc::Sender<Vec<Event>>,
) {
    let events = ReceiverStream::new(rx).flat_map(stream::iter).boxed();

    let mut reduced = reduce.transform(events).ready_chunks(BATCH_EVENTS);

    while let Some(batch) = reduced.next().await {
        // Fails only if the topology dropped the output stream. A normal shutdown ends the input instead.
        if sender.send(batch).await.is_err() {
            return;
        }
    }
}

/// Sends each event to the shard of its group, in batches of up to `BATCH_EVENTS` events per shard.
///
/// Each shard's batch fills across input arrays, so its size doesn't depend on the shard count.
async fn route(
    group_by: Vec<String>,
    shard_senders: Vec<mpsc::Sender<Vec<Event>>>,
    max_batch_delay: Duration,
    mut input: Pin<Box<dyn Stream<Item = Event> + Send>>,
) {
    let hasher = RandomState::new();
    let shards = shard_senders.len();
    let mut batches = ShardBatches::new(shard_senders, max_batch_delay);

    loop {
        let event = if let Some(event) = input.next().now_or_never() {
            event
        } else {
            // Nothing is ready: send the partial batches instead of holding them while waiting.
            if batches.send_partial().await.is_err() {
                return;
            }
            input.next().await
        };
        let Some(event) = event else {
            break;
        };

        let shard = shard_of(&hasher, &group_by, &event, shards);
        if batches.push(shard, event).await.is_err() {
            return;
        }
    }

    // The input ended: send what is left.
    _ = batches.send_partial().await;
}

/// The shard of an event's group: all the events of a group get the same one.
#[expect(
    clippy::cast_possible_truncation,
    reason = "deterministic: the same hash always maps to the same shard, and the low bits stay uniform"
)]
fn shard_of(hasher: &RandomState, group_by: &[String], event: &Event, shards: usize) -> usize {
    let log = event.as_log();
    let mut state = hasher.build_hasher();
    group_by
        .iter()
        .map(|field| log.parse_path_and_get_value(field).ok().flatten())
        .for_each(|value| value.hash(&mut state));

    (state.finish() as usize) % shards
}

/// A shard stopped, which happens only after the topology dropped the output stream.
#[derive(Debug)]
struct ShardStopped;

/// The router's batch for each shard.
struct ShardBatches {
    senders: Vec<mpsc::Sender<Vec<Event>>>,
    batches: Vec<Vec<Event>>,
    max_delay: Duration,
    /// When the router got its first event since it last sent the partial batches, so no event in a
    /// partial batch is older.
    oldest: Option<tokio::time::Instant>,
}

impl ShardBatches {
    fn new(senders: Vec<mpsc::Sender<Vec<Event>>>, max_delay: Duration) -> Self {
        let batches = senders.iter().map(|_| Vec::new()).collect();
        Self {
            senders,
            batches,
            max_delay,
            oldest: None,
        }
    }

    /// Adds an event to its shard's batch, and sends the batch once it is full.
    async fn push(&mut self, shard: usize, event: Event) -> Result<(), ShardStopped> {
        self.oldest.get_or_insert_with(tokio::time::Instant::now);

        let batch = &mut self.batches[shard];
        batch.push(event);
        if batch.len() < BATCH_EVENTS {
            return Ok(());
        }

        let full = std::mem::replace(batch, Vec::with_capacity(BATCH_EVENTS));
        self.senders[shard]
            .send(full)
            .await
            .map_err(|_| ShardStopped)?;

        // `route` sends the partial batches only when its input pauses, which may not happen
        // under sustained load.
        if self
            .oldest
            .is_some_and(|oldest| oldest.elapsed() >= self.max_delay)
        {
            self.send_partial().await?;
        }
        Ok(())
    }

    /// Sends every partial batch, concurrently so that a full shard queue doesn't hold back the
    /// others.
    async fn send_partial(&mut self) -> Result<(), ShardStopped> {
        self.oldest = None;

        let sends = self
            .senders
            .iter()
            .zip(&mut self.batches)
            .filter(|(_, batch)| !batch.is_empty())
            .map(|(sender, batch)| sender.send(std::mem::take(batch)));

        future::try_join_all(sends)
            .await
            .map(drop)
            .map_err(|_| ShardStopped)
    }
}

#[cfg(test)]
mod test {
    use crate::config::TransformContext;
    use std::sync::Arc;

    use indoc::indoc;
    use serde_json::json;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vector_lib::{enrichment::TableRegistry, lookup::owned_value_path};
    use vrl::event_path;
    use vrl::value::Kind;

    use super::*;
    use crate::{
        config::{OutputId, TransformConfig, schema, schema::Definition},
        event::{LogEvent, Value},
        test_util::components::assert_transform_compliance,
        transforms::test::create_topology,
    };

    #[tokio::test]
    async fn reduce_from_condition() {
        let reduce_config = serde_yaml::from_str::<ReduceConfig>(indoc! {"
            group_by:
              - request_id
            ends_when:
              type: vrl
              source: exists(.test_end)
        "})
        .unwrap();

        Box::pin(assert_transform_compliance(async move {
            let input_definition = schema::Definition::default_legacy_namespace()
                .with_event_field(&owned_value_path!("counter"), Kind::integer(), None)
                .with_event_field(&owned_value_path!("request_id"), Kind::bytes(), None)
                .with_event_field(
                    &owned_value_path!("test_end"),
                    Kind::bytes().or_undefined(),
                    None,
                )
                .with_event_field(
                    &owned_value_path!("extra_field"),
                    Kind::bytes().or_undefined(),
                    None,
                );
            let schema_definitions = reduce_config
                .outputs(
                    &TransformContext::default(),
                    &[("test".into(), input_definition)],
                )
                .first()
                .unwrap()
                .schema_definitions(true)
                .clone();

            let new_schema_definition = reduce_config.outputs(
                &TransformContext::default(),
                &[(OutputId::from("in"), Definition::default_legacy_namespace())],
            )[0]
            .clone()
            .log_schema_definitions
            .get(&OutputId::from("in"))
            .unwrap()
            .clone();

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test message 1");
            e_1.insert(event_path!("counter"), 1);
            e_1.insert(event_path!("request_id"), "1");
            let mut metadata_1 = e_1.metadata().clone();
            metadata_1.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_1.set_schema_definition(&Arc::new(new_schema_definition.clone()));

            let mut e_2 = LogEvent::from("test message 2");
            e_2.insert(event_path!("counter"), 2);
            e_2.insert(event_path!("request_id"), "2");
            let mut metadata_2 = e_2.metadata().clone();
            metadata_2.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_2.set_schema_definition(&Arc::new(new_schema_definition.clone()));

            let mut e_3 = LogEvent::from("test message 3");
            e_3.insert(event_path!("counter"), 3);
            e_3.insert(event_path!("request_id"), "1");

            let mut e_4 = LogEvent::from("test message 4");
            e_4.insert(event_path!("counter"), 4);
            e_4.insert(event_path!("request_id"), "1");
            e_4.insert(event_path!("test_end"), "yep");

            let mut e_5 = LogEvent::from("test message 5");
            e_5.insert(event_path!("counter"), 5);
            e_5.insert(event_path!("request_id"), "2");
            e_5.insert(event_path!("extra_field"), "value1");
            e_5.insert(event_path!("test_end"), "yep");

            for event in [e_1.into(), e_2.into(), e_3.into(), e_4.into(), e_5.into()] {
                tx.send(event).await.unwrap();
            }

            let output_1 = out.recv().await.unwrap().into_log();
            assert_eq!(output_1["message"], "test message 1".into());
            assert_eq!(output_1["counter"], Value::from(8));
            assert_eq!(output_1.metadata(), &metadata_1);
            for definition in schema_definitions.values() {
                definition.assert_valid_for_event(&output_1.clone().into());
            }

            let output_2 = out.recv().await.unwrap().into_log();
            assert_eq!(output_2["message"], "test message 2".into());
            assert_eq!(output_2["extra_field"], "value1".into());
            assert_eq!(output_2["counter"], Value::from(7));
            assert_eq!(output_2.metadata(), &metadata_2);
            for definition in schema_definitions.values() {
                definition.assert_valid_for_event(&output_2.clone().into());
            }

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }

    #[tokio::test]
    async fn reduce_merge_strategies() {
        let reduce_config = serde_yaml::from_str::<ReduceConfig>(indoc! {"
            group_by:
              - request_id
            merge_strategies:
              foo: concat
              bar: array
              baz: max
            ends_when:
              type: vrl
              source: exists(.test_end)
        "})
        .unwrap();

        Box::pin(assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);

            let new_schema_definition = reduce_config.outputs(
                &TransformContext::default(),
                &[(OutputId::from("in"), Definition::default_legacy_namespace())],
            )[0]
            .clone()
            .log_schema_definitions
            .get(&OutputId::from("in"))
            .unwrap()
            .clone();

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test message 1");
            e_1.insert(event_path!("foo"), "first foo");
            e_1.insert(event_path!("bar"), "first bar");
            e_1.insert(event_path!("baz"), 2);
            e_1.insert(event_path!("request_id"), "1");
            let mut metadata = e_1.metadata().clone();
            metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata.set_schema_definition(&Arc::new(new_schema_definition.clone()));
            tx.send(e_1.into()).await.unwrap();

            let mut e_2 = LogEvent::from("test message 2");
            e_2.insert(event_path!("foo"), "second foo");
            e_2.insert(event_path!("bar"), 2);
            e_2.insert(event_path!("baz"), "not number");
            e_2.insert(event_path!("request_id"), "1");
            tx.send(e_2.into()).await.unwrap();

            let mut e_3 = LogEvent::from("test message 3");
            e_3.insert(event_path!("foo"), 10);
            e_3.insert(event_path!("bar"), "third bar");
            e_3.insert(event_path!("baz"), 3);
            e_3.insert(event_path!("request_id"), "1");
            e_3.insert(event_path!("test_end"), "yep");
            tx.send(e_3.into()).await.unwrap();

            let output_1 = out.recv().await.unwrap().into_log();
            assert_eq!(output_1["message"], "test message 1".into());
            assert_eq!(output_1["foo"], "first foo second foo".into());
            assert_eq!(
                output_1["bar"],
                Value::Array(vec!["first bar".into(), 2.into(), "third bar".into()]),
            );
            assert_eq!(output_1["baz"], 3.into());
            assert_eq!(output_1.metadata(), &metadata);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }

    #[tokio::test]
    async fn missing_group_by() {
        let reduce_config = serde_yaml::from_str::<ReduceConfig>(indoc! {"
            group_by:
              - request_id
            ends_when:
              type: vrl
              source: exists(.test_end)
        "})
        .unwrap();

        Box::pin(assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);
            let new_schema_definition = reduce_config.outputs(
                &TransformContext::default(),
                &[(OutputId::from("in"), Definition::default_legacy_namespace())],
            )[0]
            .clone()
            .log_schema_definitions
            .get(&OutputId::from("in"))
            .unwrap()
            .clone();

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test message 1");
            e_1.insert(event_path!("counter"), 1);
            e_1.insert(event_path!("request_id"), "1");
            let mut metadata_1 = e_1.metadata().clone();
            metadata_1.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_1.set_schema_definition(&Arc::new(new_schema_definition.clone()));
            tx.send(e_1.into()).await.unwrap();

            let mut e_2 = LogEvent::from("test message 2");
            e_2.insert(event_path!("counter"), 2);
            let mut metadata_2 = e_2.metadata().clone();
            metadata_2.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_2.set_schema_definition(&Arc::new(new_schema_definition));
            tx.send(e_2.into()).await.unwrap();

            let mut e_3 = LogEvent::from("test message 3");
            e_3.insert(event_path!("counter"), 3);
            e_3.insert(event_path!("request_id"), "1");
            tx.send(e_3.into()).await.unwrap();

            let mut e_4 = LogEvent::from("test message 4");
            e_4.insert(event_path!("counter"), 4);
            e_4.insert(event_path!("request_id"), "1");
            e_4.insert(event_path!("test_end"), "yep");
            tx.send(e_4.into()).await.unwrap();

            let mut e_5 = LogEvent::from("test message 5");
            e_5.insert(event_path!("counter"), 5);
            e_5.insert(event_path!("extra_field"), "value1");
            e_5.insert(event_path!("test_end"), "yep");
            tx.send(e_5.into()).await.unwrap();

            let output_1 = out.recv().await.unwrap().into_log();
            assert_eq!(output_1["message"], "test message 1".into());
            assert_eq!(output_1["counter"], Value::from(8));
            assert_eq!(output_1.metadata(), &metadata_1);

            let output_2 = out.recv().await.unwrap().into_log();
            assert_eq!(output_2["message"], "test message 2".into());
            assert_eq!(output_2["extra_field"], "value1".into());
            assert_eq!(output_2["counter"], Value::from(7));
            assert_eq!(output_2.metadata(), &metadata_2);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }

    #[tokio::test]
    async fn max_events_0() {
        let reduce_config = serde_yaml::from_str::<ReduceConfig>(indoc! {"
            group_by:
              - id
            merge_strategies:
              id: retain
              message: array
            max_events: 0
        "});

        match reduce_config {
            Ok(_conf) => unreachable!("max_events=0 should be rejected."),
            Err(err) => assert!(
                err.to_string()
                    .contains("invalid value: integer `0`, expected a nonzero usize")
            ),
        }
    }

    #[tokio::test]
    async fn max_events_1() {
        let reduce_config = serde_yaml::from_str::<ReduceConfig>(indoc! {"
            group_by:
              - id
            merge_strategies:
              id: retain
              message: array
            max_events: 1
        "})
        .unwrap();
        Box::pin(assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test 1");
            e_1.insert(event_path!("id"), "1");

            let mut e_2 = LogEvent::from("test 2");
            e_2.insert(event_path!("id"), "1");

            let mut e_3 = LogEvent::from("test 3");
            e_3.insert(event_path!("id"), "1");

            for event in [e_1.into(), e_2.into(), e_3.into()] {
                tx.send(event).await.unwrap();
            }

            let output_1 = out.recv().await.unwrap().into_log();
            assert_eq!(output_1["message"], vec!["test 1"].into());
            let output_2 = out.recv().await.unwrap().into_log();
            assert_eq!(output_2["message"], vec!["test 2"].into());

            let output_3 = out.recv().await.unwrap().into_log();
            assert_eq!(output_3["message"], vec!["test 3"].into());

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }

    #[tokio::test]
    async fn max_events() {
        let reduce_config = serde_yaml::from_str::<ReduceConfig>(indoc! {"
            group_by:
              - id
            merge_strategies:
              id: retain
              message: array
            max_events: 3
        "})
        .unwrap();

        Box::pin(assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test 1");
            e_1.insert(event_path!("id"), "1");

            let mut e_2 = LogEvent::from("test 2");
            e_2.insert(event_path!("id"), "1");

            let mut e_3 = LogEvent::from("test 3");
            e_3.insert(event_path!("id"), "1");

            let mut e_4 = LogEvent::from("test 4");
            e_4.insert(event_path!("id"), "1");

            let mut e_5 = LogEvent::from("test 5");
            e_5.insert(event_path!("id"), "1");

            let mut e_6 = LogEvent::from("test 6");
            e_6.insert(event_path!("id"), "1");

            for event in [
                e_1.into(),
                e_2.into(),
                e_3.into(),
                e_4.into(),
                e_5.into(),
                e_6.into(),
            ] {
                tx.send(event).await.unwrap();
            }

            let output_1 = out.recv().await.unwrap().into_log();
            assert_eq!(
                output_1["message"],
                vec!["test 1", "test 2", "test 3"].into()
            );

            let output_2 = out.recv().await.unwrap().into_log();
            assert_eq!(
                output_2["message"],
                vec!["test 4", "test 5", "test 6"].into()
            );

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }

    #[tokio::test]
    async fn arrays() {
        let reduce_config = serde_yaml::from_str::<ReduceConfig>(indoc! {"
            group_by:
              - request_id
            merge_strategies:
              foo: array
              bar: concat
            ends_when:
              type: vrl
              source: exists(.test_end)
        "})
        .unwrap();

        Box::pin(assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);

            let new_schema_definition = reduce_config.outputs(
                &TransformContext::default(),
                &[(OutputId::from("in"), Definition::default_legacy_namespace())],
            )[0]
            .clone()
            .log_schema_definitions
            .get(&OutputId::from("in"))
            .unwrap()
            .clone();

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test message 1");
            e_1.insert(event_path!("foo"), json!([1, 3]));
            e_1.insert(event_path!("bar"), json!([1, 3]));
            e_1.insert(event_path!("request_id"), "1");
            let mut metadata_1 = e_1.metadata().clone();
            metadata_1.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_1.set_schema_definition(&Arc::new(new_schema_definition.clone()));

            tx.send(e_1.into()).await.unwrap();

            let mut e_2 = LogEvent::from("test message 2");
            e_2.insert(event_path!("foo"), json!([2, 4]));
            e_2.insert(event_path!("bar"), json!([2, 4]));
            e_2.insert(event_path!("request_id"), "2");
            let mut metadata_2 = e_2.metadata().clone();
            metadata_2.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_2.set_schema_definition(&Arc::new(new_schema_definition));
            tx.send(e_2.into()).await.unwrap();

            let mut e_3 = LogEvent::from("test message 3");
            e_3.insert(event_path!("foo"), json!([5, 7]));
            e_3.insert(event_path!("bar"), json!([5, 7]));
            e_3.insert(event_path!("request_id"), "1");
            tx.send(e_3.into()).await.unwrap();

            let mut e_4 = LogEvent::from("test message 4");
            e_4.insert(event_path!("foo"), json!("done"));
            e_4.insert(event_path!("bar"), json!("done"));
            e_4.insert(event_path!("request_id"), "1");
            e_4.insert(event_path!("test_end"), "yep");
            tx.send(e_4.into()).await.unwrap();

            let mut e_5 = LogEvent::from("test message 5");
            e_5.insert(event_path!("foo"), json!([6, 8]));
            e_5.insert(event_path!("bar"), json!([6, 8]));
            e_5.insert(event_path!("request_id"), "2");
            tx.send(e_5.into()).await.unwrap();

            let mut e_6 = LogEvent::from("test message 6");
            e_6.insert(event_path!("foo"), json!("done"));
            e_6.insert(event_path!("bar"), json!("done"));
            e_6.insert(event_path!("request_id"), "2");
            e_6.insert(event_path!("test_end"), "yep");
            tx.send(e_6.into()).await.unwrap();

            let output_1 = out.recv().await.unwrap().into_log();
            assert_eq!(output_1["foo"], json!([[1, 3], [5, 7], "done"]).into());
            assert_eq!(output_1["bar"], json!([1, 3, 5, 7, "done"]).into());
            assert_eq!(output_1.metadata(), &metadata_1);

            let output_2 = out.recv().await.unwrap().into_log();
            assert_eq!(output_2["foo"], json!([[2, 4], [6, 8], "done"]).into());
            assert_eq!(output_2["bar"], json!([2, 4, 6, 8, "done"]).into());
            assert_eq!(output_2.metadata(), &metadata_2);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }

    #[tokio::test]
    async fn sharded_keeps_arrival_order_within_group() {
        let reduce_config = serde_yaml::from_str::<ReduceConfig>(indoc! {"
            group_by:
              - request_id
            concurrency: 4
            merge_strategies:
              foo: array
            ends_when:
              type: vrl
              source: exists(.test_end)
        "})
        .unwrap();

        Box::pin(assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            for (foo, request_id, end) in [
                (1, "1", false),
                (2, "2", false),
                (3, "1", false),
                (4, "2", false),
                (5, "1", true),
                (6, "2", true),
            ] {
                let mut event = LogEvent::from(format!("test message {foo}"));
                event.insert(event_path!("foo"), foo);
                event.insert(event_path!("request_id"), request_id);
                if end {
                    event.insert(event_path!("test_end"), "yep");
                }
                tx.send(event.into()).await.unwrap();
            }

            // Groups may reach different shards, so either group can come out first.
            let mut outputs = [
                out.recv().await.unwrap().into_log(),
                out.recv().await.unwrap().into_log(),
            ];
            outputs.sort_by_key(|output| output["request_id"].to_string());

            assert_eq!(outputs[0]["message"], "test message 1".into());
            assert_eq!(outputs[0]["foo"], json!([1, 3, 5]).into());
            assert_eq!(outputs[1]["message"], "test message 2".into());
            assert_eq!(outputs[1]["foo"], json!([2, 4, 6]).into());

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }

    fn keyed_event(key: i64, seq: i64) -> Event {
        let mut log = LogEvent::default();
        log.insert(event_path!("key"), key);
        log.insert(event_path!("seq"), seq);
        log.into()
    }

    type ShardChannels = (
        Vec<mpsc::Sender<Vec<Event>>>,
        Vec<mpsc::Receiver<Vec<Event>>>,
    );

    fn shard_channels(shards: usize) -> ShardChannels {
        (0..shards)
            .map(|_| mpsc::channel(SHARD_QUEUE_BATCHES))
            .unzip()
    }

    #[tokio::test]
    async fn router_sends_full_batches_whatever_the_shard_count() {
        const KEYS: i64 = 64;
        let (senders, receivers) = shard_channels(8);
        // The input is always ready, and holds several full batches for every shard.
        let input = stream::iter((0..KEYS * 50).map(|seq| keyed_event(seq % KEYS, seq))).boxed();

        let ((), received) = tokio::join!(
            route(vec!["key".to_string()], senders, Duration::MAX, input),
            future::join_all(
                receivers
                    .into_iter()
                    .map(|rx| ReceiverStream::new(rx).collect::<Vec<_>>())
            ),
        );

        let mut shard_by_key = HashMap::new();
        let mut last_seq_by_key = HashMap::new();
        for (shard, batches) in received.iter().enumerate() {
            let Some((last, full)) = batches.split_last() else {
                continue;
            };
            // Only the batch sent when the input ended can be partial.
            assert!(full.iter().all(|batch| batch.len() == BATCH_EVENTS));
            assert!(last.len() <= BATCH_EVENTS);

            for event in batches.iter().flatten() {
                let key = event.as_log()["key"].as_integer().unwrap();
                let seq = event.as_log()["seq"].as_integer().unwrap();
                // A group's events all reach the same shard, in arrival order.
                assert_eq!(*shard_by_key.entry(key).or_insert(shard), shard);
                if let Some(previous) = last_seq_by_key.insert(key, seq) {
                    assert!(previous < seq);
                }
            }
        }
        assert_eq!(i64::try_from(last_seq_by_key.len()).unwrap(), KEYS);
    }

    #[tokio::test]
    async fn router_sends_partial_batches_while_input_waits() {
        let (input_tx, input_rx) = mpsc::channel(10);
        let (senders, receivers) = shard_channels(4);
        let router = tokio::spawn(route(
            vec!["key".to_string()],
            senders,
            Duration::MAX,
            ReceiverStream::new(input_rx).boxed(),
        ));

        for seq in 0..3 {
            input_tx.send(keyed_event(seq, seq)).await.unwrap();
        }

        // The input stays open with nothing ready: the router must not wait to fill the batches.
        let mut received = stream::select_all(receivers.into_iter().map(ReceiverStream::new))
            .flat_map(stream::iter);
        for _ in 0..3 {
            tokio::time::timeout(Duration::from_secs(10), received.next())
                .await
                .expect("the router held an event back")
                .unwrap();
        }

        drop(input_tx);
        router.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn partial_batches_wait_at_most_the_max_delay() {
        async fn fill_batch(batches: &mut ShardBatches, shard: usize) {
            for _ in 0..BATCH_EVENTS {
                batches.push(shard, keyed_event(0, 0)).await.unwrap();
            }
        }

        let max_delay = Duration::from_millis(5);
        let (senders, mut receivers) = shard_channels(2);
        let mut batches = ShardBatches::new(senders, max_delay);
        batches.push(1, keyed_event(1, 0)).await.unwrap();

        // Under sustained load, shard 0 fills a batch before the delay: shard 1's event waits.
        fill_batch(&mut batches, 0).await;
        assert_eq!(receivers[0].try_recv().unwrap().len(), BATCH_EVENTS);
        assert!(receivers[1].try_recv().is_err());

        // Once it has waited for the delay, the next full batch sends it too.
        tokio::time::advance(max_delay).await;
        fill_batch(&mut batches, 0).await;
        assert_eq!(receivers[0].try_recv().unwrap().len(), BATCH_EVENTS);
        assert_eq!(receivers[1].try_recv().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn strategy_path_with_nested_fields() {
        let reduce_config = serde_yaml::from_str::<ReduceConfig>(indoc! {"
            group_by:
              - id
            merge_strategies:
              id: discard
              message.a.b: array
            ends_when:
              type: vrl
              source: exists(.test_end)
        "})
        .unwrap();

        Box::pin(assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let e_1 = LogEvent::from(Value::from(btreemap! {
                "id" => 777,
                "message" => btreemap! {
                    "a" => btreemap! {
                        "b" => vec![1,2],
                        "num" => 1,
                    },
                },
                "arr" => vec![btreemap! { "a" => 1 }, btreemap! { "b" => 1 }]
            }));
            let mut metadata_1 = e_1.metadata().clone();
            metadata_1.set_upstream_id(Arc::new(OutputId::from("reduce")));

            tx.send(e_1.into()).await.unwrap();

            let e_2 = LogEvent::from(Value::from(btreemap! {
                "id" => 777,
                "message" => btreemap! {
                        "a" => btreemap! {
                            "b" => vec![3,4],
                            "num" => 2,
                        },
                },
                 "arr" => vec![btreemap! { "a" => 2 }, btreemap! { "b" => 2 }],
                "test_end" => "done",
            }));
            tx.send(e_2.into()).await.unwrap();

            let mut output = out.recv().await.unwrap().into_log();

            // Remove timestamp fields which were automatically added.
            output.remove_timestamp();
            output.remove(event_path!("timestamp_end"));

            assert_eq!(
                *output.value(),
                btreemap! {
                    "id" => 777,
                    "message" => btreemap! {
                        "a" => btreemap! {
                            "b" => vec![vec![1, 2], vec![3,4]],
                            "num" => 3,
                        },
                    },
                    "arr" => vec![btreemap! { "a" => 1 }, btreemap! { "b" => 1 }],
                    "test_end" => "done",
                }
                .into()
            );

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }

    #[test]
    fn invalid_merge_strategies_containing_indexes() {
        let config = serde_yaml::from_str::<ReduceConfig>(indoc! {"
            group_by:
              - id
            merge_strategies:
              id: discard
              'nested.msg[0]': array
        "})
        .unwrap();
        let error = Reduce::new(
            &config,
            &TableRegistry::default(),
            &MetricsStorage::default(),
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Merge strategies with indexes are currently not supported. Path: `nested.msg[0]`"
        );
    }

    #[tokio::test]
    async fn merge_objects_in_array() {
        let config = serde_yaml::from_str::<ReduceConfig>(indoc! {r#"
            group_by:
              - id
            merge_strategies:
              events: array
              '"a-b"': retain
              another: discard
            ends_when:
              type: vrl
              source: exists(.test_end)
        "#})
        .unwrap();

        Box::pin(assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            let v_1 = Value::from(btreemap! {
                "attrs" => btreemap! {
                    "nested.msg" => "foo",
                },
                "sev" => 2,
            });
            let mut e_1 = LogEvent::from(Value::from(
                btreemap! {"id" => 777, "another" => btreemap!{ "a" => 1}},
            ));
            e_1.insert(event_path!("events"), v_1.clone());
            e_1.insert(&vrl::path::parse_target_path("\"a-b\"").unwrap(), 2);
            tx.send(e_1.into()).await.unwrap();

            let v_2 = Value::from(btreemap! {
                "attrs" => btreemap! {
                    "nested.msg" => "bar",
                },
                "sev" => 3,
            });
            let mut e_2 = LogEvent::from(Value::from(
                btreemap! {"id" => 777, "test_end" => "done", "another" => btreemap!{ "b" => 2}},
            ));
            e_2.insert(event_path!("events"), v_2.clone());
            e_2.insert(&vrl::path::parse_target_path("\"a-b\"").unwrap(), 2);
            tx.send(e_2.into()).await.unwrap();

            let output = out.recv().await.unwrap().into_log();
            let expected_value = Value::from(btreemap! {
                "id" => 1554,
                "events" => vec![v_1, v_2],
                "another" => btreemap!{ "a" => 1},
                "a-b" => 2,
                "test_end" => "done"
            });
            assert_eq!(*output.value(), expected_value);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }

    #[tokio::test]
    async fn merged_quoted_path() {
        let config = serde_yaml::from_str::<ReduceConfig>(indoc! {"
            ends_when:
              type: vrl
              source: exists(.test_end)
        "})
        .unwrap();

        Box::pin(assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            let e_1 = LogEvent::from(Value::from(btreemap! {"a b" => 1}));
            tx.send(e_1.into()).await.unwrap();

            let e_2 = LogEvent::from(Value::from(btreemap! {"a b" => 2, "test_end" => "done"}));
            tx.send(e_2.into()).await.unwrap();

            let output = out.recv().await.unwrap().into_log();
            let expected_value = Value::from(btreemap! {
                "a b" => 3,
                "test_end" => "done"
            });
            assert_eq!(*output.value(), expected_value);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }
}

use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    pin::Pin,
    time::{Duration, Instant},
};

use futures::Stream;
use indexmap::IndexMap;
use vector_lib::stream::expiration_map::{Emitter, map_with_expiration};
use vector_vrl_metrics::MetricsStorage;
use vrl::{
    path::{OwnedSegment, OwnedTargetPath, PathPrefix, parse_target_path},
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

/// The merge strategy paths, stored so that a field path can be checked against them without
/// allocating.
///
/// A field is covered when its path, or any prefix of its path, is a merge strategy key. The
/// strategy loop in [`ReduceState::add_event`] already merges covered fields, so the per-field
/// loop must skip them.
#[derive(Clone, Debug)]
struct StrategyPrefixes {
    keys: HashSet<Vec<OwnedSegment>>,
    max_depth: usize,
}

impl StrategyPrefixes {
    fn new(strategies: &IndexMap<OwnedTargetPath, MergeStrategy>) -> Self {
        // The per-field loop only visits event fields, so metadata strategies never cover them.
        let keys: HashSet<Vec<OwnedSegment>> = strategies
            .keys()
            .filter(|path| path.prefix == PathPrefix::Event && !path.path.segments.is_empty())
            .map(|path| path.path.segments.clone())
            .collect();
        let max_depth = keys.iter().map(Vec::len).max().unwrap_or(0);
        Self { keys, max_depth }
    }

    /// Returns true if `path` or one of its prefixes is a merge strategy key.
    fn covers(&self, path: &OwnedTargetPath) -> bool {
        let segments = &path.path.segments;
        (1..=segments.len().min(self.max_depth)).any(|depth| self.keys.contains(&segments[..depth]))
    }
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

    fn add_event(
        &mut self,
        e: &LogEvent,
        strategies: &IndexMap<OwnedTargetPath, MergeStrategy>,
        strategy_prefixes: &StrategyPrefixes,
    ) {
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
                        emit!(ReduceAddEventError { error, path });
                        continue;
                    }
                };
                if strategy_prefixes.covers(&parsed_path) {
                    continue;
                }

                let maybe_strategy = strategies.get(&parsed_path);
                match self.fields.entry(parsed_path) {
                    Entry::Vacant(entry) => {
                        if let Some(strategy) = maybe_strategy {
                            match get_value_merger(value.clone(), strategy) {
                                Ok(m) => {
                                    entry.insert(m);
                                }
                                Err(error) => {
                                    warn!(message = "Failed to merge value.", %error);
                                }
                            }
                        } else {
                            entry.insert(value.clone().into());
                        }
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
    strategy_prefixes: StrategyPrefixes,
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

        let merge_strategies: IndexMap<OwnedTargetPath, MergeStrategy> = config
            .merge_strategies
            .iter()
            .filter_map(|(path, strategy)| {
                // TODO Invalid paths are ignored to preserve backwards compatibility.
                //      Merge strategy paths should ideally be [`lookup_v2::ConfigTargetPath`]
                //      which means an invalid path would result in an configuration error.
                let parsed_path = parse_target_path(path).ok();
                if parsed_path.is_none() {
                    warn!(message = "Ignoring strategy with invalid path.", %path);
                }
                parsed_path.map(|path| (path, strategy.clone()))
            })
            .collect();
        let strategy_prefixes = StrategyPrefixes::new(&merge_strategies);

        Ok(Reduce {
            expire_after: config.expire_after_ms,
            flush_period: config.flush_period_ms,
            end_every_period: config.end_every_period_ms,
            group_by,
            merge_strategies,
            strategy_prefixes,
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
                emit!(ReduceStaleEventFlushed);
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
                state.add_event(event, &self.merge_strategies, &self.strategy_prefixes);
                entry.insert(state);
            }
            Entry::Occupied(mut entry) => {
                entry
                    .get_mut()
                    .add_event(event, &self.merge_strategies, &self.strategy_prefixes);
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
            emitter.emit(
                if let Some(mut state) = self.merge_states.remove(&discriminant) {
                    state.add_event(&event, &self.merge_strategies, &self.strategy_prefixes);
                    state.flush().into()
                } else {
                    let mut state = ReduceState::new();
                    state.add_event(&event, &self.merge_strategies, &self.strategy_prefixes);
                    state.flush().into()
                },
            );
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

    #[test]
    fn strategy_prefixes_cover_fields_under_strategy_paths() {
        let strategies: IndexMap<OwnedTargetPath, MergeStrategy> =
            [".a", ".x.y", "\"a-b\"", "parent.\"c-d\"", "%meta.field"]
                .into_iter()
                .map(|path| (parse_target_path(path).unwrap(), MergeStrategy::Discard))
                .collect();
        let prefixes = StrategyPrefixes::new(&strategies);

        for (path, expected) in [
            (".a", true),
            (".a.b", true),
            (".ab", false),
            (".b", false),
            (".x", false),
            (".x.y", true),
            (".x.y.z", true),
            (".x.z", false),
            // Field names that are quoted when a path is printed.
            ("\"a-b\"", true),
            ("\"a-b\".c", true),
            ("parent", false),
            ("parent.\"c-d\"", true),
            ("parent.c", false),
            // Metadata strategies do not cover event fields.
            (".meta.field", false),
        ] {
            assert_eq!(
                prefixes.covers(&parse_target_path(path).unwrap()),
                expected,
                "path: {path}"
            );
        }

        let empty = StrategyPrefixes::new(&IndexMap::new());
        assert!(!empty.covers(&parse_target_path(".a").unwrap()));
    }

    #[tokio::test]
    async fn merge_strategies_on_field_names_that_need_quoting() {
        let config = serde_yaml::from_str::<ReduceConfig>(indoc! {r#"
            group_by:
              - id
            merge_strategies:
              plain: sum
              '"a-b"': sum
              'parent."c-d"': sum
              '"e-f"': discard
            ends_when:
              type: vrl
              source: exists(.test_end)
        "#})
        .unwrap();

        Box::pin(assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            let e_1 = LogEvent::from(Value::from(btreemap! {
                "id" => "x",
                "plain" => 2,
                "a-b" => 2,
                "parent" => btreemap! { "c-d" => 2 },
                "e-f" => btreemap! { "g" => 1 },
            }));
            tx.send(e_1.into()).await.unwrap();

            let e_2 = LogEvent::from(Value::from(btreemap! {
                "id" => "x",
                "plain" => 3,
                "a-b" => 3,
                "parent" => btreemap! { "c-d" => 3 },
                "e-f" => btreemap! { "g" => 2 },
                "test_end" => "done",
            }));
            tx.send(e_2.into()).await.unwrap();

            let output = out.recv().await.unwrap().into_log();
            let expected_value = Value::from(btreemap! {
                "id" => "x",
                "plain" => 5,
                "a-b" => 5,
                "parent" => btreemap! { "c-d" => 5 },
                "e-f" => btreemap! { "g" => 1 },
                "test_end" => "done",
            });
            assert_eq!(*output.value(), expected_value);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }
}

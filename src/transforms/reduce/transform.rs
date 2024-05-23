use std::collections::{hash_map, BTreeMap, HashMap};
use std::pin::Pin;
use std::time::{Duration, Instant};

use futures::Stream;
use indexmap::IndexMap;
use vrl::core::Value;
use vrl::prelude::KeyString;

use vector_lib::stream::expiration_map::{map_with_expiration, Emitter};

use crate::transforms::reduce::merge_strategy::{
    get_value_merger, MergeStrategy, ReduceValueMerger,
};
use crate::{
    conditions::Condition,
    event::{discriminant::Discriminant, Event, EventMetadata, LogEvent},
    internal_events::ReduceStaleEventFlushed,
    transforms::{reduce::config::ReduceConfig, TaskTransform},
};

#[derive(Debug)]
struct ReduceState {
    events: usize,
    fields: HashMap<KeyString, Box<dyn ReduceValueMerger>>,
    stale_since: Instant,
    metadata: EventMetadata,
}

impl ReduceState {
    fn new() -> Self {
        let fields = HashMap::new();
        let metadata = EventMetadata::default();

        Self {
            events: 0,
            stale_since: Instant::now(),
            fields,
            metadata,
        }
    }

    fn add_event(&mut self, e: LogEvent, strategies: &IndexMap<KeyString, MergeStrategy>) {
        let (value, metadata) = e.into_parts();
        self.metadata.merge(metadata);

        let fields = if let Value::Object(fields) = value {
            fields
        } else {
            BTreeMap::new()
        };

        for (k, v) in fields.into_iter() {
            let strategy = strategies.get(&k);
            match self.fields.entry(k) {
                hash_map::Entry::Vacant(entry) => {
                    if let Some(strat) = strategy {
                        match get_value_merger(v, strat) {
                            Ok(m) => {
                                entry.insert(m);
                            }
                            Err(error) => {
                                warn!(message = "Failed to merge value.", %error);
                            }
                        }
                    } else {
                        entry.insert(v.clone().into());
                    }
                }
                hash_map::Entry::Occupied(mut entry) => {
                    if let Err(error) = entry.get_mut().add(v.clone()) {
                        warn!(message = "Failed to merge value.", %error);
                    }
                }
            }
        }
        self.events += 1;
        self.stale_since = Instant::now();
    }

    fn flush(mut self) -> LogEvent {
        let mut event = LogEvent::new_with_metadata(self.metadata);
        for (k, v) in self.fields.drain() {
            if let Err(error) = v.insert_into(k, &mut event) {
                warn!(message = "Failed to merge values for field.", %error);
            }
        }
        self.events = 0;
        event
    }
}

pub struct Reduce {
    expire_after: Duration,
    flush_period: Duration,
    group_by: Vec<String>,
    merge_strategies: IndexMap<KeyString, MergeStrategy>,
    reduce_merge_states: HashMap<Discriminant, ReduceState>,
    ends_when: Option<Condition>,
    starts_when: Option<Condition>,
    max_events: Option<usize>,
}

impl Reduce {
    pub fn new(
        config: &ReduceConfig,
        enrichment_tables: &vector_lib::enrichment::TableRegistry,
    ) -> crate::Result<Self> {
        if config.ends_when.is_some() && config.starts_when.is_some() {
            return Err("only one of `ends_when` and `starts_when` can be provided".into());
        }

        let ends_when = config
            .ends_when
            .as_ref()
            .map(|c| c.build(enrichment_tables))
            .transpose()?;
        let starts_when = config
            .starts_when
            .as_ref()
            .map(|c| c.build(enrichment_tables))
            .transpose()?;
        let group_by = config.group_by.clone().into_iter().collect();
        let max_events = config.max_events.map(|max| max.into());

        Ok(Reduce {
            expire_after: config.expire_after_ms,
            flush_period: config.flush_period_ms,
            group_by,
            merge_strategies: config.merge_strategies.clone(),
            reduce_merge_states: HashMap::new(),
            ends_when,
            starts_when,
            max_events,
        })
    }

    fn flush_into(&mut self, emitter: &mut Emitter<Event>) {
        let mut flush_discriminants = Vec::new();
        let now = Instant::now();
        for (k, t) in &self.reduce_merge_states {
            if (now - t.stale_since) >= self.expire_after {
                flush_discriminants.push(k.clone());
            }
        }
        for k in &flush_discriminants {
            if let Some(t) = self.reduce_merge_states.remove(k) {
                emit!(ReduceStaleEventFlushed);
                emitter.emit(Event::from(t.flush()));
            }
        }
    }

    fn flush_all_into(&mut self, emitter: &mut Emitter<Event>) {
        self.reduce_merge_states
            .drain()
            .for_each(|(_, s)| emitter.emit(Event::from(s.flush())));
    }

    fn push_or_new_reduce_state(&mut self, event: LogEvent, discriminant: Discriminant) {
        match self.reduce_merge_states.entry(discriminant) {
            hash_map::Entry::Vacant(entry) => {
                let mut state = ReduceState::new();
                state.add_event(event, &self.merge_strategies);
                entry.insert(state);
            }
            hash_map::Entry::Occupied(mut entry) => {
                entry.get_mut().add_event(event, &self.merge_strategies);
            }
        }
    }

    pub(crate) fn transform_one(&mut self, emitter: &mut Emitter<Event>, event: Event) {
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
            } else if let Some(entry) = self.reduce_merge_states.get(&discriminant) {
                // The current event will finish this set
                if entry.events + 1 == max_events {
                    ends_here = true;
                }
            }
        }

        if starts_here {
            if let Some(state) = self.reduce_merge_states.remove(&discriminant) {
                emitter.emit(state.flush().into());
            }

            self.push_or_new_reduce_state(event, discriminant)
        } else if ends_here {
            emitter.emit(match self.reduce_merge_states.remove(&discriminant) {
                Some(mut state) => {
                    state.add_event(event, &self.merge_strategies);
                    state.flush().into()
                }
                None => {
                    let mut state = ReduceState::new();
                    state.add_event(event, &self.merge_strategies);
                    state.flush().into()
                }
            })
        } else {
            self.push_or_new_reduce_state(event, discriminant)
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
        let flush_period = self.flush_period;

        Box::pin(map_with_expiration(
            self,
            input_rx,
            flush_period,
            |me: &mut Box<Reduce>, event, emitter: &mut Emitter<Event>| {
                // called for each event
                me.transform_one(emitter, event);
            },
            |me: &mut Box<Reduce>, emitter: &mut Emitter<Event>| {
                // called periodically to check for expired events
                me.flush_into(emitter);
            },
            |me: &mut Box<Reduce>, emitter: &mut Emitter<Event>| {
                // called when the input stream ends
                me.flush_all_into(emitter);
            },
        ))
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vrl::value::Kind;

    use vector_lib::enrichment::TableRegistry;
    use vector_lib::lookup::owned_value_path;

    use crate::config::schema::Definition;
    use crate::config::{schema, LogNamespace, OutputId, TransformConfig};
    use crate::event::{LogEvent, Value};
    use crate::test_util::components::assert_transform_compliance;
    use crate::transforms::test::create_topology;

    use super::*;

    #[tokio::test]
    async fn reduce_from_condition() {
        let reduce_config = toml::from_str::<ReduceConfig>(
            r#"
group_by = [ "request_id" ]

[ends_when]
  type = "vrl"
  source = "exists(.test_end)"
"#,
        )
        .unwrap();

        assert_transform_compliance(async move {
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
                    vector_lib::enrichment::TableRegistry::default(),
                    &[("test".into(), input_definition)],
                    LogNamespace::Legacy,
                )
                .first()
                .unwrap()
                .schema_definitions(true)
                .clone();

            let new_schema_definition = reduce_config.outputs(
                TableRegistry::default(),
                &[(OutputId::from("in"), Definition::default_legacy_namespace())],
                LogNamespace::Legacy,
            )[0]
            .clone()
            .log_schema_definitions
            .get(&OutputId::from("in"))
            .unwrap()
            .clone();

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test message 1");
            e_1.insert("counter", 1);
            e_1.insert("request_id", "1");
            let mut metadata_1 = e_1.metadata().clone();
            metadata_1.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_1.set_schema_definition(&Arc::new(new_schema_definition.clone()));

            let mut e_2 = LogEvent::from("test message 2");
            e_2.insert("counter", 2);
            e_2.insert("request_id", "2");
            let mut metadata_2 = e_2.metadata().clone();
            metadata_2.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_2.set_schema_definition(&Arc::new(new_schema_definition.clone()));

            let mut e_3 = LogEvent::from("test message 3");
            e_3.insert("counter", 3);
            e_3.insert("request_id", "1");

            let mut e_4 = LogEvent::from("test message 4");
            e_4.insert("counter", 4);
            e_4.insert("request_id", "1");
            e_4.insert("test_end", "yep");

            let mut e_5 = LogEvent::from("test message 5");
            e_5.insert("counter", 5);
            e_5.insert("request_id", "2");
            e_5.insert("extra_field", "value1");
            e_5.insert("test_end", "yep");

            for event in vec![e_1.into(), e_2.into(), e_3.into(), e_4.into(), e_5.into()] {
                tx.send(event).await.unwrap();
            }

            let output_1 = out.recv().await.unwrap().into_log();
            assert_eq!(output_1["message"], "test message 1".into());
            assert_eq!(output_1["counter"], Value::from(8));
            assert_eq!(output_1.metadata(), &metadata_1);
            schema_definitions
                .values()
                .for_each(|definition| definition.assert_valid_for_event(&output_1.clone().into()));

            let output_2 = out.recv().await.unwrap().into_log();
            assert_eq!(output_2["message"], "test message 2".into());
            assert_eq!(output_2["extra_field"], "value1".into());
            assert_eq!(output_2["counter"], Value::from(7));
            assert_eq!(output_2.metadata(), &metadata_2);
            schema_definitions
                .values()
                .for_each(|definition| definition.assert_valid_for_event(&output_2.clone().into()));

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn reduce_merge_strategies() {
        let reduce_config = toml::from_str::<ReduceConfig>(
            r#"
group_by = [ "request_id" ]

merge_strategies.foo = "concat"
merge_strategies.bar = "array"
merge_strategies.baz = "max"

[ends_when]
  type = "vrl"
  source = "exists(.test_end)"
"#,
        )
        .unwrap();

        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);

            let new_schema_definition = reduce_config.outputs(
                TableRegistry::default(),
                &[(OutputId::from("in"), Definition::default_legacy_namespace())],
                LogNamespace::Legacy,
            )[0]
            .clone()
            .log_schema_definitions
            .get(&OutputId::from("in"))
            .unwrap()
            .clone();

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test message 1");
            e_1.insert("foo", "first foo");
            e_1.insert("bar", "first bar");
            e_1.insert("baz", 2);
            e_1.insert("request_id", "1");
            let mut metadata = e_1.metadata().clone();
            metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata.set_schema_definition(&Arc::new(new_schema_definition.clone()));
            tx.send(e_1.into()).await.unwrap();

            let mut e_2 = LogEvent::from("test message 2");
            e_2.insert("foo", "second foo");
            e_2.insert("bar", 2);
            e_2.insert("baz", "not number");
            e_2.insert("request_id", "1");
            tx.send(e_2.into()).await.unwrap();

            let mut e_3 = LogEvent::from("test message 3");
            e_3.insert("foo", 10);
            e_3.insert("bar", "third bar");
            e_3.insert("baz", 3);
            e_3.insert("request_id", "1");
            e_3.insert("test_end", "yep");
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
        })
        .await;
    }

    #[tokio::test]
    async fn missing_group_by() {
        let reduce_config = toml::from_str::<ReduceConfig>(
            r#"
group_by = [ "request_id" ]

[ends_when]
  type = "vrl"
  source = "exists(.test_end)"
"#,
        )
        .unwrap();

        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);
            let new_schema_definition = reduce_config.outputs(
                TableRegistry::default(),
                &[(OutputId::from("in"), Definition::default_legacy_namespace())],
                LogNamespace::Legacy,
            )[0]
            .clone()
            .log_schema_definitions
            .get(&OutputId::from("in"))
            .unwrap()
            .clone();

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test message 1");
            e_1.insert("counter", 1);
            e_1.insert("request_id", "1");
            let mut metadata_1 = e_1.metadata().clone();
            metadata_1.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_1.set_schema_definition(&Arc::new(new_schema_definition.clone()));
            tx.send(e_1.into()).await.unwrap();

            let mut e_2 = LogEvent::from("test message 2");
            e_2.insert("counter", 2);
            let mut metadata_2 = e_2.metadata().clone();
            metadata_2.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_2.set_schema_definition(&Arc::new(new_schema_definition));
            tx.send(e_2.into()).await.unwrap();

            let mut e_3 = LogEvent::from("test message 3");
            e_3.insert("counter", 3);
            e_3.insert("request_id", "1");
            tx.send(e_3.into()).await.unwrap();

            let mut e_4 = LogEvent::from("test message 4");
            e_4.insert("counter", 4);
            e_4.insert("request_id", "1");
            e_4.insert("test_end", "yep");
            tx.send(e_4.into()).await.unwrap();

            let mut e_5 = LogEvent::from("test message 5");
            e_5.insert("counter", 5);
            e_5.insert("extra_field", "value1");
            e_5.insert("test_end", "yep");
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
        })
        .await;
    }

    #[tokio::test]
    async fn max_events_0() {
        let reduce_config = toml::from_str::<ReduceConfig>(
            r#"
group_by = [ "id" ]
merge_strategies.id = "retain"
merge_strategies.message = "array"
max_events = 0
            "#,
        );

        match reduce_config {
            Ok(_conf) => unreachable!("max_events=0 should be rejected."),
            Err(err) => assert!(err
                .to_string()
                .contains("invalid value: integer `0`, expected a nonzero usize")),
        }
    }

    #[tokio::test]
    async fn max_events_1() {
        let reduce_config = toml::from_str::<ReduceConfig>(
            r#"
group_by = [ "id" ]
merge_strategies.id = "retain"
merge_strategies.message = "array"
max_events = 1
            "#,
        )
        .unwrap();
        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test 1");
            e_1.insert("id", "1");

            let mut e_2 = LogEvent::from("test 2");
            e_2.insert("id", "1");

            let mut e_3 = LogEvent::from("test 3");
            e_3.insert("id", "1");

            for event in vec![e_1.into(), e_2.into(), e_3.into()] {
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
        })
        .await;
    }

    #[tokio::test]
    async fn max_events() {
        let reduce_config = toml::from_str::<ReduceConfig>(
            r#"
group_by = [ "id" ]
merge_strategies.id = "retain"
merge_strategies.message = "array"
max_events = 3
            "#,
        )
        .unwrap();

        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test 1");
            e_1.insert("id", "1");

            let mut e_2 = LogEvent::from("test 2");
            e_2.insert("id", "1");

            let mut e_3 = LogEvent::from("test 3");
            e_3.insert("id", "1");

            let mut e_4 = LogEvent::from("test 4");
            e_4.insert("id", "1");

            let mut e_5 = LogEvent::from("test 5");
            e_5.insert("id", "1");

            let mut e_6 = LogEvent::from("test 6");
            e_6.insert("id", "1");

            for event in vec![
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
        })
        .await
    }

    #[tokio::test]
    async fn arrays() {
        let reduce_config = toml::from_str::<ReduceConfig>(
            r#"
group_by = [ "request_id" ]

merge_strategies.foo = "array"
merge_strategies.bar = "concat"

[ends_when]
  type = "vrl"
  source = "exists(.test_end)"
"#,
        )
        .unwrap();

        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);

            let new_schema_definition = reduce_config.outputs(
                TableRegistry::default(),
                &[(OutputId::from("in"), Definition::default_legacy_namespace())],
                LogNamespace::Legacy,
            )[0]
            .clone()
            .log_schema_definitions
            .get(&OutputId::from("in"))
            .unwrap()
            .clone();

            let (topology, mut out) = create_topology(ReceiverStream::new(rx), reduce_config).await;

            let mut e_1 = LogEvent::from("test message 1");
            e_1.insert("foo", json!([1, 3]));
            e_1.insert("bar", json!([1, 3]));
            e_1.insert("request_id", "1");
            let mut metadata_1 = e_1.metadata().clone();
            metadata_1.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_1.set_schema_definition(&Arc::new(new_schema_definition.clone()));

            tx.send(e_1.into()).await.unwrap();

            let mut e_2 = LogEvent::from("test message 2");
            e_2.insert("foo", json!([2, 4]));
            e_2.insert("bar", json!([2, 4]));
            e_2.insert("request_id", "2");
            let mut metadata_2 = e_2.metadata().clone();
            metadata_2.set_upstream_id(Arc::new(OutputId::from("transform")));
            metadata_2.set_schema_definition(&Arc::new(new_schema_definition));
            tx.send(e_2.into()).await.unwrap();

            let mut e_3 = LogEvent::from("test message 3");
            e_3.insert("foo", json!([5, 7]));
            e_3.insert("bar", json!([5, 7]));
            e_3.insert("request_id", "1");
            tx.send(e_3.into()).await.unwrap();

            let mut e_4 = LogEvent::from("test message 4");
            e_4.insert("foo", json!("done"));
            e_4.insert("bar", json!("done"));
            e_4.insert("request_id", "1");
            e_4.insert("test_end", "yep");
            tx.send(e_4.into()).await.unwrap();

            let mut e_5 = LogEvent::from("test message 5");
            e_5.insert("foo", json!([6, 8]));
            e_5.insert("bar", json!([6, 8]));
            e_5.insert("request_id", "2");
            tx.send(e_5.into()).await.unwrap();

            let mut e_6 = LogEvent::from("test message 6");
            e_6.insert("foo", json!("done"));
            e_6.insert("bar", json!("done"));
            e_6.insert("request_id", "2");
            e_6.insert("test_end", "yep");
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
        })
        .await;
    }
}

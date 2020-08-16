use super::Transform;
use crate::{
    conditions::{AnyCondition, Condition},
    config::{DataType, TransformConfig, TransformContext, TransformDescription},
    event::discriminant::Discriminant,
    event::{Event, LogEvent},
};
use async_stream::stream;
use futures::{
    compat::{Compat, Compat01As03},
    stream, StreamExt,
};
use futures01::Stream as Stream01;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map, HashMap};
use std::time::{Duration, Instant};
use string_cache::DefaultAtom as Atom;

mod merge_strategy;

use merge_strategy::*;

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields, default)]
pub struct ReduceConfig {
    pub expire_after_ms: Option<u64>,

    pub flush_period_ms: Option<u64>,

    /// An ordered list of fields to distinguish reduces by. Each
    /// reduce has a separate event merging state.
    #[serde(default)]
    pub identifier_fields: Vec<String>,

    #[serde(default)]
    pub merge_strategies: IndexMap<String, MergeStrategy>,

    /// An optional condition that determines when an event is the end of a
    /// reduce.
    pub ends_when: Option<AnyCondition>,
}

inventory::submit! {
    TransformDescription::new::<ReduceConfig>("reduce")
}

#[typetag::serde(name = "reduce")]
impl TransformConfig for ReduceConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let t = Reduce::new(self)?;
        Ok(Box::new(t))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "reduce"
    }
}

#[derive(Debug)]
struct ReduceState {
    fields: HashMap<String, Box<dyn ReduceValueMerger>>,
    stale_since: Instant,
}

impl ReduceState {
    fn new(e: LogEvent, strategies: &IndexMap<String, MergeStrategy>) -> Self {
        Self {
            stale_since: Instant::now(),
            fields: e
                .into_iter()
                .filter_map(|(k, v)| {
                    if let Some(strat) = strategies.get(&k) {
                        match get_value_merger(v, strat) {
                            Ok(m) => Some((k, m)),
                            Err(err) => {
                                warn!("failed to create merger for field '{}': {}", k, err);
                                None
                            }
                        }
                    } else {
                        Some((k, v.into()))
                    }
                })
                .collect(),
        }
    }

    fn add_event(&mut self, e: LogEvent, strategies: &IndexMap<String, MergeStrategy>) {
        for (k, v) in e.into_iter() {
            let strategy = strategies.get(&k);
            match self.fields.entry(k) {
                hash_map::Entry::Vacant(entry) => {
                    if let Some(strat) = strategy {
                        match get_value_merger(v, strat) {
                            Ok(m) => {
                                entry.insert(m);
                            }
                            Err(err) => {
                                warn!("failed to merge value: {}", err);
                            }
                        }
                    } else {
                        entry.insert(v.clone().into());
                    }
                }
                hash_map::Entry::Occupied(mut entry) => {
                    if let Err(err) = entry.get_mut().add(v.clone()) {
                        warn!("failed to merge value: {}", err);
                    }
                }
            }
        }
        self.stale_since = Instant::now();
    }

    fn flush(mut self) -> LogEvent {
        let mut event = Event::new_empty_log().into_log();
        for (k, v) in self.fields.drain() {
            if let Err(err) = v.insert_into(k, &mut event) {
                warn!("failed to merge values for field: {}", err);
            }
        }
        event
    }
}

//------------------------------------------------------------------------------

pub struct Reduce {
    expire_after: Duration,
    flush_period: Duration,
    identifier_fields: Vec<Atom>,
    merge_strategies: IndexMap<String, MergeStrategy>,
    reduce_merge_states: HashMap<Discriminant, ReduceState>,
    ends_when: Option<Box<dyn Condition>>,
}

impl Reduce {
    fn new(config: &ReduceConfig) -> crate::Result<Self> {
        let ends_when = if let Some(ends_conf) = &config.ends_when {
            Some(ends_conf.build()?)
        } else {
            None
        };

        let identifier_fields = config
            .identifier_fields
            .clone()
            .into_iter()
            .map(Atom::from)
            .collect();

        Ok(Reduce {
            expire_after: Duration::from_millis(config.expire_after_ms.unwrap_or(30000)),
            flush_period: Duration::from_millis(config.flush_period_ms.unwrap_or(1000)),
            identifier_fields,
            merge_strategies: config.merge_strategies.clone(),
            reduce_merge_states: HashMap::new(),
            ends_when,
        })
    }

    fn flush_into(&mut self, output: &mut Vec<Event>) {
        let mut flush_discriminants = Vec::new();
        for (k, t) in &self.reduce_merge_states {
            if t.stale_since.elapsed() >= self.expire_after {
                flush_discriminants.push(k.clone());
            }
        }
        for k in &flush_discriminants {
            if let Some(t) = self.reduce_merge_states.remove(k) {
                output.push(Event::from(t.flush()));
            }
        }
    }

    fn flush_all_into(&mut self, output: &mut Vec<Event>) {
        self.reduce_merge_states
            .drain()
            .for_each(|(_, s)| output.push(Event::from(s.flush())));
    }
}

impl Transform for Reduce {
    // Only used in tests
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut output = Vec::new();
        self.transform_into(&mut output, event);
        output.pop()
    }

    fn transform_into(&mut self, output: &mut Vec<Event>, event: Event) {
        let ends_here = self
            .ends_when
            .as_ref()
            .map(|c| c.check(&event))
            .unwrap_or(false);

        let event = event.into_log();
        let discriminant = Discriminant::from_log_event(&event, &self.identifier_fields);

        if ends_here {
            output.push(Event::from(
                if let Some(mut state) = self.reduce_merge_states.remove(&discriminant) {
                    state.add_event(event, &self.merge_strategies);
                    state.flush()
                } else {
                    ReduceState::new(event, &self.merge_strategies).flush()
                },
            ));
        } else {
            match self.reduce_merge_states.entry(discriminant) {
                hash_map::Entry::Vacant(entry) => {
                    entry.insert(ReduceState::new(event, &self.merge_strategies));
                }
                hash_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().add_event(event, &self.merge_strategies);
                }
            }
        }

        self.flush_into(output);
    }

    fn transform_stream(
        self: Box<Self>,
        input_rx: Box<dyn Stream01<Item = Event, Error = ()> + Send>,
    ) -> Box<dyn Stream01<Item = Event, Error = ()> + Send>
    where
        Self: 'static,
    {
        let mut me = self;

        let poll_period = me.flush_period;

        let mut flush_stream = tokio::time::interval(poll_period);
        let mut input_stream = Compat01As03::new(input_rx);

        let stream = stream! {
          loop {
            let mut output = Vec::new();
            let done = tokio::select! {
                _ = flush_stream.next() => {
                  me.flush_into(&mut output);
                  false
                }
                maybe_event = input_stream.next() => {
                  match maybe_event {
                    None => {
                      me.flush_all_into(&mut output);
                      true
                    }
                    Some(Ok(event)) => {
                      me.transform_into(&mut output, event);
                      false
                    }
                    Some(Err(())) => panic!("Unexpected error reading channel"),
                  }
                }
            };
            yield stream::iter(output.into_iter());
            if done { break }
          }
        }
        .flatten();

        // Needed for compat
        let try_stream = Box::pin(stream.map::<Result<Event, ()>, _>(Ok));

        Box::new(Compat::new(try_stream))
    }
}

#[cfg(test)]
mod test {
    use super::ReduceConfig;
    use crate::{
        config::{TransformConfig, TransformContext},
        event::Value,
        Event,
    };
    use serde_json::json;

    #[test]
    fn reduce_from_condition() {
        let mut reduce = toml::from_str::<ReduceConfig>(
            r#"
identifier_fields = [ "request_id" ]

[ends_when]
  "test_end.exists" = true
"#,
        )
        .unwrap()
        .build(TransformContext::new_test())
        .unwrap();

        let mut outputs = Vec::new();

        let mut e = Event::from("test message 1");
        e.as_mut_log().insert("counter", 1);
        e.as_mut_log().insert("request_id", "1");
        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 2");
        e.as_mut_log().insert("counter", 2);
        e.as_mut_log().insert("request_id", "2");
        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 3");
        e.as_mut_log().insert("counter", 3);
        e.as_mut_log().insert("request_id", "1");
        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 4");
        e.as_mut_log().insert("counter", 4);
        e.as_mut_log().insert("request_id", "1");
        e.as_mut_log().insert("test_end", "yep");
        reduce.transform_into(&mut outputs, e);

        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs.first().unwrap().as_log()[&"message".into()],
            "test message 1".into()
        );
        assert_eq!(
            outputs.first().unwrap().as_log()[&"counter".into()],
            Value::from(8)
        );

        outputs.clear();

        let mut e = Event::from("test message 5");
        e.as_mut_log().insert("counter", 5);
        e.as_mut_log().insert("request_id", "2");
        e.as_mut_log().insert("extra_field", "value1");
        e.as_mut_log().insert("test_end", "yep");
        reduce.transform_into(&mut outputs, e);

        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs.first().unwrap().as_log()[&"message".into()],
            "test message 2".into()
        );
        assert_eq!(
            outputs.first().unwrap().as_log()[&"extra_field".into()],
            "value1".into()
        );
        assert_eq!(
            outputs.first().unwrap().as_log()[&"counter".into()],
            Value::from(7)
        );
    }

    #[test]
    fn reduce_merge_strategies() {
        let mut reduce = toml::from_str::<ReduceConfig>(
            r#"
identifier_fields = [ "request_id" ]

merge_strategies.foo = "concat"
merge_strategies.bar = "array"
merge_strategies.baz = "max"

[ends_when]
  "test_end.exists" = true
"#,
        )
        .unwrap()
        .build(TransformContext::new_test())
        .unwrap();

        let mut outputs = Vec::new();

        let mut e = Event::from("test message 1");
        e.as_mut_log().insert("foo", "first foo");
        e.as_mut_log().insert("bar", "first bar");
        e.as_mut_log().insert("baz", 2);
        e.as_mut_log().insert("request_id", "1");

        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 2");
        e.as_mut_log().insert("foo", "second foo");
        e.as_mut_log().insert("bar", 2);
        e.as_mut_log().insert("baz", "not number");
        e.as_mut_log().insert("request_id", "1");

        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 3");
        e.as_mut_log().insert("foo", 10);
        e.as_mut_log().insert("bar", "third bar");
        e.as_mut_log().insert("baz", 3);
        e.as_mut_log().insert("request_id", "1");
        e.as_mut_log().insert("test_end", "yep");

        reduce.transform_into(&mut outputs, e);

        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs.first().unwrap().as_log()[&"message".into()],
            "test message 1".into()
        );
        assert_eq!(
            outputs.first().unwrap().as_log()[&"foo".into()],
            "first foo second foo".into()
        );
        assert_eq!(
            outputs.first().unwrap().as_log()[&"bar".into()],
            Value::Array(vec!["first bar".into(), 2.into(), "third bar".into()]),
        );
        assert_eq!(outputs.first().unwrap().as_log()[&"baz".into()], 3.into(),);
    }

    #[test]
    fn missing_identifier() {
        let mut reduce = toml::from_str::<ReduceConfig>(
            r#"
identifier_fields = [ "request_id" ]

[ends_when]
  "test_end.exists" = true
"#,
        )
        .unwrap()
        .build(TransformContext::new_test())
        .unwrap();

        let mut outputs = Vec::new();

        let mut e = Event::from("test message 1");
        e.as_mut_log().insert("counter", 1);
        e.as_mut_log().insert("request_id", "1");
        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 2");
        e.as_mut_log().insert("counter", 2);
        // e.as_mut_log().insert("request_id", "");
        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 3");
        e.as_mut_log().insert("counter", 3);
        e.as_mut_log().insert("request_id", "1");
        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 4");
        e.as_mut_log().insert("counter", 4);
        e.as_mut_log().insert("request_id", "1");
        e.as_mut_log().insert("test_end", "yep");
        reduce.transform_into(&mut outputs, e);

        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs.first().unwrap().as_log()[&"message".into()],
            "test message 1".into()
        );
        assert_eq!(
            outputs.first().unwrap().as_log()[&"counter".into()],
            Value::from(8)
        );

        outputs.clear();

        let mut e = Event::from("test message 5");
        e.as_mut_log().insert("counter", 5);
        e.as_mut_log().insert("extra_field", "value1");
        e.as_mut_log().insert("test_end", "yep");
        reduce.transform_into(&mut outputs, e);

        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs.first().unwrap().as_log()[&"message".into()],
            "test message 2".into()
        );
        assert_eq!(
            outputs.first().unwrap().as_log()[&"extra_field".into()],
            "value1".into()
        );
        assert_eq!(
            outputs.first().unwrap().as_log()[&"counter".into()],
            Value::from(7)
        );
    }

    #[test]
    fn arrays() {
        let mut reduce = toml::from_str::<ReduceConfig>(
            r#"
identifier_fields = [ "request_id" ]

merge_strategies.foo = "array"
merge_strategies.bar = "concat"

[ends_when]
  "test_end.exists" = true
"#,
        )
        .unwrap()
        .build(TransformContext::new_test())
        .unwrap();

        let mut outputs = Vec::new();

        let mut e = Event::from("test message 1");
        e.as_mut_log().insert("foo", json!([1, 3]));
        e.as_mut_log().insert("bar", json!([1, 3]));
        e.as_mut_log().insert("request_id", "1");
        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 2");
        e.as_mut_log().insert("foo", json!([2, 4]));
        e.as_mut_log().insert("bar", json!([2, 4]));
        e.as_mut_log().insert("request_id", "2");
        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 3");
        e.as_mut_log().insert("foo", json!([5, 7]));
        e.as_mut_log().insert("bar", json!([5, 7]));
        e.as_mut_log().insert("request_id", "1");
        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 4");
        e.as_mut_log().insert("foo", json!("done"));
        e.as_mut_log().insert("bar", json!("done"));
        e.as_mut_log().insert("request_id", "1");
        e.as_mut_log().insert("test_end", "yep");
        reduce.transform_into(&mut outputs, e);

        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs.first().unwrap().as_log()[&"foo".into()],
            json!([[1, 3], [5, 7], "done"]).into()
        );

        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs.first().unwrap().as_log()[&"bar".into()],
            json!([1, 3, 5, 7, "done"]).into()
        );

        outputs.clear();

        let mut e = Event::from("test message 5");
        e.as_mut_log().insert("foo", json!([6, 8]));
        e.as_mut_log().insert("bar", json!([6, 8]));
        e.as_mut_log().insert("request_id", "2");
        reduce.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 6");
        e.as_mut_log().insert("foo", json!("done"));
        e.as_mut_log().insert("bar", json!("done"));
        e.as_mut_log().insert("request_id", "2");
        e.as_mut_log().insert("test_end", "yep");
        reduce.transform_into(&mut outputs, e);

        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs.first().unwrap().as_log()[&"foo".into()],
            json!([[2, 4], [6, 8], "done"]).into()
        );
        assert_eq!(
            outputs.first().unwrap().as_log()[&"bar".into()],
            json!([2, 4, 6, 8, "done"]).into()
        );
    }
}

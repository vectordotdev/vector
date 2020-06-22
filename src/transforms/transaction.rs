use super::Transform;
use crate::{
    conditions::{AnyCondition, Condition},
    event::discriminant::Discriminant,
    event::{Event, LogEvent, Value},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use async_stream::stream;
use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use futures::{
    compat::{Compat, Compat01As03},
    stream,
    stream::StreamExt,
};
use futures01::Stream as Stream01;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map, HashMap};
use std::time::{Duration, Instant};
use string_cache::DefaultAtom as Atom;

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields, default)]
pub struct TransactionConfig {
    pub expire_after_ms: Option<u64>,

    pub flush_period_ms: Option<u64>,

    /// An ordered list of fields to distinguish transactions by. Each
    /// transaction has a separate event merging state.
    #[serde(default)]
    pub identifier_fields: Vec<String>,

    #[serde(default)]
    pub merge_strategies: IndexMap<String, MergeStrategy>,

    /// An optional condition that determines when an event is the end of a
    /// transaction.
    pub ends_when: Option<AnyCondition>,
}

inventory::submit! {
    TransformDescription::new::<TransactionConfig>("transaction")
}

#[typetag::serde(name = "transaction")]
impl TransformConfig for TransactionConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let t = Transaction::new(self)?;
        Ok(Box::new(t))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "transaction"
    }
}

//------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    Discard,
    Sum,
    Max,
    Min,
    Array,
    Concat,
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DiscardMerger {
    v: Value,
}

impl DiscardMerger {
    fn new(v: Value) -> Self {
        return Self { v };
    }
}

impl TransactionValueMerger for DiscardMerger {
    fn add(&mut self, _v: Value) -> Result<(), String> {
        Ok(())
    }

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        v.insert(k, self.v);
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ConcatMerger {
    v: BytesMut,
}

impl ConcatMerger {
    fn new(v: Bytes) -> Self {
        Self { v: v.into() }
    }
}

impl TransactionValueMerger for ConcatMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        if let Value::Bytes(b) = v {
            self.v.extend(&[b' ']);
            self.v.extend_from_slice(&b);
            Ok(())
        } else {
            Err(format!(
                "expected numeric value, found: '{}'",
                v.to_string_lossy()
            ))
        }
    }

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        v.insert(k, Value::Bytes(self.v.into()));
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ArrayMerger {
    v: Vec<Value>,
}

impl ArrayMerger {
    fn new(v: Vec<Value>) -> Self {
        Self { v }
    }
}

impl TransactionValueMerger for ArrayMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        self.v.push(v);
        Ok(())
    }

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        v.insert(k, Value::Array(self.v));
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TimestampWindowMerger {
    started: DateTime<Utc>,
    latest: DateTime<Utc>,
}

impl TimestampWindowMerger {
    fn new(v: DateTime<Utc>) -> Self {
        return Self {
            started: v.clone(),
            latest: v,
        };
    }
}

impl TransactionValueMerger for TimestampWindowMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        if let Value::Timestamp(ts) = v {
            self.latest = ts
        } else {
            return Err(format!(
                "expected timestamp value, found: {}",
                v.to_string_lossy()
            ));
        }
        Ok(())
    }

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        v.insert(format!("{}_end", k), Value::Timestamp(self.latest));
        v.insert(k, Value::Timestamp(self.started));
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum NumberMergerValue {
    Int(i64),
    Float(f64),
}

impl From<i64> for NumberMergerValue {
    fn from(v: i64) -> Self {
        NumberMergerValue::Int(v)
    }
}

impl From<f64> for NumberMergerValue {
    fn from(v: f64) -> Self {
        NumberMergerValue::Float(v)
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct AddNumbersMerger {
    v: NumberMergerValue,
}

impl AddNumbersMerger {
    fn new(v: NumberMergerValue) -> Self {
        return Self { v };
    }
}

impl TransactionValueMerger for AddNumbersMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        // Try and keep max precision with integer values, but once we've
        // received a float downgrade to float precision.
        match v {
            Value::Integer(i) => match self.v {
                NumberMergerValue::Int(j) => self.v = NumberMergerValue::Int(i + j),
                NumberMergerValue::Float(j) => self.v = NumberMergerValue::Float(i as f64 + j),
            },
            Value::Float(f) => match self.v {
                NumberMergerValue::Int(j) => self.v = NumberMergerValue::Float(f + j as f64),
                NumberMergerValue::Float(j) => self.v = NumberMergerValue::Float(f + j),
            },
            _ => {
                return Err(format!(
                    "expected numeric value, found: '{}'",
                    v.to_string_lossy()
                ));
            }
        }
        Ok(())
    }

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        match self.v {
            NumberMergerValue::Float(f) => v.insert(k, Value::Float(f)),
            NumberMergerValue::Int(i) => v.insert(k, Value::Integer(i)),
        };
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MaxNumberMerger {
    v: NumberMergerValue,
}

impl MaxNumberMerger {
    fn new(v: NumberMergerValue) -> Self {
        return Self { v };
    }
}

impl TransactionValueMerger for MaxNumberMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        // Try and keep max precision with integer values, but once we've
        // received a float downgrade to float precision.
        match v {
            Value::Integer(i) => {
                match self.v {
                    NumberMergerValue::Int(i2) => {
                        if i > i2 {
                            self.v = NumberMergerValue::Int(i);
                        }
                    }
                    NumberMergerValue::Float(f2) => {
                        let f = i as f64;
                        if f > f2 {
                            self.v = NumberMergerValue::Float(f);
                        }
                    }
                };
            }
            Value::Float(f) => {
                let f2 = match self.v {
                    NumberMergerValue::Int(i2) => i2 as f64,
                    NumberMergerValue::Float(f2) => f2,
                };
                if f > f2 {
                    self.v = NumberMergerValue::Float(f);
                }
            }
            _ => {
                return Err(format!(
                    "expected numeric value, found: '{}'",
                    v.to_string_lossy()
                ));
            }
        }
        Ok(())
    }

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        match self.v {
            NumberMergerValue::Float(f) => v.insert(k, Value::Float(f)),
            NumberMergerValue::Int(i) => v.insert(k, Value::Integer(i)),
        };
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MinNumberMerger {
    v: NumberMergerValue,
}

impl MinNumberMerger {
    fn new(v: NumberMergerValue) -> Self {
        return Self { v };
    }
}

impl TransactionValueMerger for MinNumberMerger {
    fn add(&mut self, v: Value) -> Result<(), String> {
        // Try and keep max precision with integer values, but once we've
        // received a float downgrade to float precision.
        match v {
            Value::Integer(i) => {
                match self.v {
                    NumberMergerValue::Int(i2) => {
                        if i < i2 {
                            self.v = NumberMergerValue::Int(i);
                        }
                    }
                    NumberMergerValue::Float(f2) => {
                        let f = i as f64;
                        if f < f2 {
                            self.v = NumberMergerValue::Float(f);
                        }
                    }
                };
            }
            Value::Float(f) => {
                let f2 = match self.v {
                    NumberMergerValue::Int(i2) => i2 as f64,
                    NumberMergerValue::Float(f2) => f2,
                };
                if f < f2 {
                    self.v = NumberMergerValue::Float(f);
                }
            }
            _ => {
                return Err(format!(
                    "expected numeric value, found: '{}'",
                    v.to_string_lossy()
                ));
            }
        }
        Ok(())
    }

    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String> {
        match self.v {
            NumberMergerValue::Float(f) => v.insert(k, Value::Float(f)),
            NumberMergerValue::Int(i) => v.insert(k, Value::Integer(i)),
        };
        Ok(())
    }
}

//------------------------------------------------------------------------------

trait TransactionValueMerger: std::fmt::Debug + Send + Sync {
    fn add(&mut self, v: Value) -> Result<(), String>;
    fn insert_into(self: Box<Self>, k: String, v: &mut LogEvent) -> Result<(), String>;
}

impl From<Value> for Box<dyn TransactionValueMerger> {
    fn from(v: Value) -> Self {
        match v {
            Value::Integer(i) => Box::new(AddNumbersMerger::new(i.into())),
            Value::Float(f) => Box::new(AddNumbersMerger::new(f.into())),
            Value::Timestamp(ts) => Box::new(TimestampWindowMerger::new(ts)),
            Value::Map(_) => Box::new(DiscardMerger::new(v)),
            Value::Null => Box::new(DiscardMerger::new(v)),
            Value::Boolean(_) => Box::new(DiscardMerger::new(v)),
            Value::Bytes(_) => Box::new(DiscardMerger::new(v)),
            Value::Array(_) => Box::new(DiscardMerger::new(v)),
        }
    }
}

fn get_value_merger(
    v: Value,
    m: &MergeStrategy,
) -> Result<Box<dyn TransactionValueMerger>, String> {
    match m {
        MergeStrategy::Sum => match v {
            Value::Integer(i) => Ok(Box::new(AddNumbersMerger::new(i.into()))),
            Value::Float(f) => Ok(Box::new(AddNumbersMerger::new(f.into()))),
            _ => Err(format!(
                "expected number value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::Max => match v {
            Value::Integer(i) => Ok(Box::new(MaxNumberMerger::new(i.into()))),
            Value::Float(f) => Ok(Box::new(MaxNumberMerger::new(f.into()))),
            _ => Err(format!(
                "expected number value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::Min => match v {
            Value::Integer(i) => Ok(Box::new(MinNumberMerger::new(i.into()))),
            Value::Float(f) => Ok(Box::new(MinNumberMerger::new(f.into()))),
            _ => Err(format!(
                "expected number value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::Concat => match v {
            Value::Bytes(b) => Ok(Box::new(ConcatMerger::new(b))),
            _ => Err(format!(
                "expected string value, found: '{}'",
                v.to_string_lossy()
            )),
        },
        MergeStrategy::Array => match v {
            Value::Array(a) => Ok(Box::new(ArrayMerger::new(a))),
            _ => Ok(Box::new(ArrayMerger::new(vec![v]))),
        },
        MergeStrategy::Discard => Ok(Box::new(DiscardMerger::new(v))),
    }
}

//------------------------------------------------------------------------------

struct TransactionState {
    fields: HashMap<String, Box<dyn TransactionValueMerger>>,
    stale_since: Instant,
}

impl TransactionState {
    fn new(e: LogEvent, strategies: &IndexMap<String, MergeStrategy>) -> Self {
        Self {
            stale_since: Instant::now(),
            // TODO: all_fields alternative that consumes
            fields: e
                .all_fields()
                .filter_map(|(k, v)| {
                    if let Some(strat) = strategies.get(&k) {
                        match get_value_merger(v.clone(), strat) {
                            Ok(m) => Some((k, m)),
                            Err(err) => {
                                warn!("failed to create merger for field '{}': {}", k, err);
                                None
                            }
                        }
                    } else {
                        Some((k, v.clone().into()))
                    }
                })
                .collect(),
        }
    }

    fn add_event(&mut self, e: LogEvent, strategies: &IndexMap<String, MergeStrategy>) {
        for (k, v) in e.all_fields() {
            let strategy = strategies.get(&k);
            match self.fields.entry(k) {
                hash_map::Entry::Vacant(entry) => {
                    if let Some(strat) = strategy {
                        match get_value_merger(v.clone(), strat) {
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

pub struct Transaction {
    expire_after: Duration,
    flush_period: Duration,
    identifier_fields: Vec<Atom>,
    merge_strategies: IndexMap<String, MergeStrategy>,
    transaction_merge_states: HashMap<Discriminant, TransactionState>,
    ends_when: Option<Box<dyn Condition>>,
}

impl Transaction {
    fn new(config: &TransactionConfig) -> crate::Result<Self> {
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

        Ok(Transaction {
            expire_after: Duration::from_millis(config.expire_after_ms.unwrap_or(30000)),
            flush_period: Duration::from_millis(config.flush_period_ms.unwrap_or(1000)),
            identifier_fields,
            merge_strategies: config.merge_strategies.clone(),
            transaction_merge_states: HashMap::new(),
            ends_when,
        })
    }

    fn flush_into(&mut self, output: &mut Vec<Event>) {
        let mut flush_discriminants = Vec::new();
        for (k, t) in &self.transaction_merge_states {
            if t.stale_since.elapsed() >= self.expire_after {
                flush_discriminants.push(k.clone());
            }
        }
        for k in &flush_discriminants {
            if let Some(t) = self.transaction_merge_states.remove(k) {
                output.push(Event::from(t.flush()));
            }
        }
    }

    fn flush_all_into(&mut self, output: &mut Vec<Event>) {
        self.transaction_merge_states
            .drain()
            .for_each(|(_, s)| output.push(Event::from(s.flush())));
    }
}

impl Transform for Transaction {
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
                if let Some(mut state) = self.transaction_merge_states.remove(&discriminant) {
                    state.add_event(event, &self.merge_strategies);
                    state.flush()
                } else {
                    TransactionState::new(event, &self.merge_strategies).flush()
                },
            ));
        } else {
            match self.transaction_merge_states.entry(discriminant) {
                hash_map::Entry::Vacant(entry) => {
                    entry.insert(TransactionState::new(event, &self.merge_strategies));
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

        let poll_period = me.flush_period.clone();

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
                    Some(Err(())) => unreachable!(),
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
    use super::TransactionConfig;
    use crate::{
        event::Value,
        topology::config::{TransformConfig, TransformContext},
        Event,
    };

    #[test]
    fn transaction_from_condition() {
        let rt = crate::runtime::Runtime::single_threaded().unwrap();
        let mut transaction = toml::from_str::<TransactionConfig>(
            r#"
identifier_fields = [ "request_id" ]
[ends_when]
  "test_end.exists" = true
"#,
        )
        .unwrap()
        .build(TransformContext::new_test(rt.executor()))
        .unwrap();

        let mut outputs = Vec::new();

        let mut e = Event::from("test message 1");
        e.as_mut_log().insert("counter", 1);
        e.as_mut_log().insert("request_id", "1");
        transaction.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 2");
        e.as_mut_log().insert("counter", 2);
        e.as_mut_log().insert("request_id", "2");
        transaction.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 3");
        e.as_mut_log().insert("counter", 3);
        e.as_mut_log().insert("request_id", "1");
        transaction.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 4");
        e.as_mut_log().insert("counter", 4);
        e.as_mut_log().insert("request_id", "1");
        e.as_mut_log().insert("test_end", "yep");
        transaction.transform_into(&mut outputs, e);

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
        transaction.transform_into(&mut outputs, e);

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
    fn transaction_merge_strategies() {
        let rt = crate::runtime::Runtime::single_threaded().unwrap();
        let mut transaction = toml::from_str::<TransactionConfig>(
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
        .build(TransformContext::new_test(rt.executor()))
        .unwrap();

        let mut outputs = Vec::new();

        let mut e = Event::from("test message 1");
        e.as_mut_log().insert("foo", "first foo");
        e.as_mut_log().insert("bar", "first bar");
        e.as_mut_log().insert("baz", 2);
        e.as_mut_log().insert("request_id", "1");

        transaction.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 2");
        e.as_mut_log().insert("foo", "second foo");
        e.as_mut_log().insert("bar", 2);
        e.as_mut_log().insert("baz", "not number");
        e.as_mut_log().insert("request_id", "1");

        transaction.transform_into(&mut outputs, e);

        let mut e = Event::from("test message 3");
        e.as_mut_log().insert("foo", 10);
        e.as_mut_log().insert("bar", "third bar");
        e.as_mut_log().insert("baz", 3);
        e.as_mut_log().insert("request_id", "1");
        e.as_mut_log().insert("test_end", "yep");

        transaction.transform_into(&mut outputs, e);

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
}

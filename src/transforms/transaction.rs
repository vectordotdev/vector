use super::Transform;
use crate::{
    conditions::{Condition, DefaultedCondition},
    event::discriminant::Discriminant,
    event::{Event, LogEvent, Value},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use futures01::{stream, sync::mpsc::Receiver, Async, Poll, Stream};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map, HashMap};
use std::time::{Duration, Instant};
use string_cache::DefaultAtom as Atom;

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct TransactionConfig {
    pub flush_period_ms: Option<u64>,

    /// An ordered list of fields to distinguish transactions by. Each
    /// transaction has a separate event merging state.
    pub identifier_fields: Vec<Atom>,

    #[serde(default)]
    pub merge_strategies: IndexMap<Atom, MergeStrategy>,

    /// An optional condition that determines when an event is the end of a
    /// transaction.
    pub ends_when: Option<DefaultedCondition>,
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
    Array,
    Append,
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

    fn insert_into(self: Box<Self>, k: Atom, v: &mut LogEvent) -> Result<(), String> {
        Ok(v.insert(k, self.v))
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct AppendMerger {
    v: BytesMut,
}

impl AppendMerger {
    fn new(v: Bytes) -> Self {
        Self { v: v.into() }
    }
}

impl TransactionValueMerger for AppendMerger {
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

    fn insert_into(self: Box<Self>, k: Atom, v: &mut LogEvent) -> Result<(), String> {
        Ok(v.insert(k, Value::Bytes(self.v.into())))
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

    fn insert_into(self: Box<Self>, k: Atom, v: &mut LogEvent) -> Result<(), String> {
        Ok(v.insert(k, Value::Array(self.v)))
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

    fn insert_into(self: Box<Self>, k: Atom, v: &mut LogEvent) -> Result<(), String> {
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

    fn insert_into(self: Box<Self>, k: Atom, v: &mut LogEvent) -> Result<(), String> {
        Ok(match self.v {
            NumberMergerValue::Float(f) => v.insert(k, Value::Float(f)),
            NumberMergerValue::Int(i) => v.insert(k, Value::Integer(i)),
        })
    }
}

//------------------------------------------------------------------------------

trait TransactionValueMerger: std::fmt::Debug + Send + Sync {
    fn add(&mut self, v: Value) -> Result<(), String>;
    fn insert_into(self: Box<Self>, k: Atom, v: &mut LogEvent) -> Result<(), String>;
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
        MergeStrategy::Append => match v {
            Value::Bytes(b) => Ok(Box::new(AppendMerger::new(b))),
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
    fields: HashMap<Atom, Box<dyn TransactionValueMerger>>,
    stale_since: Instant,
}

impl TransactionState {
    fn new(e: LogEvent, strategies: &IndexMap<Atom, MergeStrategy>) -> Self {
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

    fn add_event(&mut self, e: LogEvent, strategies: &IndexMap<Atom, MergeStrategy>) {
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
    flush_period: Duration,
    poll_period: Duration,
    identifier_fields: Vec<Atom>,
    merge_strategies: IndexMap<Atom, MergeStrategy>,
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
        Ok(Transaction {
            flush_period: Duration::from_millis(config.flush_period_ms.unwrap_or(30000)),
            poll_period: Duration::from_millis(2000), // TODO: Configure this?
            identifier_fields: config.identifier_fields.clone(),
            merge_strategies: config.merge_strategies.clone(),
            transaction_merge_states: HashMap::new(),
            ends_when: ends_when,
        })
    }

    fn flush_into(&mut self, output: &mut Vec<Event>) {
        let mut flush_discriminants = Vec::new();
        for (k, t) in &self.transaction_merge_states {
            if t.stale_since.elapsed() >= self.flush_period {
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

enum StreamEvent {
    Flush,
    FlushAll,
    Event(Event),
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
                    event
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
        mut input_rx: Receiver<Event>,
    ) -> Box<dyn Stream<Item = Event, Error = ()> + Send>
    where
        Self: 'static,
    {
        let mut me = self;

        let flush_period = me.poll_period.clone();
        let mut last_flush = Instant::now();

        let poll_flush = move || -> Poll<Option<StreamEvent>, ()> {
            // TODO: This blocks until a message is ready, which defeats the
            // point in polling. Looks like newer futures has `poll_next`, which
            // is what we actually want.
            let p = input_rx.poll();
            match p {
                Poll::Ok(Async::NotReady) => {
                    // If our input channel hasn't yielded anything
                    let now = Instant::now();

                    // And it has been long enough since the last flush
                    if flush_period <= now.duration_since(last_flush) {
                        last_flush = now;

                        // Trigger a flush
                        Poll::Ok(Async::Ready(Some(StreamEvent::Flush)))
                    } else {
                        Poll::Ok(Async::NotReady)
                    }
                }
                // Pass `Poll<Option<Event>>` as `Poll<Option<StreamEvent>>`
                _ => return p.map(|p| p.map(|p| p.map(|p| StreamEvent::Event(p)))),
            }
        };

        Box::new(
            stream::poll_fn(poll_flush)
                .chain(stream::iter_ok(vec![StreamEvent::FlushAll]))
                .map(move |event_opt| {
                    let mut output = Vec::new();
                    match event_opt {
                        StreamEvent::Flush => me.flush_into(&mut output),
                        StreamEvent::FlushAll => me.flush_all_into(&mut output),
                        StreamEvent::Event(event) => me.transform_into(&mut output, event),
                    }
                    futures01::stream::iter_ok(output.into_iter())
                })
                .flatten(),
        )
    }
}

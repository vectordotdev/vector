mod error;
pub mod log_event;
pub mod lua;
pub mod metric;
pub mod value;
pub mod visitors;

pub use error::EventError;
pub use log_event::LogEvent;
pub use metric::{Metric, MetricKind, MetricValue, StatisticKind};
use std::{
    collections::{BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
};
pub use value::Value;

#[derive(PartialEq, Debug, Clone)]
pub enum Event {
    Log(LogEvent),
    Metric(Metric),
}

impl Event {
    pub fn new_empty_log() -> Self {
        Event::Log(LogEvent::default())
    }

    pub fn as_log(&self) -> &LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn as_mut_log(&mut self) -> &mut LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn into_log(self) -> LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn as_metric(&self) -> &Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    pub fn as_mut_metric(&mut self) -> &mut Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    pub fn into_metric(self) -> Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }
}

#[macro_export]
macro_rules! log_event {
    ($($key:expr => $value:expr),*  $(,)?) => {
        {
            let mut event = $crate::event::Event::Log($crate::event::LogEvent::default());
            let log = event.as_mut_log();
            $(
                log.insert($key, $value);
            )*
            event
        }
    };
}

impl From<BTreeMap<String, Value>> for Event {
    fn from(map: BTreeMap<String, Value>) -> Self {
        Self::Log(LogEvent::from(map))
    }
}

impl From<HashMap<String, Value>> for Event {
    fn from(map: HashMap<String, Value>) -> Self {
        Self::Log(LogEvent::from(map))
    }
}

impl TryFrom<serde_json::Value> for Event {
    type Error = crate::Error;

    fn try_from(map: serde_json::Value) -> Result<Self, Self::Error> {
        match map {
            serde_json::Value::Object(fields) => Ok(Event::from(
                fields
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<BTreeMap<_, _>>(),
            )),
            _ => Err(crate::Error::from(
                "Attempted to convert non-Object JSON into an Event.",
            )),
        }
    }
}

impl TryInto<serde_json::Value> for Event {
    type Error = serde_json::Error;

    fn try_into(self) -> Result<serde_json::Value, Self::Error> {
        match self {
            Event::Log(fields) => serde_json::to_value(fields),
            Event::Metric(metric) => serde_json::to_value(metric),
        }
    }
}

impl From<LogEvent> for Event {
    fn from(log: LogEvent) -> Self {
        Event::Log(log)
    }
}

impl From<Metric> for Event {
    fn from(metric: Metric) -> Self {
        Event::Metric(metric)
    }
}

impl vrl::Target for Event {
    fn get(&self, path: &vrl::Path) -> Result<Option<vrl::Value>, String> {
        match self {
            Event::Log(log) => vrl::Target::get(log, path),
            Event::Metric(metric) => vrl::Target::get(metric, path),
        }
    }

    fn remove(
        &mut self,
        path: &vrl::Path,
        compact: bool,
    ) -> Result<Option<vrl::Value>, String> {
        match self {
            Event::Log(log) => vrl::Target::remove(log, path, compact),
            Event::Metric(metric) => vrl::Target::remove(metric, path, compact),
        }
    }

    fn insert(&mut self, path: &vrl::Path, value: vrl::Value) -> Result<(), String> {
        match self {
            Event::Log(log) => vrl::Target::insert(log, path, value),
            Event::Metric(metric) => vrl::Target::insert(metric, path, value),
        }
    }
}

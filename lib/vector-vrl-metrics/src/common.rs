use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::interval;
use tokio_stream::{wrappers::IntervalStream, StreamExt};
use vector_common::shutdown::ShutdownSignal;
use vrl::{diagnostic::Label, prelude::*};

use arc_swap::ArcSwap;
use vector_core::{event::Metric, metrics::Controller};

#[derive(Debug)]
pub(crate) enum Error {
    MetricsStorageNotLoaded,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::MetricsStorageNotLoaded => write!(f, "metrics storage not loaded"),
        }
    }
}

impl std::error::Error for Error {}

impl DiagnosticMessage for Error {
    fn code(&self) -> usize {
        111
    }

    fn labels(&self) -> Vec<Label> {
        match self {
            Error::MetricsStorageNotLoaded => {
                vec![Label::primary(
                    "VRL metrics error: metrics storage not loaded".to_string(),
                    Span::default(),
                )]
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct MetricsStorage {
    cache: Arc<ArcSwap<Vec<Metric>>>,
}

impl MetricsStorage {
    pub(crate) fn get_metric(&self, metric: &str) -> Option<Metric> {
        self.cache
            .load()
            .iter()
            .find(|m| m.name() == metric)
            .cloned()
    }

    pub(crate) fn find_metrics(&self, metric: &str) -> Vec<Metric> {
        self.cache
            .load()
            .iter()
            .filter(|m| m.name() == metric)
            .cloned()
            .collect()
    }

    pub fn refresh_metrics(&self) {
        let new_metrics = Controller::get()
            .expect("metrics not initialized")
            .capture_metrics();
        self.cache.store(new_metrics.into());
    }

    pub async fn run_periodic_refresh(
        &self,
        refresh_interval: Duration,
        mut shutdown: ShutdownSignal,
    ) {
        let mut intervals = IntervalStream::new(interval(refresh_interval));
        loop {
            tokio::select! {
                Some(_) = intervals.next() => {
                    self.refresh_metrics();
                }
                _ = &mut shutdown => {
                    break;
                }
            }
        }
    }
}

pub(crate) fn metrics_vrl_typedef() -> BTreeMap<Field, Kind> {
    BTreeMap::from([
        (Field::from("name"), Kind::bytes()),
        (Field::from("tags"), Kind::any_object()),
        (Field::from("type"), Kind::bytes()),
        (Field::from("kind"), Kind::bytes()),
        (Field::from("value"), Kind::integer() | Kind::null()),
    ])
}

pub(crate) fn metric_into_vrl(value: &Metric) -> Value {
    Value::Object(BTreeMap::from([
        ("name".into(), Value::Bytes(value.name().to_string().into())),
        (
            "tags".into(),
            Value::Object(BTreeMap::from_iter(
                value
                    .tags()
                    .map(|t| {
                        t.iter_sets()
                            .map(|(k, v)| {
                                (
                                    k.into(),
                                    Value::Array(
                                        v.iter()
                                            .filter_map(|v| {
                                                v.map(ToString::to_string)
                                                    .map(Into::into)
                                                    .map(Value::Bytes)
                                            })
                                            .collect(),
                                    ),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            )),
        ),
        ("type".into(), Value::Bytes(value.value().as_name().into())),
        (
            "kind".into(),
            Value::Bytes(
                match value.kind() {
                    vector_core::event::MetricKind::Incremental => "incremental",
                    vector_core::event::MetricKind::Absolute => "Absolute",
                }
                .into(),
            ),
        ),
        (
            "value".into(),
            match value.value() {
                vector_core::event::MetricValue::Counter { value }
                | vector_core::event::MetricValue::Gauge { value } => {
                    NotNan::new(*value).map_or(Value::Null, Value::Float)
                }
                _ => Value::Null,
            },
        ),
    ]))
}

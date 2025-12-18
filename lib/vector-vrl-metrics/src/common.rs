use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::interval;
use tokio_stream::{wrappers::IntervalStream, StreamExt};
use vector_common::shutdown::ShutdownSignal;
use vrl::{diagnostic::Label, prelude::*, value};

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
        112
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
    pub(crate) fn get_metric(
        &self,
        metric: &str,
        tags: BTreeMap<String, String>,
    ) -> Option<Metric> {
        self.cache
            .load()
            .iter()
            .find(|m| m.name() == metric && tags.iter().all(|tag| tag_matches(m, tag)))
            .cloned()
    }

    pub(crate) fn find_metrics(&self, metric: &str, tags: BTreeMap<String, String>) -> Vec<Metric> {
        self.cache
            .load()
            .iter()
            .filter(|m| m.name() == metric && tags.iter().all(|tag| tag_matches(m, tag)))
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

/// Checks if the tag matches - also considers wildcards
fn tag_matches(metric: &Metric, (tag_key, tag_value): (&String, &String)) -> bool {
    if let Some(wildcard_index) = tag_value.find('*') {
        let Some(metric_tag_value) = metric.tag_value(tag_key) else {
            return false;
        };

        metric_tag_value.starts_with(&tag_value[0..wildcard_index])
            && metric_tag_value.ends_with(&tag_value[(wildcard_index + 1)..])
    } else {
        metric.tag_matches(tag_key, tag_value)
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
    value!({
        name: { value.name() },
        tags: {
            BTreeMap::from_iter(
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
                                        v.map(ToString::to_string).map(Into::into).map(Value::Bytes)
                                    })
                                    .collect(),
                                ),
                            )
                        })
                    .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            )
        },
        "type": { value.value().as_name() },
        kind: {
            match value.kind() {
                vector_core::event::MetricKind::Incremental => "incremental",
                vector_core::event::MetricKind::Absolute => "absolute",
            }
        },
        value: {
            match value.value() {
                vector_core::event::MetricValue::Counter { value }
                | vector_core::event::MetricValue::Gauge { value } => NotNan::new(*value).ok(),
                _ => None,
            };
        }
    })
}

// Tests are defined here to simplify them - enabling access to `MetricsStorage`
#[cfg(test)]
mod tests {
    use vector_core::{
        compile_vrl,
        event::{Event, LogEvent, MetricKind, MetricTags, VrlTarget},
    };
    use vrl::{
        compiler::{
            runtime::{Runtime, Terminate},
            CompilationResult, CompileConfig,
        },
        diagnostic::DiagnosticList,
    };

    use super::*;

    fn compile(
        storage: MetricsStorage,
        vrl_source: &str,
    ) -> Result<CompilationResult, DiagnosticList> {
        let functions = vrl::stdlib::all().into_iter();

        let functions = functions.chain(crate::all()).collect::<Vec<_>>();

        let state = TypeState::default();

        let mut config = CompileConfig::default();
        config.set_custom(storage.clone());
        config.set_read_only();

        compile_vrl(vrl_source, &functions, &state, config)
    }

    fn compile_and_run(storage: MetricsStorage, vrl_source: &str) -> Result<Value, Terminate> {
        let CompilationResult {
            program,
            warnings: _,
            config: _,
        } = compile(storage, vrl_source).expect("compilation failed");

        let mut target = VrlTarget::new(Event::Log(LogEvent::default()), program.info(), false);
        Runtime::default().resolve(&mut target, &program, &TimeZone::default())
    }

    fn assert_metric_matches(
        metric: &BTreeMap<KeyString, Value>,
        name: &str,
        value: f64,
        tags: Option<Vec<(&str, &str)>>,
    ) {
        assert_eq!(metric.get("name").unwrap().as_str().unwrap(), name);
        assert_eq!(
            metric.get("value").unwrap().as_float().unwrap(),
            NotNan::new(value).unwrap()
        );

        if let Some(tags) = tags {
            let metric_tags = metric.get("tags").unwrap().as_object().unwrap();
            for (key, value) in tags {
                assert_eq!(
                    metric_tags
                        .get(key)
                        .unwrap()
                        .as_array_unwrap()
                        .first()
                        .unwrap()
                        .as_str()
                        .unwrap(),
                    value
                );
            }
        }
    }

    #[test]
    fn test_get_vector_metric() {
        let storage = MetricsStorage::default();
        storage.cache.store(
            vec![Metric::new(
                "test",
                MetricKind::Absolute,
                vector_core::event::MetricValue::Gauge { value: 1.0 },
            )]
            .into(),
        );

        let result = compile_and_run(
            storage,
            r#"
            get_vector_metric("test")
        "#,
        )
        .expect("vrl failed");
        let result = result.as_object().unwrap();

        assert_metric_matches(result, "test", 1.0, None);
    }

    #[test]
    fn test_find_vector_metrics() {
        let storage = MetricsStorage::default();
        storage.cache.store(
            vec![
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "a".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "b".to_string(),
                )]))),
            ]
            .into(),
        );

        let result = compile_and_run(
            storage,
            r#"
            find_vector_metrics("test")
        "#,
        )
        .expect("vrl failed");
        let result = result.as_array_unwrap();

        assert_metric_matches(
            result[0].as_object().unwrap(),
            "test",
            1.0,
            Some(vec![("component_id", "a")]),
        );
        assert_metric_matches(
            result[1].as_object().unwrap(),
            "test",
            1.0,
            Some(vec![("component_id", "b")]),
        );
    }

    #[test]
    fn test_get_vector_metric_by_tag() {
        let storage = MetricsStorage::default();
        storage.cache.store(
            vec![
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "a".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "b".to_string(),
                )]))),
            ]
            .into(),
        );

        let result = compile_and_run(
            storage,
            r#"
            get_vector_metric("test", tags: { "component_id": "b" })
        "#,
        )
        .expect("vrl failed");
        let result = result.as_object().unwrap();

        assert_metric_matches(result, "test", 1.0, Some(vec![("component_id", "b")]));
    }

    #[test]
    fn test_find_vector_metrics_wildcard() {
        let storage = MetricsStorage::default();
        storage.cache.store(
            vec![
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "a".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "b".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                ),
            ]
            .into(),
        );

        let result = compile_and_run(
            storage,
            r#"
            find_vector_metrics("test", tags: { "component_id": "*" })
        "#,
        )
        .expect("vrl failed");
        let result = result.as_array_unwrap();

        // 2 metrics, because they have component_id, 3rd one doesn't
        assert_eq!(result.len(), 2);
        assert_metric_matches(
            result[0].as_object().unwrap(),
            "test",
            1.0,
            Some(vec![("component_id", "a")]),
        );
        assert_metric_matches(
            result[1].as_object().unwrap(),
            "test",
            1.0,
            Some(vec![("component_id", "b")]),
        );
    }

    #[test]
    fn test_find_vector_metrics_wildcard_start() {
        let storage = MetricsStorage::default();
        storage.cache.store(
            vec![
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "prefix.a".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "something_else".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "prefix.c".to_string(),
                )]))),
            ]
            .into(),
        );

        let result = compile_and_run(
            storage,
            r#"
            find_vector_metrics("test", tags: { "component_id": "prefix.*" })
        "#,
        )
        .expect("vrl failed");
        let result = result.as_array_unwrap();

        assert_eq!(result.len(), 2);
        assert_metric_matches(
            result[0].as_object().unwrap(),
            "test",
            1.0,
            Some(vec![("component_id", "prefix.a")]),
        );
        assert_metric_matches(
            result[1].as_object().unwrap(),
            "test",
            1.0,
            Some(vec![("component_id", "prefix.c")]),
        );
    }

    #[test]
    fn test_find_vector_metrics_wildcard_end() {
        let storage = MetricsStorage::default();
        storage.cache.store(
            vec![
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "a.suffix".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "something_else".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "c.suffix".to_string(),
                )]))),
            ]
            .into(),
        );

        let result = compile_and_run(
            storage,
            r#"
            find_vector_metrics("test", tags: { "component_id": "*.suffix" })
        "#,
        )
        .expect("vrl failed");
        let result = result.as_array_unwrap();

        assert_eq!(result.len(), 2);
        assert_metric_matches(
            result[0].as_object().unwrap(),
            "test",
            1.0,
            Some(vec![("component_id", "a.suffix")]),
        );
        assert_metric_matches(
            result[1].as_object().unwrap(),
            "test",
            1.0,
            Some(vec![("component_id", "c.suffix")]),
        );
    }

    #[test]
    fn test_find_vector_metrics_wildcard_middle() {
        let storage = MetricsStorage::default();
        storage.cache.store(
            vec![
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "start.a.end".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "something_else".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "start.c.end".to_string(),
                )]))),
            ]
            .into(),
        );

        let result = compile_and_run(
            storage,
            r#"
            find_vector_metrics("test", tags: { "component_id": "start.*.end" })
        "#,
        )
        .expect("vrl failed");
        let result = result.as_array_unwrap();

        assert_eq!(result.len(), 2);
        assert_metric_matches(
            result[0].as_object().unwrap(),
            "test",
            1.0,
            Some(vec![("component_id", "start.a.end")]),
        );
        assert_metric_matches(
            result[1].as_object().unwrap(),
            "test",
            1.0,
            Some(vec![("component_id", "start.c.end")]),
        );
    }

    #[test]
    fn test_aggregate_vector_metrics_sum() {
        let storage = MetricsStorage::default();
        storage.cache.store(
            vec![
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 6.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "start.a.end".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "something_else".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 3.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "start.c.end".to_string(),
                )]))),
            ]
            .into(),
        );

        let result = compile_and_run(
            storage,
            r#"
            aggregate_vector_metrics("sum", "test", tags: { "component_id": "start.*.end" })
        "#,
        )
        .expect("vrl failed");
        let result = result.as_float().unwrap();

        assert_eq!(result.into_inner(), 9.0);
    }

    #[test]
    fn test_aggregate_vector_metrics_avg() {
        let storage = MetricsStorage::default();
        storage.cache.store(
            vec![
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 6.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "start.a.end".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "something_else".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 3.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "start.c.end".to_string(),
                )]))),
            ]
            .into(),
        );

        let result = compile_and_run(
            storage,
            r#"
            aggregate_vector_metrics("avg", "test", tags: { "component_id": "start.*.end" })
        "#,
        )
        .expect("vrl failed");
        let result = result.as_float().unwrap();

        assert_eq!(result.into_inner(), 4.5);
    }

    #[test]
    fn test_aggregate_vector_metrics_max() {
        let storage = MetricsStorage::default();
        storage.cache.store(
            vec![
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 6.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "start.a.end".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "something_else".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 3.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "start.c.end".to_string(),
                )]))),
            ]
            .into(),
        );

        let result = compile_and_run(
            storage,
            r#"
            aggregate_vector_metrics("max", "test", tags: { "component_id": "start.*.end" })
        "#,
        )
        .expect("vrl failed");
        let result = result.as_float().unwrap();

        assert_eq!(result.into_inner(), 6.0);
    }

    #[test]
    fn test_aggregate_vector_metrics_min() {
        let storage = MetricsStorage::default();
        storage.cache.store(
            vec![
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 6.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "start.a.end".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 1.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "something_else".to_string(),
                )]))),
                Metric::new(
                    "test",
                    MetricKind::Absolute,
                    vector_core::event::MetricValue::Gauge { value: 3.0 },
                )
                .with_tags(Some(MetricTags::from_iter([(
                    "component_id".to_string(),
                    "start.c.end".to_string(),
                )]))),
            ]
            .into(),
        );

        let result = compile_and_run(
            storage,
            r#"
            aggregate_vector_metrics("min", "test", tags: { "component_id": "start.*.end" })
        "#,
        )
        .expect("vrl failed");
        let result = result.as_float().unwrap();

        assert_eq!(result.into_inner(), 3.0);
    }
}

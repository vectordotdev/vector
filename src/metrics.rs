use crate::{event::Metric, Event};
use metrics::{Key, KeyData, Label, Recorder, SharedString, Unit};
use metrics_tracing_context::{LabelFilter, TracingContextLayer};
use metrics_util::layers::Layer;
use metrics_util::{CompositeKey, Handle, MetricKind, Registry};
use once_cell::sync::OnceCell;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

static CONTROLLER: OnceCell<Controller> = OnceCell::new();

// Cardinality counter parameters, expose the internal metrics registry
// cardinality.
// Useful for the end users to help understand the characteristics of their
// environment and how vectors acts in it.
const CARDINALITY_KEY_NAME: &str = "internal_metrics_cardinality";
static CARDINALITY_KEY_DATA_NAME: [SharedString; 1] =
    [SharedString::const_str(&CARDINALITY_KEY_NAME)];
static CARDINALITY_KEY_DATA: KeyData = KeyData::from_static_name(&CARDINALITY_KEY_DATA_NAME);
static CARDINALITY_KEY: CompositeKey =
    CompositeKey::new(MetricKind::COUNTER, Key::Borrowed(&CARDINALITY_KEY_DATA));

pub fn init() -> crate::Result<()> {
    // Prepare the registry.
    let registry = Registry::new();
    let registry = Arc::new(registry);

    // Init the cardinality counter.
    let cardinality_counter = Arc::new(AtomicU64::new(1));
    // Inject the cardinality counter into the registry.
    registry.op(
        CARDINALITY_KEY.clone(),
        |_| {},
        || Handle::Counter(Arc::clone(&cardinality_counter)),
    );

    // Initialize the controller.
    let controller = Controller {
        registry: Arc::clone(&registry),
    };
    // Register the controller globally.
    CONTROLLER
        .set(controller)
        .map_err(|_| "controller already initialized")?;

    // Initialize the recorder.
    let recorder = VectorRecorder {
        registry: Arc::clone(&registry),
        cardinality_counter: Arc::clone(&cardinality_counter),
    };
    // Apply a layer to capture tracing span fields as labels.
    let recorder = TracingContextLayer::new(VectorLabelFilter).layer(recorder);
    // Register the recorder globally.
    metrics::set_boxed_recorder(Box::new(recorder)).map_err(|_| "recorder already initialized")?;

    // Done.
    Ok(())
}

/// [`VectorRecorder`] is a [`metrics::Recorder`] implementation that's suitable
/// for the advanced usage that we have in Vector.
struct VectorRecorder {
    registry: Arc<Registry<CompositeKey, Handle>>,
    cardinality_counter: Arc<AtomicU64>,
}

impl VectorRecorder {
    fn bump_cardinality_counter_and<F, O>(&self, f: F) -> O
    where
        F: FnOnce() -> O,
    {
        self.cardinality_counter.fetch_add(1, Ordering::Relaxed);
        f()
    }
}

impl Recorder for VectorRecorder {
    fn register_counter(&self, key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::COUNTER, key);
        self.registry.op(
            ckey,
            |_| {},
            || self.bump_cardinality_counter_and(Handle::counter),
        )
    }
    fn register_gauge(&self, key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::GAUGE, key);
        self.registry.op(
            ckey,
            |_| {},
            || self.bump_cardinality_counter_and(Handle::gauge),
        )
    }
    fn register_histogram(
        &self,
        key: Key,
        _unit: Option<Unit>,
        _description: Option<&'static str>,
    ) {
        let ckey = CompositeKey::new(MetricKind::HISTOGRAM, key);
        self.registry.op(
            ckey,
            |_| {},
            || self.bump_cardinality_counter_and(Handle::histogram),
        )
    }

    fn increment_counter(&self, key: Key, value: u64) {
        let ckey = CompositeKey::new(MetricKind::COUNTER, key);
        self.registry.op(
            ckey,
            |handle| handle.increment_counter(value),
            || self.bump_cardinality_counter_and(Handle::counter),
        )
    }
    fn update_gauge(&self, key: Key, value: f64) {
        let ckey = CompositeKey::new(MetricKind::GAUGE, key);
        self.registry.op(
            ckey,
            |handle| handle.update_gauge(value),
            || self.bump_cardinality_counter_and(Handle::gauge),
        )
    }
    fn record_histogram(&self, key: Key, value: u64) {
        let ckey = CompositeKey::new(MetricKind::HISTOGRAM, key);
        self.registry.op(
            ckey,
            |handle| handle.record_histogram(value),
            || self.bump_cardinality_counter_and(Handle::histogram),
        )
    }
}

#[derive(Debug, Clone)]
struct VectorLabelFilter;

impl LabelFilter for VectorLabelFilter {
    fn should_include_label(&self, label: &Label) -> bool {
        let key = label.key();
        key == "component_name" || key == "component_type" || key == "component_kind"
    }
}

/// Controller allows capturing metric snapshots.
pub struct Controller {
    registry: Arc<Registry<CompositeKey, Handle>>,
}

/// Get a handle to the globally registered controller, if it's initialized.
pub fn get_controller() -> crate::Result<&'static Controller> {
    CONTROLLER
        .get()
        .ok_or_else(|| "metrics system not initialized".into())
}

fn snapshot(controller: &Controller) -> Vec<Event> {
    let handles = controller.registry.get_handles();
    handles
        .into_iter()
        .map(|(ck, (_generation, m))| {
            let (_, k) = ck.into_parts();
            Metric::from_metric_kv(k, m).into()
        })
        .collect()
}

/// Take a snapshot of all gathered metrics and expose them as metric
/// [`Event`]s.
pub fn capture_metrics(controller: &Controller) -> impl Iterator<Item = Event> {
    snapshot(controller).into_iter()
}

#[cfg(test)]
mod tests {
    use crate::test_util::trace_init;
    use metrics::counter;
    use tracing::{span, Level};

    #[ignore]
    #[test]
    fn test_labels_injection() {
        trace_init();
        let _ = super::init();

        let span = span!(
            Level::ERROR,
            "my span",
            component_name = "my_component_name",
            component_type = "my_component_type",
            component_kind = "my_component_kind",
            some_other_label = "qwerty"
        );
        // See https://github.com/tokio-rs/tracing/issues/978
        if span.is_disabled() {
            panic!("test is not configured properly, set TEST_LOG=info env var")
        }
        let _enter = span.enter();

        counter!("labels_injected_total", 1);

        let metric = super::capture_metrics(super::get_controller().unwrap())
            .map(|e| e.into_metric())
            .find(|metric| metric.name == "labels_injected_total")
            .unwrap();

        let expected_tags = Some(
            vec![
                ("component_name".to_owned(), "my_component_name".to_owned()),
                ("component_type".to_owned(), "my_component_type".to_owned()),
                ("component_kind".to_owned(), "my_component_kind".to_owned()),
            ]
            .into_iter()
            .collect(),
        );

        assert_eq!(metric.tags, expected_tags);
    }

    #[test]
    fn test_cardinality_metric() {
        trace_init();
        let _ = super::init();

        let capture_value = || {
            let metric = super::capture_metrics(super::get_controller().unwrap())
                .map(|e| e.into_metric())
                .find(|metric| metric.name == super::CARDINALITY_KEY_NAME)
                .unwrap();
            match metric.value {
                crate::event::MetricValue::Counter { value } => value,
                _ => panic!("invalid metric value type, expected coutner, got something else"),
            }
        };

        let intial_value = capture_value();

        counter!("cardinality_test_metric_1", 1);
        assert_eq!(capture_value(), intial_value + 1.0);

        counter!("cardinality_test_metric_1", 1);
        assert_eq!(capture_value(), intial_value + 1.0);

        counter!("cardinality_test_metric_2", 1);
        counter!("cardinality_test_metric_3", 1);
        assert_eq!(capture_value(), intial_value + 3.0);

        counter!("cardinality_test_metric_1", 1);
        counter!("cardinality_test_metric_2", 1);
        counter!("cardinality_test_metric_3", 1);
        assert_eq!(capture_value(), intial_value + 3.0);
    }
}

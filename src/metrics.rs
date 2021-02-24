use crate::{event::Metric, Event};
use dashmap::DashMap;
use metrics::{GaugeValue, Key, KeyData, Label, Recorder, SharedString, Unit};
use metrics_tracing_context::{LabelFilter, TracingContextLayer};
use metrics_util::layers::Layer;
use metrics_util::{CompositeKey, Handle, MetricKind};
use once_cell::sync::OnceCell;
use std::hash::Hash;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

static CONTROLLER: OnceCell<Controller> = OnceCell::new();

// Cardinality counter parameters, expose the internal metrics registry
// cardinality.
// Useful for the end users to help understand the characteristics of their
// environment and how vectors acts in it.
const CARDINALITY_KEY_NAME: &str = "internal_metrics_cardinality_total";
static CARDINALITY_KEY_DATA_NAME: [SharedString; 1] =
    [SharedString::const_str(&CARDINALITY_KEY_NAME)];
static CARDINALITY_KEY_DATA: KeyData = KeyData::from_static_name(&CARDINALITY_KEY_DATA_NAME);
static CARDINALITY_KEY: CompositeKey =
    CompositeKey::new(MetricKind::COUNTER, Key::Borrowed(&CARDINALITY_KEY_DATA));

fn metrics_enabled() -> bool {
    !matches!(std::env::var("DISABLE_INTERNAL_METRICS_CORE"), Ok(x) if x == "true")
}

fn tracing_context_layer_enabled() -> bool {
    !matches!(std::env::var("DISABLE_INTERNAL_METRICS_TRACING_INTEGRATION"), Ok(x) if x == "true")
}

pub fn init() -> crate::Result<()> {
    // An escape hatch to allow disabing internal metrics core.
    // May be used for performance reasons.
    // This is a hidden and undocumented functionality.
    if !metrics_enabled() {
        metrics::set_boxed_recorder(Box::new(metrics::NoopRecorder))
            .map_err(|_| "recorder already initialized")?;
        info!(message = "Internal metrics core is disabled.");
        return Ok(());
    }

    // Prepare the registry.
    let registry = VectorRegistry {
        map: DashMap::new(),
    };
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

    // If enabled, apply a layer to capture tracing span fields as labels.
    let recorder: Box<dyn metrics::Recorder> = if tracing_context_layer_enabled() {
        Box::new(TracingContextLayer::new(VectorLabelFilter).layer(recorder))
    } else {
        Box::new(recorder)
    };

    // Register the recorder globally.
    metrics::set_boxed_recorder(recorder).map_err(|_| "recorder already initialized")?;

    // Done.
    Ok(())
}

/// [`VectorRegistry`] is a vendored version of [`metrics_util::Registry`].
///
/// We are removing the generational wrappers that upstream added, as they
/// might've been the cause of the performance issues on the multi-core systems
/// under high paralellism.
///
/// The suspicion is that the atomics usage in the generational somehow causes
/// permanent cache invalidation starvation at some scenarios - however, it's
/// based on the empiric observations, and we currently don't have
/// a comprehensive mental model to back up this behaviour.
/// It was decided to just eliminate the generationals - for now.
/// Maybe in the long term too - we don't need them, so why pay the price?
/// They're not zero-cost.
#[derive(Debug)]
struct VectorRegistry<K, H>
where
    K: Eq + Hash + Clone + 'static,
    H: 'static,
{
    pub map: DashMap<K, H>,
}

impl<K, H> VectorRegistry<K, H>
where
    K: Eq + Hash + Clone + 'static,
    H: 'static,
{
    /// Perform an operation on a given key.
    ///
    /// The `op` function will be called for the handle under the given `key`.
    ///
    /// If the `key` is not already mapped, the `init` function will be
    /// called, and the resulting handle will be stored in the registry.
    pub fn op<I, O, V>(&self, key: K, op: O, init: I) -> V
    where
        I: FnOnce() -> H,
        O: FnOnce(&H) -> V,
    {
        let valref = self.map.entry(key).or_insert_with(init);
        op(valref.value())
    }
}

/// [`VectorRecorder`] is a [`metrics::Recorder`] implementation that's suitable
/// for the advanced usage that we have in Vector.
struct VectorRecorder {
    registry: Arc<VectorRegistry<CompositeKey, Handle>>,
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
    fn update_gauge(&self, key: Key, value: GaugeValue) {
        let ckey = CompositeKey::new(MetricKind::GAUGE, key);
        self.registry.op(
            ckey,
            |handle| handle.update_gauge(value),
            || self.bump_cardinality_counter_and(Handle::gauge),
        )
    }
    fn record_histogram(&self, key: Key, value: f64) {
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
    registry: Arc<VectorRegistry<CompositeKey, Handle>>,
}

/// Get a handle to the globally registered controller, if it's initialized.
pub fn get_controller() -> crate::Result<&'static Controller> {
    CONTROLLER
        .get()
        .ok_or_else(|| "metrics system not initialized".into())
}

fn snapshot(controller: &Controller) -> Vec<Event> {
    controller
        .registry
        .map
        .iter()
        .map(|valref| Metric::from_metric_kv(valref.key().key(), valref.value()).into())
        .collect()
}

/// Clear all metrics from the registry.
pub fn reset(controller: &Controller) {
    controller.registry.map.clear()
}

/// Take a snapshot of all gathered metrics and expose them as metric
/// [`Event`]s.
pub fn capture_metrics(controller: &Controller) -> impl Iterator<Item = Event> {
    snapshot(controller).into_iter()
}

#[cfg(test)]
mod tests {
    use crate::{event::Event, test_util::trace_init};
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
            .find(|metric| metric.name() == "labels_injected_total")
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

        assert_eq!(metric.tags(), expected_tags.as_ref());
    }

    #[test]
    fn test_cardinality_metric() {
        trace_init();
        let _ = super::init();

        let capture_value = || {
            let metric = super::capture_metrics(super::get_controller().unwrap())
                .map(Event::into_metric)
                .find(|metric| metric.name() == super::CARDINALITY_KEY_NAME)
                .unwrap();
            match metric.data.value {
                crate::event::MetricValue::Counter { value } => value,
                _ => panic!("invalid metric value type, expected coutner, got something else"),
            }
        };

        let intial_value = capture_value();

        counter!("cardinality_test_metric_1", 1);
        assert!(capture_value() >= intial_value + 1.0);

        counter!("cardinality_test_metric_1", 1);
        assert!(capture_value() >= intial_value + 1.0);

        counter!("cardinality_test_metric_2", 1);
        counter!("cardinality_test_metric_3", 1);
        assert!(capture_value() >= intial_value + 3.0);

        // Other tests could possibly increase the cardinality, so just
        // try adding the same test metrics a few times and fail only if
        // it keeps increasing.
        for count in 1..=10 {
            let start_value = capture_value();
            counter!("cardinality_test_metric_1", 1);
            counter!("cardinality_test_metric_2", 1);
            counter!("cardinality_test_metric_3", 1);
            let end_value = capture_value();
            assert!(end_value >= start_value);
            if start_value == end_value {
                break;
            }
            if count == 10 {
                panic!("Cardinality count still increasing after 10 loops!");
            }
        }
    }
}

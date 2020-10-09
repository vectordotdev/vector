use crate::{event::Metric, Event};
use metrics::{Key, Label, Recorder};
use metrics_tracing_context::{LabelFilter, TracingContextLayer};
use metrics_util::layers::Layer;
use metrics_util::{CompositeKey, Handle, MetricKind, Registry};
use once_cell::sync::OnceCell;
use std::sync::Arc;

static CONTROLLER: OnceCell<Controller> = OnceCell::new();

pub fn init() -> crate::Result<()> {
    // Prepare the registry.
    let registry = Registry::new();
    let registry = Arc::new(registry);

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
}

impl Recorder for VectorRecorder {
    fn register_counter(&self, key: Key, _description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::Counter, key);
        self.registry.op(ckey, |_| {}, Handle::counter)
    }
    fn register_gauge(&self, key: Key, _description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::Gauge, key);
        self.registry.op(ckey, |_| {}, Handle::gauge)
    }
    fn register_histogram(&self, key: Key, _description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::Histogram, key);
        self.registry.op(ckey, |_| {}, Handle::histogram)
    }

    fn increment_counter(&self, key: Key, value: u64) {
        let ckey = CompositeKey::new(MetricKind::Counter, key);
        self.registry.op(
            ckey,
            |handle| handle.increment_counter(value),
            Handle::counter,
        )
    }
    fn update_gauge(&self, key: Key, value: f64) {
        let ckey = CompositeKey::new(MetricKind::Gauge, key);
        self.registry
            .op(ckey, |handle| handle.update_gauge(value), Handle::gauge)
    }
    fn record_histogram(&self, key: Key, value: u64) {
        let ckey = CompositeKey::new(MetricKind::Histogram, key);
        self.registry.op(
            ckey,
            |handle| handle.record_histogram(value),
            Handle::histogram,
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
        .map(|(ck, m)| {
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

        counter!("labels_injected", 1);

        let metric = super::capture_metrics(super::get_controller().unwrap())
            .map(|e| e.into_metric())
            .find(|metric| metric.name == "labels_injected")
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
}

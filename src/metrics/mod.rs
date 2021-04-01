mod label_filter;
mod recorder;
mod registry;
#[cfg(test)]
mod tests;

use crate::metrics::label_filter::VectorLabelFilter;
use crate::metrics::recorder::{InnerRecorder, OuterRecorder};
use crate::metrics::registry::VectorRegistry;
use crate::{event::Metric, Event};
use metrics::{Key, KeyData, SharedString};
use metrics_tracing_context::TracingContextLayer;
use metrics_util::layers::Layer;
use metrics_util::{CompositeKey, Handle, MetricKind};
use once_cell::sync::OnceCell;
use std::sync::{atomic::AtomicU64, mpsc, Arc};
use std::thread;

static CONTROLLER: OnceCell<Controller> = OnceCell::new();
// Cardinality counter parameters, expose the internal metrics registry
// cardinality. Useful for the end users to help understand the characteristics
// of their environment and how vectors acts in it.
const CARDINALITY_KEY_NAME: &str = "internal_metrics_cardinality_total";
static CARDINALITY_KEY_DATA_NAME: [SharedString; 1] =
    [SharedString::const_str(&CARDINALITY_KEY_NAME)];
static CARDINALITY_KEY_DATA: KeyData = KeyData::from_static_name(&CARDINALITY_KEY_DATA_NAME);
static CARDINALITY_KEY: CompositeKey =
    CompositeKey::new(MetricKind::COUNTER, Key::Borrowed(&CARDINALITY_KEY_DATA));

/// Controller allows capturing metric snapshots.
pub struct Controller {
    registry: VectorRegistry<CompositeKey, Handle>,
}

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
    let registry = VectorRegistry::default();

    // Initialize the controller.
    let controller = Controller {
        registry: registry.clone(),
    };
    // Register the controller globally.
    CONTROLLER
        .set(controller)
        .map_err(|_| "controller already initialized")?;

    // Initialize the recorder.
    let (snd, rcv) = mpsc::channel();

    let inner_recorder = InnerRecorder::new(rcv, registry);
    // Create a thread to handle the loop for `InnerRecorder`. This populates
    // the `registry` from information provided by `OuterRecorder`, the
    // interface with metrics-rs.
    //
    // We immediately drop the handle and so the thread becomes a detatched
    // background thread.
    let _ = thread::Builder::new()
        .name("metrics_recorder".to_string())
        .spawn(|| inner_recorder.run())?;
    let recorder = OuterRecorder::new(snd);

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

/// Clear all metrics from the registry.
pub fn reset(controller: &Controller) {
    controller
        .registry
        .map
        .lock()
        .expect("metric map poisoned")
        .clear()
}

/// Take a snapshot of all gathered metrics and expose them as metric
/// [`Event`]s.
pub fn capture_metrics(controller: &Controller) -> impl Iterator<Item = Event> {
    snapshot(controller).into_iter()
}

/// Get a handle to the globally registered controller, if it's initialized.
pub fn get_controller() -> crate::Result<&'static Controller> {
    CONTROLLER
        .get()
        .ok_or_else(|| "metrics system not initialized".into())
}

fn snapshot(controller: &Controller) -> Vec<Event> {
    let mut events: Vec<Event> = controller
        .registry
        .map
        .lock()
        .expect("metric map poisoned")
        .iter()
        .map(|(k, v)| Metric::from_metric_kv(k.key(), v).into())
        .collect();
    let cardinality = Handle::Counter(Arc::new(AtomicU64::new(events.len() as u64)));
    let metric = Metric::from_metric_kv(CARDINALITY_KEY.key(), &cardinality);
    events.push(metric.into());
    events
}

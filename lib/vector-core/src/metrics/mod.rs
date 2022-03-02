mod ddsketch;
mod handle;
mod label_filter;
mod recorder;

use std::sync::Arc;

use metrics::Key;
use metrics_tracing_context::TracingContextLayer;
use metrics_util::{layers::Layer, Generational, NotTracked};
use once_cell::sync::OnceCell;
use snafu::Snafu;

pub use crate::metrics::{
    ddsketch::{AgentDDSketch, BinMap, Config},
    handle::{Counter, Handle},
};
use crate::{
    event::Metric,
    metrics::{label_filter::VectorLabelFilter, recorder::VectorRecorder},
};

pub(self) type Registry = metrics_util::Registry<Key, Handle, NotTracked<Handle>>;

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Snafu)]
pub enum Error {
    #[snafu(display("Recorder already initialized."))]
    AlreadyInitialized,
    #[snafu(display("Metrics system was not initialized."))]
    NotInitialized,
}

static CONTROLLER: OnceCell<Controller> = OnceCell::new();

// Cardinality counter parameters, expose the internal metrics registry
// cardinality. Useful for the end users to help understand the characteristics
// of their environment and how vectors acts in it.
const CARDINALITY_KEY_NAME: &str = "internal_metrics_cardinality_total";
static CARDINALITY_KEY: Key = Key::from_static_name(CARDINALITY_KEY_NAME);

/// Controller allows capturing metric snapshots.
pub struct Controller {
    recorder: VectorRecorder,
}

fn metrics_enabled() -> bool {
    !matches!(std::env::var("DISABLE_INTERNAL_METRICS_CORE"), Ok(x) if x == "true")
}

fn tracing_context_layer_enabled() -> bool {
    !matches!(std::env::var("DISABLE_INTERNAL_METRICS_TRACING_INTEGRATION"), Ok(x) if x == "true")
}

fn init(recorder: VectorRecorder) -> Result<()> {
    // An escape hatch to allow disabing internal metrics core. May be used for
    // performance reasons. This is a hidden and undocumented functionality.
    if !metrics_enabled() {
        metrics::set_boxed_recorder(Box::new(metrics::NoopRecorder))
            .map_err(|_| Error::AlreadyInitialized)?;
        info!(message = "Internal metrics core is disabled.");
        return Ok(());
    }

    ////
    //// Prepare the controller
    ////

    // The `Controller` is a safe spot in memory for us to stash a clone of the
    // registry -- where metrics are actually kept -- so that our sub-systems
    // interested in these metrics can grab copies. See `capture_metrics` and
    // its callers for an example.
    let controller = Controller {
        recorder: recorder.clone(),
    };
    CONTROLLER
        .set(controller)
        .map_err(|_| Error::AlreadyInitialized)?;

    ////
    //// Initialize the recorder.
    ////

    // The recorder is the interface between metrics-rs and our registry. In our
    // case it doesn't _do_ much other than shepherd into the registry and
    // update the cardinality counter, see above, as needed.
    let recorder: Box<dyn metrics::Recorder> = if tracing_context_layer_enabled() {
        // Apply a layer to capture tracing span fields as labels.
        Box::new(TracingContextLayer::new(VectorLabelFilter).layer(recorder))
    } else {
        Box::new(recorder)
    };

    // This where we combine metrics-rs and our registry. We box it to avoid
    // having to fiddle with statics ourselves.
    metrics::set_boxed_recorder(recorder).map_err(|_| Error::AlreadyInitialized)
}

/// Initialize the default metrics sub-system
///
/// # Errors
///
/// This function will error if it is called multiple times.
pub fn init_global() -> Result<()> {
    init(VectorRecorder::new_global())
}

/// Initialize the thread-local metrics sub-system
///
/// # Errors
///
/// This function will error if it is called multiple times.
pub fn init_test() -> Result<()> {
    init(VectorRecorder::new_test())
}

impl Controller {
    /// Clear all metrics from the registry.
    pub fn reset(&self) {
        self.recorder.with_registry(Registry::clear);
    }

    /// Get a handle to the globally registered controller, if it's initialized.
    ///
    /// # Errors
    ///
    /// This function will fail if the metrics subsystem has not been correctly
    /// initialized.
    pub fn get() -> Result<&'static Self> {
        CONTROLLER.get().ok_or(Error::NotInitialized)
    }

    /// Take a snapshot of all gathered metrics and expose them as metric
    /// [`Event`](crate::event::Event)s.
    pub fn capture_metrics(&self) -> Vec<Metric> {
        let mut metrics: Vec<Metric> = Vec::new();
        self.recorder.with_registry(|registry| {
            registry.visit(|_kind, (key, handle)| {
                metrics.push(Metric::from_metric_kv(key, handle.get_inner()));
            });
        });

        // Add alias `processed_events_total` for `component_sent_events_total`.
        for i in 0..metrics.len() {
            let metric = &metrics[i];
            if metric.name() == "component_sent_events_total" {
                let alias = metric.clone().with_name("processed_events_total");
                metrics.push(alias);
            }
        }

        let handle = Handle::Counter(Arc::new(Counter::with_count(metrics.len() as u64 + 1)));
        metrics.push(Metric::from_metric_kv(&CARDINALITY_KEY, &handle));

        metrics
    }
}

#[macro_export]
/// This macro is used to emit metrics as a `counter` while simultaneously
/// converting from absolute values to incremental values.
///
/// Values that do not arrive in strictly monotonically increasing order are
/// ignored and will not be emitted.
macro_rules! update_counter {
    ($label:literal, $value:expr) => {{
        use ::std::sync::atomic::{AtomicU64, Ordering};

        static PREVIOUS_VALUE: AtomicU64 = AtomicU64::new(0);

        let new_value = $value;
        let mut previous_value = PREVIOUS_VALUE.load(Ordering::Relaxed);

        loop {
            // Either a new greater value has been emitted before this thread updated the counter
            // or values were provided that are not in strictly monotonically increasing order.
            // Ignore.
            if new_value <= previous_value {
                break;
            }

            match PREVIOUS_VALUE.compare_exchange_weak(
                previous_value,
                new_value,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                // Another thread has written a new value before us. Re-enter loop.
                Err(value) => previous_value = value,
                // Calculate delta to last emitted value and emit it.
                Ok(_) => {
                    let delta = new_value - previous_value;
                    // Albeit very unlikely, note that this sequence of deltas might be emitted in
                    // a different order than they were calculated.
                    counter!($label, delta);
                    break;
                }
            }
        }
    }};
}

mod ddsketch;
mod label_filter;
mod recency;
mod recorder;
mod storage;

use std::{sync::OnceLock, time::Duration};

use chrono::Utc;
use metrics::Key;
use metrics_tracing_context::TracingContextLayer;
use metrics_util::layers::Layer;
use snafu::Snafu;

pub use self::ddsketch::{AgentDDSketch, BinMap, Config};
use self::{label_filter::VectorLabelFilter, recorder::Registry, recorder::VectorRecorder};
use crate::event::{Metric, MetricValue};

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Snafu)]
pub enum Error {
    #[snafu(display("Recorder already initialized."))]
    AlreadyInitialized,
    #[snafu(display("Metrics system was not initialized."))]
    NotInitialized,
    #[snafu(display("Timeout value of {} must be positive.", timeout))]
    TimeoutMustBePositive { timeout: f64 },
}

static CONTROLLER: OnceLock<Controller> = OnceLock::new();

// Cardinality counter parameters, expose the internal metrics registry
// cardinality. Useful for the end users to help understand the characteristics
// of their environment and how vectors acts in it.
const CARDINALITY_KEY_NAME: &str = "internal_metrics_cardinality";
static CARDINALITY_KEY: Key = Key::from_static_name(CARDINALITY_KEY_NAME);

// Older deprecated counter key name
const CARDINALITY_COUNTER_KEY_NAME: &str = "internal_metrics_cardinality_total";
static CARDINALITY_COUNTER_KEY: Key = Key::from_static_name(CARDINALITY_COUNTER_KEY_NAME);

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
    // An escape hatch to allow disabling internal metrics core. May be used for
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

/// Initialize the thread-local metrics sub-system. This function will loop until a recorder is
/// actually set.
pub fn init_test() {
    if init(VectorRecorder::new_test()).is_err() {
        // The only error case returned by `init` is `AlreadyInitialized`. A race condition is
        // possible here: if metrics are being initialized by two (or more) test threads
        // simultaneously, the ones that fail to set return immediately, possibly allowing
        // subsequent code to execute before the static recorder value is actually set within the
        // `metrics` crate. To prevent subsequent code from running with an unset recorder, loop
        // here until a recorder is available.
        while metrics::try_recorder().is_none() {}
    }
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

    /// Set or clear the expiry time after which idle metrics are dropped from the set of captured
    /// metrics. Invalid timeouts (zero or negative values) are silently remapped to no expiry.
    ///
    /// # Errors
    ///
    /// The contained timeout value must be positive.
    pub fn set_expiry(&self, timeout: Option<f64>) -> Result<()> {
        if let Some(timeout) = timeout {
            if timeout <= 0.0 {
                return Err(Error::TimeoutMustBePositive { timeout });
            }
        }
        self.recorder
            .with_registry(|registry| registry.set_expiry(timeout.map(Duration::from_secs_f64)));
        Ok(())
    }

    /// Take a snapshot of all gathered metrics and expose them as metric
    /// [`Event`](crate::event::Event)s.
    pub fn capture_metrics(&self) -> Vec<Metric> {
        let timestamp = Utc::now();

        let mut metrics = self.recorder.with_registry(Registry::visit_metrics);

        #[allow(clippy::cast_precision_loss)]
        let value = (metrics.len() + 2) as f64;
        metrics.push(Metric::from_metric_kv(
            &CARDINALITY_KEY,
            MetricValue::Gauge { value },
            timestamp,
        ));
        metrics.push(Metric::from_metric_kv(
            &CARDINALITY_COUNTER_KEY,
            MetricValue::Counter { value },
            timestamp,
        ));

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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::event::MetricKind;

    const IDLE_TIMEOUT: f64 = 0.5;

    fn init_metrics() -> &'static Controller {
        init_test();
        Controller::get().expect("Could not get global metrics controller")
    }

    #[test]
    fn cardinality_matches() {
        for cardinality in [0, 1, 10, 100, 1000, 10000] {
            init_test();
            let controller = Controller::get().unwrap();
            controller.reset();

            for idx in 0..cardinality {
                metrics::counter!("test", 1, "idx" => idx.to_string());
            }

            let metrics = controller.capture_metrics();
            assert_eq!(metrics.len(), cardinality + 2);

            #[allow(clippy::cast_precision_loss)]
            let value = metrics.len() as f64;
            for metric in metrics {
                match metric.name() {
                    CARDINALITY_KEY_NAME => {
                        assert_eq!(metric.value(), &MetricValue::Gauge { value });
                        assert_eq!(metric.kind(), MetricKind::Absolute);
                    }
                    CARDINALITY_COUNTER_KEY_NAME => {
                        assert_eq!(metric.value(), &MetricValue::Counter { value });
                        assert_eq!(metric.kind(), MetricKind::Absolute);
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn handles_registered_metrics() {
        let controller = init_metrics();

        let counter = metrics::register_counter!("test7");
        assert_eq!(controller.capture_metrics().len(), 3);
        counter.increment(1);
        assert_eq!(controller.capture_metrics().len(), 3);
        let gauge = metrics::register_gauge!("test8");
        assert_eq!(controller.capture_metrics().len(), 4);
        gauge.set(1.0);
        assert_eq!(controller.capture_metrics().len(), 4);
    }

    #[test]
    fn expires_metrics() {
        let controller = init_metrics();
        controller.set_expiry(Some(IDLE_TIMEOUT)).unwrap();

        metrics::counter!("test2", 1);
        metrics::counter!("test3", 2);
        assert_eq!(controller.capture_metrics().len(), 4);

        std::thread::sleep(Duration::from_secs_f64(IDLE_TIMEOUT * 2.0));
        metrics::counter!("test2", 3);
        assert_eq!(controller.capture_metrics().len(), 3);
    }

    #[test]
    fn expires_metrics_tags() {
        let controller = init_metrics();
        controller.set_expiry(Some(IDLE_TIMEOUT)).unwrap();

        metrics::counter!("test4", 1, "tag" => "value1");
        metrics::counter!("test4", 2, "tag" => "value2");
        assert_eq!(controller.capture_metrics().len(), 4);

        std::thread::sleep(Duration::from_secs_f64(IDLE_TIMEOUT * 2.0));
        metrics::counter!("test4", 3, "tag" => "value1");
        assert_eq!(controller.capture_metrics().len(), 3);
    }

    #[test]
    fn skips_expiring_registered() {
        let controller = init_metrics();
        controller.set_expiry(Some(IDLE_TIMEOUT)).unwrap();

        let a = metrics::register_counter!("test5");
        metrics::counter!("test6", 5);
        assert_eq!(controller.capture_metrics().len(), 4);
        a.increment(1);
        assert_eq!(controller.capture_metrics().len(), 4);

        std::thread::sleep(Duration::from_secs_f64(IDLE_TIMEOUT * 2.0));
        assert_eq!(controller.capture_metrics().len(), 3);

        a.increment(1);
        let metrics = controller.capture_metrics();
        assert_eq!(metrics.len(), 3);
        let metric = metrics
            .into_iter()
            .find(|metric| metric.name() == "test5")
            .expect("Test metric is not present");
        match metric.value() {
            MetricValue::Counter { value } => assert_eq!(*value, 2.0),
            value => panic!("Invalid metric value {value:?}"),
        }
    }
}

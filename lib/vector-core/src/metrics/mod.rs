mod ddsketch;
mod label_filter;
mod recency;
mod recorder;
mod storage;

use std::{future::Future, pin::Pin, sync::OnceLock, task::Context, task::Poll, time::Duration};

use chrono::Utc;
use metrics::Key;
use metrics_tracing_context::TracingContextLayer;
use metrics_util::layers::Layer;
use snafu::Snafu;

pub use self::ddsketch::{AgentDDSketch, BinMap, Config};
use self::{label_filter::VectorLabelFilter, recorder::VectorRecorder};
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

static GLOBAL_CONTROLLER: OnceLock<Controller> = OnceLock::new();

// Cardinality counter parameters, expose the internal metrics registry
// cardinality. Useful for the end users to help understand the characteristics
// of their environment and how vectors acts in it.
const CARDINALITY_KEY_NAME: &str = "internal_metrics_cardinality";
static CARDINALITY_KEY: Key = Key::from_static_name(CARDINALITY_KEY_NAME);

// Older deprecated counter key name
const CARDINALITY_COUNTER_KEY_NAME: &str = "internal_metrics_cardinality_total";
static CARDINALITY_COUNTER_KEY: Key = Key::from_static_name(CARDINALITY_COUNTER_KEY_NAME);

/// Controller allows capturing metric snapshots.
#[derive(Clone)]
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
        metrics::set_global_recorder(metrics::NoopRecorder)
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
    let Ok(()) = GLOBAL_CONTROLLER.set(controller) else {
        return Err(Error::AlreadyInitialized);
    };

    ////
    //// Initialize the recorder.
    ////

    // The recorder is the interface between metrics-rs and our registry. In our
    // case it doesn't _do_ much other than shepherd into the registry and
    // update the cardinality counter, see above, as needed.
    if tracing_context_layer_enabled() {
        // Apply a layer to capture tracing span fields as labels.
        metrics::set_global_recorder(TracingContextLayer::new(VectorLabelFilter).layer(recorder))
            .map_err(|_| Error::AlreadyInitialized)
    } else {
        metrics::set_global_recorder(recorder).map_err(|_| Error::AlreadyInitialized)
    }
}

/// Initialize the default metrics sub-system
///
/// # Errors
///
/// This function will error if it is called multiple times.
pub fn init_global() -> Result<()> {
    init(VectorRecorder::new())
}

/// Run the given function in the context of a new local test recorder.
pub fn with_test_recorder<T>(doit: impl FnOnce(Controller) -> T) -> T {
    let recorder = VectorRecorder::new();
    let controller = Controller {
        recorder: recorder.clone(),
    };
    metrics::with_local_recorder(&recorder, || doit(controller))
}

/// Run the given async function in the context of a new local test recorder. Returns a new `Future`
/// and so must be `.await`ed to complete the execution.
#[must_use]
pub fn with_test_recorder_async<F>(doit: impl FnOnce(Controller) -> F) -> TestFutureWrapper<F> {
    let recorder = VectorRecorder::new();
    let future = doit(Controller {
        recorder: recorder.clone(),
    });
    TestFutureWrapper { recorder, future }
}

/// A wrapper for a `Future` that is being executed in the context of a local test recorder.
#[pin_project::pin_project]
pub struct TestFutureWrapper<F> {
    recorder: VectorRecorder,
    #[pin]
    future: F,
}

impl<F: Future> Future for TestFutureWrapper<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        metrics::with_local_recorder(this.recorder, || this.future.poll(ctx))
    }
}

impl Controller {
    /// Clear all metrics from the registry.
    pub fn reset(&self) {
        self.recorder.registry().clear();
    }

    /// Get a handle to the globally registered controller, if it's initialized.
    ///
    /// # Errors
    ///
    /// This function will fail if the metrics subsystem has not been correctly
    /// initialized.
    pub fn get_global() -> Result<&'static Self> {
        GLOBAL_CONTROLLER.get().ok_or(Error::NotInitialized)
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
            .registry()
            .set_expiry(timeout.map(Duration::from_secs_f64));
        Ok(())
    }

    /// Take a snapshot of all gathered metrics and expose them as metric
    /// [`Event`](crate::event::Event)s.
    pub fn capture_metrics(&self) -> Vec<Metric> {
        let timestamp = Utc::now();

        let mut metrics = self.recorder.registry().visit_metrics();

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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::event::MetricKind;

    const IDLE_TIMEOUT: f64 = 0.5;

    #[test]
    fn cardinality_matches() {
        for cardinality in [0, 1, 10, 100, 1000, 10000] {
            with_test_recorder(|controller| {
                for idx in 0..cardinality {
                    metrics::counter!("test", "idx" => idx.to_string()).increment(1);
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
            });
        }
    }

    #[test]
    fn handles_registered_metrics() {
        with_test_recorder(|controller| {
            let counter = metrics::counter!("test7");
            assert_eq!(controller.capture_metrics().len(), 3);
            counter.increment(1);
            assert_eq!(controller.capture_metrics().len(), 3);
            let gauge = metrics::gauge!("test8");
            assert_eq!(controller.capture_metrics().len(), 4);
            gauge.set(1.0);
            assert_eq!(controller.capture_metrics().len(), 4);
        });
    }

    #[test]
    fn expires_metrics() {
        with_test_recorder(|controller| {
            controller.set_expiry(Some(IDLE_TIMEOUT)).unwrap();

            metrics::counter!("test2").increment(1);
            metrics::counter!("test3").increment(2);
            assert_eq!(controller.capture_metrics().len(), 4);

            std::thread::sleep(Duration::from_secs_f64(IDLE_TIMEOUT * 2.0));
            metrics::counter!("test2").increment(3);
            assert_eq!(controller.capture_metrics().len(), 3);
        });
    }

    #[test]
    fn expires_metrics_tags() {
        with_test_recorder(|controller| {
            controller.set_expiry(Some(IDLE_TIMEOUT)).unwrap();

            metrics::counter!("test4", "tag" => "value1").increment(1);
            metrics::counter!("test4", "tag" => "value2").increment(2);
            assert_eq!(controller.capture_metrics().len(), 4);

            std::thread::sleep(Duration::from_secs_f64(IDLE_TIMEOUT * 2.0));
            metrics::counter!("test4", "tag" => "value1").increment(3);
            assert_eq!(controller.capture_metrics().len(), 3);
        });
    }

    #[test]
    fn skips_expiring_registered() {
        with_test_recorder(|controller| {
            controller.set_expiry(Some(IDLE_TIMEOUT)).unwrap();

            let a = metrics::counter!("test5");
            metrics::counter!("test6").increment(5);
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
        });
    }
}

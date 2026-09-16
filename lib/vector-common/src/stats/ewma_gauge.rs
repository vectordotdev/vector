use std::sync::{Arc, Mutex};
use std::time::Instant;

use metrics::Gauge;

use super::{AtomicEwma, TimeEwma};

/// Couples a [`Gauge`] with an [`AtomicEwma`] so gauge readings reflect the EWMA.
#[derive(Clone, Debug)]
pub struct EwmaGauge {
    gauge: Gauge,
    // Note that the `Gauge` internally is equivalent to an `Arc<AtomicF64>` so we need to use the
    // same semantics for the EWMA calculation as well.
    ewma: Arc<AtomicEwma>,
}

impl EwmaGauge {
    #[must_use]
    pub fn new(gauge: Gauge, alpha: Option<f64>) -> Self {
        let alpha = alpha.unwrap_or(super::DEFAULT_EWMA_ALPHA);
        let ewma = Arc::new(AtomicEwma::new(alpha));
        Self { gauge, ewma }
    }

    /// Records a new value, updates the EWMA, and sets the gauge accordingly.
    pub fn record(&self, value: f64) {
        let average = self.ewma.update(value);
        self.gauge.set(average);
    }
}

/// Couples a [`Gauge`] with a [`TimeEwma`] so gauge readings reflect the EWMA. Since `TimeEwma` has
/// an internal state consisting of multiple values, this gauge requires a mutex to protect the
/// state update.
#[derive(Clone, Debug)]
pub struct TimeEwmaGauge {
    gauge: Gauge,
    ewma: Arc<Mutex<TimeEwma>>,
}

impl TimeEwmaGauge {
    #[must_use]
    pub fn new(gauge: Gauge, half_life_seconds: f64) -> Self {
        let ewma = Arc::new(Mutex::new(TimeEwma::new(half_life_seconds)));
        Self { gauge, ewma }
    }

    /// Records a new value, updates the EWMA, and sets the gauge accordingly.
    ///
    /// # Panics
    ///
    /// Panics if the EWMA mutex is poisoned.
    pub fn record(&self, value: f64, reference: Instant) {
        let mut ewma = self.ewma.lock().expect("time ewma gauge mutex poisoned");
        let average = ewma.update(value, reference);
        self.gauge.set(average);
    }
}

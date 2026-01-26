use std::sync::Arc;

use metrics::Gauge;

use super::AtomicEwma;

/// The default alpha parameter used when constructing EWMA-backed gauges.
pub const DEFAULT_EWMA_ALPHA: f64 = 0.9;

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
        let alpha = alpha.unwrap_or(DEFAULT_EWMA_ALPHA);
        let ewma = Arc::new(AtomicEwma::new(alpha));
        Self { gauge, ewma }
    }

    /// Records a new value, updates the EWMA, and sets the gauge accordingly.
    pub fn record(&self, value: f64) {
        let average = self.ewma.update(value);
        self.gauge.set(average);
    }
}

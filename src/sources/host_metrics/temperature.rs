use vector_lib::metric_tags;

use super::HostMetrics;

const COMPONENT: &str = "component";
const TEMPERATURE_CELSIUS: &str = "temperature_celsius";
const TEMPERATURE_MAX_CELSIUS: &str = "temperature_max_celsius";
const TEMPERATURE_CRITICAL_CELSIUS: &str = "temperature_critical_celsius";

impl HostMetrics {
    pub async fn temperature_metrics(&mut self, output: &mut super::MetricsBuffer) {
        output.name = "temperature";
        // Refresh the long-lived component list in place. `Component::max()` is
        // derived by sysinfo from successive refreshes when the sensor does not
        // expose a hardware maximum, so recreating the list every scrape (as a
        // fresh `Components::new_with_refreshed_list()` would) resets that
        // history and makes the reported max equal the latest sample.
        self.components.refresh(true);
        for component in &self.components {
            // Some sensors expose an empty label (for example when sysinfo falls
            // back to `/sys/class/thermal`); use the component id as a fallback
            // so distinct sensors are not collapsed into a single series.
            let label = if component.label().is_empty() {
                component.id().unwrap_or_default()
            } else {
                component.label()
            };
            let tags = || metric_tags!(COMPONENT => label);
            // Skip non-finite readings: sysinfo can return `NaN` when a sensor
            // file exists but the read fails, and downstream sinks reject NaN.
            if let Some(temperature) = component.temperature().filter(|t| t.is_finite()) {
                output.gauge(TEMPERATURE_CELSIUS, temperature as f64, tags());
            }
            if let Some(max) = component.max().filter(|m| m.is_finite()) {
                output.gauge(TEMPERATURE_MAX_CELSIUS, max as f64, tags());
            }
            if let Some(critical) = component.critical().filter(|c| c.is_finite()) {
                output.gauge(TEMPERATURE_CRITICAL_CELSIUS, critical as f64, tags());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{HostMetrics, HostMetricsConfig, MetricsBuffer, tests::all_gauges},
        COMPONENT,
    };

    #[tokio::test]
    async fn generates_temperature_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .temperature_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;

        // Temperature sensors are not exposed in many environments (containers,
        // virtual machines, CI runners), so the component list can legitimately
        // be empty. When metrics are produced, they must all be gauges named
        // `temperature*` and carry the `component` tag.
        assert!(all_gauges(&metrics));
        for metric in &metrics {
            assert!(
                metric.name().starts_with("temperature"),
                "unexpected metric name: {}",
                metric.name()
            );
            assert!(
                metric
                    .tags()
                    .expect("temperature metric is missing tags")
                    .contains_key(COMPONENT),
                "temperature metric is missing the `component` tag"
            );
        }
    }
}

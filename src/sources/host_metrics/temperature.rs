use sysinfo::Components;
use vector_lib::metric_tags;

use super::HostMetrics;

const COMPONENT: &str = "component";
const TEMPERATURE_CELSIUS: &str = "temperature_celsius";
const TEMPERATURE_MAX_CELSIUS: &str = "temperature_max_celsius";
const TEMPERATURE_CRITICAL_CELSIUS: &str = "temperature_critical_celsius";

impl HostMetrics {
    pub async fn temperature_metrics(&self, output: &mut super::MetricsBuffer) {
        output.name = "temperature";
        let components = Components::new_with_refreshed_list();
        for component in &components {
            let label = component.label();
            let tags = || metric_tags!(COMPONENT => label);
            if let Some(temperature) = component.temperature() {
                output.gauge(TEMPERATURE_CELSIUS, temperature as f64, tags());
            }
            if let Some(max) = component.max() {
                output.gauge(TEMPERATURE_MAX_CELSIUS, max as f64, tags());
            }
            if let Some(critical) = component.critical() {
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

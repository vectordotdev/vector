use crate::internal_events::{HostMetricsScrapeDetailError, HostMetricsScrapeError};
use futures::StreamExt;
#[cfg(target_os = "linux")]
use heim::cpu::os::linux::CpuTimeExt;
use heim::units::time::second;
use vector_lib::{event::MetricTags, metric_tags};

use super::{filter_result, HostMetrics};

const MODE: &str = "mode";
const CPU_SECS_TOTAL: &str = "cpu_seconds_total";
const LOGICAL_CPUS: &str = "logical_cpus";
const PHYSICAL_CPUS: &str = "physical_cpus";

impl HostMetrics {
    pub async fn cpu_metrics(&self, output: &mut super::MetricsBuffer) {
        // adds the metrics from cpu time for each cpu
        match heim::cpu::times().await {
            Ok(times) => {
                let times: Vec<_> = times
                    .filter_map(|result| filter_result(result, "Failed to load/parse CPU time."))
                    .collect()
                    .await;
                output.name = "cpu";
                for (index, times) in times.into_iter().enumerate() {
                    let tags = |name: &str| metric_tags!(MODE => name, "cpu" => index.to_string());
                    output.counter(CPU_SECS_TOTAL, times.idle().get::<second>(), tags("idle"));
                    #[cfg(target_os = "linux")]
                    output.counter(
                        CPU_SECS_TOTAL,
                        times.io_wait().get::<second>(),
                        tags("io_wait"),
                    );
                    #[cfg(target_os = "linux")]
                    output.counter(CPU_SECS_TOTAL, times.nice().get::<second>(), tags("nice"));
                    output.counter(
                        CPU_SECS_TOTAL,
                        times.system().get::<second>(),
                        tags("system"),
                    );
                    output.counter(CPU_SECS_TOTAL, times.user().get::<second>(), tags("user"));
                }
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load CPU times.",
                    error,
                });
            }
        }
        // adds the logical cpu count gauge
        match heim::cpu::logical_count().await {
            Ok(count) => output.gauge(LOGICAL_CPUS, count as f64, MetricTags::default()),
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load logical CPU count.",
                    error,
                });
            }
        }
        // adds the physical cpu count gauge
        match heim::cpu::physical_count().await {
            Ok(Some(count)) => output.gauge(PHYSICAL_CPUS, count as f64, MetricTags::default()),
            Ok(None) => {
                emit!(HostMetricsScrapeError {
                    message: "Unable to determine physical CPU count.",
                });
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load physical CPU count.",
                    error,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{HostMetrics, HostMetricsConfig, MetricsBuffer};
    use super::{CPU_SECS_TOTAL, LOGICAL_CPUS, MODE, PHYSICAL_CPUS};

    #[tokio::test]
    async fn generates_cpu_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .cpu_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;

        assert!(!metrics.is_empty());

        let mut n_physical_cpus = 0;
        let mut n_logical_cpus = 0;

        for metric in metrics {
            // the cpu_seconds_total metrics must have mode
            if metric.name() == CPU_SECS_TOTAL {
                let tags = metric.tags();
                assert!(
                    tags.is_some(),
                    "Metric cpu_seconds_total must have a mode tag"
                );
                let tags = tags.unwrap();
                assert!(
                    tags.contains_key(MODE),
                    "Metric cpu_seconds_total must have a mode tag"
                );
            } else if metric.name() == PHYSICAL_CPUS {
                n_physical_cpus += 1;
            } else if metric.name() == LOGICAL_CPUS {
                n_logical_cpus += 1;
            } else {
                // catch any bogey
                panic!("unrecognized metric name");
            }
        }

        // cpu count metrics should each be present once
        assert_eq!(
            n_logical_cpus, 1,
            "There can only be one! (logical_cpus metric)"
        );
        assert_eq!(
            n_physical_cpus, 1,
            "There can only be one! (physical_cpus metric)"
        );
    }
}

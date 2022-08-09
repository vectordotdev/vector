use std::collections::BTreeMap;

use futures::StreamExt;
#[cfg(target_os = "linux")]
use heim::cpu::os::linux::CpuTimeExt;
use heim::units::time::second;
use vector_common::btreemap;

use super::{filter_result, HostMetrics};

const MODE: &str = "mode";
const NAME: &str = "cpu_seconds_total";
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
                    let tags = |name: &str| {
                        BTreeMap::from([
                            (String::from(MODE), String::from(name)),
                            (String::from("cpu"), index.to_string()),
                        ])
                    };
                    output.counter(NAME, times.idle().get::<second>(), tags("idle"));
                    #[cfg(target_os = "linux")]
                    output.counter(NAME, times.io_wait().get::<second>(), tags("io_wait"));
                    #[cfg(target_os = "linux")]
                    output.counter(NAME, times.nice().get::<second>(), tags("nice"));
                    output.counter(NAME, times.system().get::<second>(), tags("system"));
                    output.counter(NAME, times.user().get::<second>(), tags("user"));
                }
            }
            Err(error) => {
                error!(message = "Failed to load CPU times.", %error, internal_log_rate_secs = 60);
            }
        }
        // adds the logical cpu count gauge
        match heim::cpu::logical_count().await {
            Ok(count) => output.gauge(
                NAME,
                count as f64,
                btreemap! {
                    MODE => LOGICAL_CPUS,
                },
            ),
            Err(error) => {
                error!(message = "Failed to load logical CPU count.", %error, internal_log_rate_secs = 60);
            }
        }
        // adds the physical cpu count gauge
        match heim::cpu::physical_count().await {
            Ok(Some(count)) => output.gauge(
                NAME,
                count as f64,
                btreemap! {
                    MODE => PHYSICAL_CPUS
                },
            ),
            Ok(None) => {
                error!(
                    message = "Unable to determine physical CPU count.",
                    internal_log_rate_secs = 60
                );
            }
            Err(error) => {
                error!(message = "Failed to load physical CPU count.", %error, internal_log_rate_secs = 60);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        tests::{count_name, count_tag},
        HostMetrics, HostMetricsConfig, MetricsBuffer,
    };
    use super::{LOGICAL_CPUS, MODE, NAME, PHYSICAL_CPUS};

    #[tokio::test]
    async fn generates_cpu_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .cpu_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;

        assert!(!metrics.is_empty());

        // They should all be named cpu_seconds_total
        assert_eq!(metrics.len(), count_name(&metrics, NAME));

        // They should all have a "mode" tag
        assert_eq!(count_tag(&metrics, MODE), metrics.len());

        // cpu count metrics should be present
        let mut iter = metrics.iter();
        assert!(iter.any(|metric| { metric.tag_matches(MODE, LOGICAL_CPUS) }));
        assert!(iter.any(|metric| { metric.tag_matches(MODE, PHYSICAL_CPUS) }));
    }
}

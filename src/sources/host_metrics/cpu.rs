use std::collections::BTreeMap;

use futures::StreamExt;
#[cfg(target_os = "linux")]
use heim::cpu::os::linux::CpuTimeExt;
use heim::units::time::second;

use super::{filter_result, HostMetrics};

const NAME: &str = "cpu_seconds_total";

impl HostMetrics {
    pub async fn cpu_metrics(&self, output: &mut super::MetricsBuffer) {
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
                            (String::from("mode"), String::from(name)),
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
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        tests::{all_counters, count_name, count_tag},
        HostMetrics, HostMetricsConfig, MetricsBuffer,
    };

    #[tokio::test]
    async fn generates_cpu_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .cpu_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;

        assert!(!metrics.is_empty());
        assert!(all_counters(&metrics));

        // They should all be named cpu_seconds_total
        assert_eq!(metrics.len(), count_name(&metrics, "cpu_seconds_total"));

        // They should all have a "mode" tag
        assert_eq!(count_tag(&metrics, "mode"), metrics.len());
    }
}

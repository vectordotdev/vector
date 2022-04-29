use chrono::Utc;
use futures::{stream, StreamExt};
#[cfg(target_os = "linux")]
use heim::cpu::os::linux::CpuTimeExt;
use heim::units::time::second;
use vector_common::btreemap;

use super::{filter_result, HostMetrics};
use crate::event::metric::Metric;

impl HostMetrics {
    pub async fn cpu_metrics(&self) -> Vec<Metric> {
        match heim::cpu::times().await {
            Ok(times) => {
                times
                    .filter_map(|result| filter_result(result, "Failed to load/parse CPU time."))
                    .enumerate()
                    .map(|(index, times)| {
                        let timestamp = Utc::now();
                        let name = "cpu_seconds_total";
                        stream::iter(
                            vec![
                                self.counter(
                                    name,
                                    timestamp,
                                    times.idle().get::<second>(),
                                    btreemap! { "mode" => "idle", "cpu" => index.to_string() },
                                ),
                                #[cfg(target_os = "linux")]
                                self.counter(
                                    name,
                                    timestamp,
                                    times.nice().get::<second>(),
                                    btreemap! { "mode" => "nice", "cpu" => index.to_string() },
                                ),
                                self.counter(
                                    name,
                                    timestamp,
                                    times.system().get::<second>(),
                                    btreemap! { "mode" => "system", "cpu" => index.to_string() },
                                ),
                                self.counter(
                                    name,
                                    timestamp,
                                    times.user().get::<second>(),
                                    btreemap! { "mode" => "user", "cpu" => index.to_string() },
                                ),
                            ]
                            .into_iter(),
                        )
                    })
                    .flatten()
                    .collect::<Vec<_>>()
                    .await
            }
            Err(error) => {
                error!(message = "Failed to load CPU times.", %error, internal_log_rate_secs = 60);
                vec![]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        tests::{all_counters, count_name, count_tag},
        HostMetrics, HostMetricsConfig,
    };

    #[tokio::test]
    async fn generates_cpu_metrics() {
        let metrics = HostMetrics::new(HostMetricsConfig::default())
            .cpu_metrics()
            .await;
        assert!(!metrics.is_empty());
        assert!(all_counters(&metrics));

        // They should all be named cpu_seconds_total
        assert_eq!(metrics.len(), count_name(&metrics, "cpu_seconds_total"));

        // They should all have a "mode" tag
        assert_eq!(count_tag(&metrics, "mode"), metrics.len());
    }
}

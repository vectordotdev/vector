use std::ffi::OsStr;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};
#[cfg(target_os = "linux")]
use vector_lib::metric_tags;
use vector_lib::{
    configurable::configurable_component,
    internal_event::{CounterName, GaugeName},
};

use super::{FilterList, HostMetrics, default_all_processes, example_processes};

/// Options for the process metrics collector.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct ProcessConfig {
    /// Lists of process name patterns to include or exclude.
    #[serde(default = "default_all_processes")]
    #[configurable(metadata(docs::examples = "example_processes()"))]
    processes: FilterList,
}

const RUNTIME: CounterName = CounterName::ProcessRuntime;
const CPU_USAGE: GaugeName = GaugeName::ProcessCpuUsage;
const MEMORY_USAGE: GaugeName = GaugeName::ProcessMemoryUsage;
const MEMORY_VIRTUAL_USAGE: GaugeName = GaugeName::ProcessMemoryVirtualUsage;

impl HostMetrics {
    pub fn process_metrics(&mut self, output: &mut super::MetricsBuffer) {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::default()
                .with_memory()
                .with_cpu()
                .with_cmd(UpdateKind::OnlyIfNotSet),
        );
        output.name = "process";
        let sep = OsStr::new(" ");
        for (pid, process) in self.system.processes().iter().filter(|&(_, proc)| {
            self.config
                .process
                .processes
                .contains_str(proc.name().to_str())
        }) {
            let tags = || {
                metric_tags!(
                "pid" => pid.as_u32().to_string(),
                "name" => process.name().to_str().unwrap_or("unknown"),
                "command" => process.cmd().join(sep).to_str().unwrap_or(""))
            };
            output.gauge(CPU_USAGE, process.cpu_usage().into(), tags());
            output.gauge(MEMORY_USAGE, process.memory() as f64, tags());
            output.gauge(
                MEMORY_VIRTUAL_USAGE,
                process.virtual_memory() as f64,
                tags(),
            );
            output.counter(RUNTIME, process.run_time() as f64, tags());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{HostMetrics, HostMetricsConfig, MetricsBuffer};
    use crate::sources::host_metrics::tests::count_tag;

    #[tokio::test]
    async fn generates_process_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default()).process_metrics(&mut buffer);
        let metrics = buffer.into_metrics();
        assert!(!metrics.is_empty());

        // All metrics are named process_*
        assert!(
            !metrics
                .iter()
                .any(|metric| !metric.name().starts_with("process_"))
        );

        // They should all have the required tag
        assert_eq!(count_tag(&metrics, "pid"), metrics.len());
        assert_eq!(count_tag(&metrics, "name"), metrics.len());
        assert_eq!(count_tag(&metrics, "command"), metrics.len());
    }
}

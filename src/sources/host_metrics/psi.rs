use std::io::{self, BufRead};
use std::path::Path;

use procfs::prelude::*;
use procfs::{CpuPressure, IoPressure, MemoryPressure, PressureRecord};
use vector_lib::event::MetricTags;

use super::{HostMetrics, MetricsBuffer};
use crate::internal_events::HostMetricsScrapeDetailError;

const PSI_AVG10: &str = "psi_avg10";
const PSI_AVG60: &str = "psi_avg60";
const PSI_AVG300: &str = "psi_avg300";
const PSI_TOTAL: &str = "psi_total";
const RESOURCE: &str = "resource";
const LEVEL: &str = "level";

impl HostMetrics {
    pub async fn psi_metrics(&self, output: &mut MetricsBuffer) {
        let result = tokio::task::spawn_blocking(collect_psi_stats)
            .await
            .unwrap_or_else(|join_error| {
                Err(PsiError(format!(
                    "Failed to join blocking task: {}",
                    join_error
                )))
            });

        match result {
            Ok(stats) => {
                output.name = "psi";
                emit_pressure_record(output, &stats.cpu_some, "cpu", "some");
                emit_pressure_record(output, &stats.cpu_full, "cpu", "full");
                emit_pressure_record(output, &stats.memory_some, "memory", "some");
                emit_pressure_record(output, &stats.memory_full, "memory", "full");
                emit_pressure_record(output, &stats.io_some, "io", "some");
                emit_pressure_record(output, &stats.io_full, "io", "full");
                if let Some(ref record) = stats.irq_full {
                    emit_pressure_record(output, record, "irq", "full");
                }
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load PSI info.",
                    error,
                });
            }
        }
    }
}

fn emit_pressure_record(
    output: &mut MetricsBuffer,
    record: &PressureRecord,
    resource: &str,
    level: &str,
) {
    let tags = || -> MetricTags {
        metric_tags! {
            RESOURCE => resource,
            LEVEL => level,
        }
    };
    output.gauge(PSI_AVG10, f64::from(record.avg10), tags());
    output.gauge(PSI_AVG60, f64::from(record.avg60), tags());
    output.gauge(PSI_AVG300, f64::from(record.avg300), tags());
    output.counter(PSI_TOTAL, record.total as f64, tags());
}

#[derive(Debug)]
struct PsiError(String);

impl std::fmt::Display for PsiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PsiError {}

impl From<procfs::ProcError> for PsiError {
    fn from(err: procfs::ProcError) -> Self {
        PsiError(err.to_string())
    }
}

impl From<io::Error> for PsiError {
    fn from(err: io::Error) -> Self {
        PsiError(err.to_string())
    }
}

#[derive(Debug)]
struct PsiStats {
    cpu_some: PressureRecord,
    cpu_full: PressureRecord,
    memory_some: PressureRecord,
    memory_full: PressureRecord,
    io_some: PressureRecord,
    io_full: PressureRecord,
    irq_full: Option<PressureRecord>,
}

fn collect_psi_stats() -> Result<PsiStats, PsiError> {
    let cpu = CpuPressure::current()?;
    let memory = MemoryPressure::current()?;
    let io = IoPressure::current()?;
    let irq_full = read_irq_pressure()?;

    Ok(PsiStats {
        cpu_some: cpu.some,
        cpu_full: cpu.full,
        memory_some: memory.some,
        memory_full: memory.full,
        io_some: io.some,
        io_full: io.full,
        irq_full,
    })
}

/// Reads IRQ pressure from `/proc/pressure/irq`.
///
/// IRQ pressure only has a `full` line (no `some`). The file may not exist
/// on older kernels, so a missing file returns `Ok(None)`.
fn read_irq_pressure() -> Result<Option<PressureRecord>, PsiError> {
    let path = Path::new("/proc/pressure/irq");
    if !path.exists() {
        return Ok(None);
    }

    let file = std::fs::File::open(path)?;
    let reader = io::BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        // IRQ only has "full" line
        if line.starts_with("full ") {
            let record = procfs::parse_pressure_record(&line)?;
            return Ok(Some(record));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use crate::event::metric::MetricValue;
    use crate::sources::host_metrics::{HostMetrics, HostMetricsConfig, MetricsBuffer};

    use super::*;

    #[tokio::test]
    async fn generates_psi_metrics() {
        // PSI requires Linux 4.20+; skip if /proc/pressure/cpu is missing.
        if !Path::new("/proc/pressure/cpu").exists() {
            return;
        }

        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .psi_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;

        // At minimum we expect cpu(some+full) + memory(some+full) + io(some+full) = 6 records × 4 metrics = 24
        assert!(
            metrics.len() >= 24,
            "Expected at least 24 PSI metrics, got {}",
            metrics.len()
        );

        // If /proc/pressure/irq exists, we expect 28 total
        if Path::new("/proc/pressure/irq").exists() {
            assert_eq!(
                metrics.len(),
                28,
                "Expected 28 PSI metrics (with IRQ), got {}",
                metrics.len()
            );
        }
    }

    #[tokio::test]
    async fn psi_metrics_have_correct_types() {
        if !Path::new("/proc/pressure/cpu").exists() {
            return;
        }

        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .psi_metrics(&mut buffer)
            .await;

        for metric in &buffer.metrics {
            match metric.name() {
                PSI_AVG10 | PSI_AVG60 | PSI_AVG300 => {
                    assert!(
                        matches!(metric.value(), MetricValue::Gauge { .. }),
                        "Expected gauge for {}, got {:?}",
                        metric.name(),
                        metric.value()
                    );
                }
                PSI_TOTAL => {
                    assert!(
                        matches!(metric.value(), MetricValue::Counter { .. }),
                        "Expected counter for {}, got {:?}",
                        metric.name(),
                        metric.value()
                    );
                }
                other => panic!("Unexpected PSI metric name: {}", other),
            }
        }
    }

    #[tokio::test]
    async fn psi_metrics_have_correct_tags() {
        if !Path::new("/proc/pressure/cpu").exists() {
            return;
        }

        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .psi_metrics(&mut buffer)
            .await;

        for metric in &buffer.metrics {
            let tags = metric.tags().expect("PSI metric should have tags");
            assert!(
                tags.contains_key(RESOURCE),
                "PSI metric {} missing '{}' tag",
                metric.name(),
                RESOURCE
            );
            assert!(
                tags.contains_key(LEVEL),
                "PSI metric {} missing '{}' tag",
                metric.name(),
                LEVEL
            );

            let resource = tags.get(RESOURCE).unwrap();
            assert!(
                ["cpu", "memory", "io", "irq"].contains(&resource.as_ref()),
                "Unexpected resource tag value: {}",
                resource
            );

            let level = tags.get(LEVEL).unwrap();
            assert!(
                ["some", "full"].contains(&level.as_ref()),
                "Unexpected level tag value: {}",
                level
            );
        }
    }
}

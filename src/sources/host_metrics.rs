use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::{
        metric::{Metric, MetricKind, MetricValue},
        Event,
    },
    shutdown::ShutdownSignal,
    Pipeline,
};
use chrono::Utc;
use futures::{
    compat::Future01CompatExt,
    future::{FutureExt, TryFutureExt},
    stream::{self, StreamExt},
};
use futures01::Sink;
use glob::Pattern;
#[cfg(target_os = "macos")]
use heim::memory::os::macos::MemoryExt;
#[cfg(not(target_os = "windows"))]
use heim::memory::os::SwapExt;
#[cfg(target_os = "windows")]
use heim::net::os::windows::IoCountersExt;
#[cfg(target_os = "linux")]
use heim::{
    cpu::os::linux::CpuTimeExt, memory::os::linux::MemoryExt, net::os::linux::IoCountersExt,
};
use heim::{
    units::{information::byte, ratio::ratio, time::second},
    Error,
};
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::fmt;
use std::path::Path;
use std::time::Duration;
use tokio::{select, time};

macro_rules! btreemap {
    ( $( $key:expr => $value:expr ),* ) => {{
        #[allow(unused_mut)]
        let mut result = std::collections::BTreeMap::default();
        $( result.insert($key, $value); )*
            result
    }}
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Collector {
    Cpu,
    Disk,
    Filesystem,
    Load,
    Memory,
    Network,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct DiskConfig {
    devices: Option<Vec<PatternWrapper>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct FilesystemConfig {
    devices: Option<Vec<PatternWrapper>>,
    filesystems: Option<Vec<PatternWrapper>>,
    mountpoints: Option<Vec<PatternWrapper>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct NetworkConfig {
    devices: Option<Vec<PatternWrapper>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct HostMetricsConfig {
    #[serde(default = "scrape_interval_default")]
    scrape_interval_secs: u64,

    collectors: Option<Vec<Collector>>,

    #[serde(default)]
    disk: DiskConfig,
    #[serde(default)]
    filesystem: FilesystemConfig,
    #[serde(default)]
    network: NetworkConfig,
}

const fn scrape_interval_default() -> u64 {
    15
}

inventory::submit! {
    SourceDescription::new::<HostMetricsConfig>("host_metrics")
}

#[typetag::serde(name = "host_metrics")]
impl SourceConfig for HostMetricsConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        Ok(Box::new(self.clone().run(out, shutdown).boxed().compat()))
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "host_metrics"
    }
}

impl HostMetricsConfig {
    async fn run(self, mut out: Pipeline, shutdown: ShutdownSignal) -> Result<(), ()> {
        let interval = Duration::from_secs(self.scrape_interval_secs);
        let mut interval = time::interval(interval).map(|_| ());
        let mut shutdown = shutdown.compat();

        loop {
            select! {
                Some(()) = interval.next() => (),
                _ = &mut shutdown => break,
                else => break,
            };

            let metrics = self.capture_metrics().await;

            let (sink, _) = out
                .send_all(futures01::stream::iter_ok(metrics))
                .compat()
                .await
                .map_err(|error| error!(message = "Error sending host metrics", %error))?;
            out = sink;
        }

        Ok(())
    }

    fn has_collector(&self, collector: Collector) -> bool {
        match &self.collectors {
            None => true,
            Some(collectors) => collectors.iter().find(|&&c| c == collector).is_some(),
        }
    }

    async fn capture_metrics(&self) -> impl Iterator<Item = Event> {
        let hostname = crate::get_hostname();
        let mut metrics = Vec::new();
        if self.has_collector(Collector::Cpu) {
            metrics.extend(add_collector("cpu", cpu_metrics().await));
        }
        if self.has_collector(Collector::Disk) {
            metrics.extend(add_collector("disk", disk_metrics(&self.disk).await));
        }
        if self.has_collector(Collector::Filesystem) {
            metrics.extend(add_collector(
                "filesystem",
                filesystem_metrics(&self.filesystem).await,
            ));
        }
        if self.has_collector(Collector::Load) {
            metrics.extend(add_collector("load", loadavg_metrics().await));
        }
        if self.has_collector(Collector::Memory) {
            metrics.extend(add_collector("memory", memory_metrics().await));
            metrics.extend(add_collector("memory", swap_metrics().await));
        }
        if self.has_collector(Collector::Network) {
            metrics.extend(add_collector("network", net_metrics(&self.network).await));
        }
        if let Ok(hostname) = &hostname {
            for metric in &mut metrics {
                (metric.tags.as_mut().unwrap()).insert("host".into(), hostname.into());
            }
        }
        metrics.into_iter().map(Into::into)
    }
}

macro_rules! counter {
    ( $name:expr, $timestamp:expr, $value:expr $( , $tag:literal => $tagval:expr )* ) => {
        metric!($name, $timestamp, Counter, $value, btreemap![ $( $tag.into() => $tagval.into() ),* ] )
    };
    ( $name:expr, $timestamp:expr, $value:expr, $tags:expr ) => {
        metric!($name, $timestamp, Counter, $value, $tags)
    };
}

macro_rules! gauge {
    ( $name:expr, $timestamp:expr, $value:expr $( , $tag:literal => $tagval:expr )* ) => {
        metric!($name, $timestamp, Gauge, $value, btreemap![ $( $tag.into() => $tagval.into() ),* ] )
    };
    ( $name:expr, $timestamp:expr, $value:expr, $tags:expr ) => {
        metric!($name, $timestamp, Gauge, $value, $tags)
    };
}

macro_rules! metric {
    ( $name:expr, $timestamp:expr, $type:ident, $value:expr, $tags:expr ) => {
        Metric {
            name: $name.into(),
            timestamp: $timestamp,
            kind: MetricKind::Absolute,
            value: MetricValue::$type {
                value: $value as f64,
            },
            tags: Some($tags),
        }
    };
}

async fn cpu_metrics() -> Vec<Metric> {
    match heim::cpu::times().await {
        Ok(times) => {
            times
                .filter_map(|result| filter_result(result, "Failed to load/parse CPU time"))
                .map(|times| {
                    let timestamp = Some(Utc::now());
                    let name = "host_cpu_seconds_total";
                    stream::iter(
                    vec![
                        counter!(name, timestamp, times.idle().get::<second>(), "mode" => "idle"),
                        #[cfg(target_os = "linux")]
                        counter!(name, timestamp, times.nice().get::<second>(), "mode" => "nice"),
                        counter!(name, timestamp, times.system().get::<second>(), "mode" => "system"),
                        counter!(name, timestamp, times.user().get::<second>(), "mode" => "user"),
                    ]
                    .into_iter(),
                )
                })
                .flatten()
                .collect::<Vec<_>>()
                .await
        }
        Err(error) => {
            error!(message = "Failed to load CPU times", %error, rate_limit_secs = 60);
            vec![]
        }
    }
}

async fn memory_metrics() -> Vec<Metric> {
    match heim::memory::memory().await {
        Ok(memory) => {
            let timestamp = Some(Utc::now());
            vec![
                gauge!(
                    "host_memory_total_bytes",
                    timestamp,
                    memory.total().get::<byte>()
                ),
                gauge!(
                    "host_memory_free_bytes",
                    timestamp,
                    memory.free().get::<byte>()
                ),
                gauge!(
                    "host_memory_available_bytes",
                    timestamp,
                    memory.available().get::<byte>()
                ),
                #[cfg(target_os = "linux")]
                gauge!(
                    "host_memory_active_bytes",
                    timestamp,
                    memory.active().get::<byte>()
                ),
                #[cfg(target_os = "linux")]
                gauge!(
                    "host_memory_buffers_bytes",
                    timestamp,
                    memory.buffers().get::<byte>()
                ),
                #[cfg(target_os = "linux")]
                gauge!(
                    "host_memory_cached_bytes",
                    timestamp,
                    memory.cached().get::<byte>()
                ),
                #[cfg(target_os = "linux")]
                gauge!(
                    "host_memory_shared_bytes",
                    timestamp,
                    memory.shared().get::<byte>()
                ),
                #[cfg(target_os = "linux")]
                gauge!(
                    "host_memory_used_bytes",
                    timestamp,
                    memory.used().get::<byte>()
                ),
                #[cfg(target_os = "macos")]
                gauge!(
                    "host_memory_active_bytes",
                    timestamp,
                    memory.active().get::<byte>()
                ),
                #[cfg(target_os = "macos")]
                gauge!(
                    "host_memory_inactive_bytes",
                    timestamp,
                    memory.inactive().get::<byte>()
                ),
                #[cfg(target_os = "macos")]
                gauge!(
                    "host_memory_wired_bytes",
                    timestamp,
                    memory.wire().get::<byte>()
                ),
            ]
        }
        Err(error) => {
            error!(message = "Failed to load memory info", %error, rate_limit_secs = 60);
            vec![]
        }
    }
}

async fn swap_metrics() -> Vec<Metric> {
    match heim::memory::swap().await {
        Ok(swap) => {
            let timestamp = Some(Utc::now());
            vec![
                gauge!(
                    "host_memory_swap_free_bytes",
                    timestamp,
                    swap.free().get::<byte>()
                ),
                gauge!(
                    "host_memory_swap_total_bytes",
                    timestamp,
                    swap.total().get::<byte>()
                ),
                gauge!(
                    "host_memory_swap_used_bytes",
                    timestamp,
                    swap.used().get::<byte>()
                ),
                #[cfg(not(target_os = "windows"))]
                counter!(
                    "host_memory_swapped_in_bytes_total",
                    timestamp,
                    swap.sin().map(|swap| swap.get::<byte>()).unwrap_or(0)
                ),
                #[cfg(not(target_os = "windows"))]
                counter!(
                    "host_memory_swapped_out_bytes_total",
                    timestamp,
                    swap.sout().map(|swap| swap.get::<byte>()).unwrap_or(0)
                ),
            ]
        }
        Err(error) => {
            error!(message = "Failed to load swap info", %error, rate_limit_secs = 60);
            vec![]
        }
    }
}

async fn loadavg_metrics() -> Vec<Metric> {
    #[cfg(unix)]
    let result = match heim::cpu::os::unix::loadavg().await {
        Ok(loadavg) => {
            let timestamp = Some(Utc::now());
            vec![
                gauge!("host_load1", timestamp, loadavg.0.get::<ratio>()),
                gauge!("host_load5", timestamp, loadavg.1.get::<ratio>()),
                gauge!("host_load15", timestamp, loadavg.2.get::<ratio>()),
            ]
        }
        Err(error) => {
            error!(message = "Failed to load load average info", %error, rate_limit_secs = 60);
            vec![]
        }
    };
    #[cfg(not(unix))]
    let result = vec![];

    result
}

async fn net_metrics(config: &NetworkConfig) -> Vec<Metric> {
    match heim::net::io_counters().await {
        Ok(counters) => {
            counters
                .filter_map(|result| filter_result(result, "Failed to load/parse network data"))
                // The following pair should be possible to do in one
                // .filter_map, but it results in a strange "one type is
                // more general than the other" error.
                .map(|counter| {
                    vec_contains_str(&config.devices, counter.interface()).map(|()| counter)
                })
                .filter_map(|counter| async { counter })
                .map(|counter| {
                    let timestamp = Some(Utc::now());
                    let interface = counter.interface();
                    stream::iter(
                        vec![
                            counter!(
                                "host_network_receive_bytes_total",
                                timestamp,
                                counter.bytes_recv().get::<byte>(),
                                "device" => interface
                            ),
                            counter!(
                                "host_network_receive_errs_total",
                                timestamp,
                                counter.errors_recv(),
                                "device" => interface
                            ),
                            counter!(
                                "host_network_receive_packets_drop_total",
                                timestamp,
                                counter.drop_sent(),
                                "device" => interface
                            ),
                            counter!(
                                "host_network_receive_packets_total",
                                timestamp,
                                counter.drop_recv(),
                                "device" => interface
                            ),
                            counter!(
                                "host_network_transmit_bytes_total",
                                timestamp,
                                counter.bytes_sent().get::<byte>(),
                                "device" => interface
                            ),
                            counter!(
                                "host_network_transmit_errs_total",
                                timestamp,
                                counter.errors_sent(),
                                "device" => interface
                            ),
                            #[cfg(any(target_os = "windows", target_os = "linux"))]
                            counter!(
                                "host_network_transmit_packets_total",
                                timestamp,
                                counter.packets_sent(),
                                "device" => interface
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
            error!(message = "Failed to load network I/O counters", %error, rate_limit_secs = 60);
            vec![]
        }
    }
}

async fn filesystem_metrics(config: &FilesystemConfig) -> Vec<Metric> {
    match heim::disk::partitions().await {
        Ok(partitions) => {
            partitions
                .filter_map(|result| filter_result(result, "Failed to load/parse partition data"))
                .map(|partition| {
                    vec_contains_path(&config.mountpoints, partition.mount_point()).map(|()| partition)
                })
                .filter_map(|partition| async { partition })
                .map(|partition| match partition.device() {
                    Some(device) => vec_contains_path(&config.devices, device.as_ref()).map(|()| partition),
                    None => Some(partition),
                })
                .filter_map(|partition| async { partition })
                .map(|partition| {
                    vec_contains_str(&config.filesystems, partition.file_system().as_str())
                        .map(|()| partition)
                })
                .filter_map(|partition| async { partition })
                .filter_map(|partition| async {
                    heim::disk::usage(partition.mount_point())
                        .await
                        .map(|usage| (partition, usage))
                        .map_err(|error| {
                            error!(message = "Failed to load partition usage data", %error, rate_limit_secs = 60)
                        })
                        .ok()
                })
                .map(|(partition, usage)| {
                    let timestamp = Some(Utc::now());
                    let fs = partition.file_system();
                    let mut tags = btreemap![
                        "filesystem".to_string() => fs.as_str().to_string(),
                        "mountpoint".into() => partition.mount_point().to_string_lossy().into()
                    ];
                    if let Some(device) = partition.device() {
                        tags.insert("device".into(), device.to_string_lossy().into());
                    }
                    stream::iter(
                        vec![
                            gauge!(
                                "host_filesystem_free_bytes",
                                timestamp,
                                usage.free().get::<byte>(),
                                tags.clone()
                            ),
                            gauge!(
                                "host_filesystem_total_bytes",
                                timestamp,
                                usage.total().get::<byte>(),
                                tags.clone()
                            ),
                            gauge!(
                                "host_filesystem_used_bytes",
                                timestamp,
                                usage.used().get::<byte>(),
                                tags
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
            error!(message = "Failed to load partitions info", %error, rate_limit_secs = 60);
            vec![]
        }
    }
}

async fn disk_metrics(config: &DiskConfig) -> Vec<Metric> {
    match heim::disk::io_counters().await {
        Ok(counters) => {
            counters
                .filter_map(|result| filter_result(result, "Failed to load/parse disk I/O data"))
                .map(|counter| {
                    vec_contains_path(&config.devices, counter.device_name().as_ref())
                        .map(|()| counter)
                })
                .filter_map(|counter| async { counter })
                .map(|counter| {
                    let timestamp = Some(Utc::now());
                    let tags = btreemap![
                        "device".into() => counter.device_name().to_string_lossy().to_string()
                    ];
                    stream::iter(
                        vec![
                            gauge!(
                                "host_disk_read_bytes_total",
                                timestamp,
                                counter.read_bytes().get::<byte>(),
                                tags.clone()
                            ),
                            gauge!(
                                "host_disk_reads_completed_total",
                                timestamp,
                                counter.read_count(),
                                tags.clone()
                            ),
                            gauge!(
                                "host_disk_written_bytes_total",
                                timestamp,
                                counter.write_bytes().get::<byte>(),
                                tags.clone()
                            ),
                            gauge!(
                                "host_disk_writes_completed_total",
                                timestamp,
                                counter.write_count(),
                                tags
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
            error!(message = "Failed to load disk I/O info", %error, rate_limit_secs = 60);
            vec![]
        }
    }
}

async fn filter_result<T>(result: Result<T, Error>, message: &'static str) -> Option<T> {
    result
        .map_err(|error| error!(message, %error, rate_limit_secs = 60))
        .ok()
}

fn vec_contains_path(vec: &Option<Vec<PatternWrapper>>, value: &Path) -> Option<()> {
    match vec {
        // No patterns list matches everything
        None => Some(()),
        // Otherwise find the given value
        Some(vec) => vec
            .iter()
            .find(|&pattern| pattern.matches_path(value))
            .map(|_| ()),
    }
}

fn vec_contains_str(vec: &Option<Vec<PatternWrapper>>, value: &str) -> Option<()> {
    match vec {
        // No patterns list matches everything
        None => Some(()),
        // Otherwise find the given value
        Some(vec) => vec
            .iter()
            .find(|&pattern| pattern.matches(value))
            .map(|_| ()),
    }
}

fn add_collector(collector: &str, mut metrics: Vec<Metric>) -> Vec<Metric> {
    for metric in &mut metrics {
        (metric.tags.as_mut().unwrap()).insert("collector".into(), collector.into());
    }
    metrics
}

// Pattern doesn't implement Deserialize or Serialize, and we can't
// implement them ourselves due the orphan rules, so make a wrapper.
#[derive(Clone, Debug)]
struct PatternWrapper(Pattern);

impl PatternWrapper {
    fn matches(&self, s: &str) -> bool {
        self.0.matches(s)
    }

    fn matches_path(&self, p: &Path) -> bool {
        self.0.matches_path(p)
    }
}

impl<'de> Deserialize<'de> for PatternWrapper {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(PatternVisitor)
    }
}

struct PatternVisitor;

impl<'de> Visitor<'de> for PatternVisitor {
    type Value = PatternWrapper;

    fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "a string")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
        Pattern::new(s)
            .map(PatternWrapper)
            .map_err(de::Error::custom)
    }
}

impl Serialize for PatternWrapper {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.as_str())
    }
}

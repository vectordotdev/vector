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
#[cfg(target_os = "linux")]
use heim::cpu::os::linux::CpuTimeExt;
#[cfg(target_os = "macos")]
use heim::memory::os::macos::MemoryExt;
#[cfg(not(target_os = "windows"))]
use heim::memory::os::SwapExt;
use heim::{
    units::{information::byte, ratio::ratio, time::second},
    Error,
};
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct HostMetricsConfig {
    #[serde(default = "scrape_interval_default")]
    scrape_interval_secs: u64,
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

            let metrics = capture_metrics().await;

            let (sink, _) = out
                .send_all(futures01::stream::iter_ok(metrics))
                .compat()
                .await
                .map_err(|error| error!(message = "Error sending host metrics", %error))?;
            out = sink;
        }

        Ok(())
    }
}

async fn capture_metrics() -> impl Iterator<Item = Event> {
    let hostname = crate::get_hostname();
    (cpu_metrics().await)
        .chain(memory_metrics().await)
        .chain(swap_metrics().await)
        .chain(loadavg_metrics().await)
        .map(move |mut metric| {
            if let Ok(hostname) = &hostname {
                (metric.tags.as_mut().unwrap()).insert("host".into(), hostname.into());
            }
            metric
        })
        .map(Into::into)
}

macro_rules! counter {
    ( $name:expr, $timestamp:expr, $value:expr $( , $tag:literal => $tagval:literal )* ) => {
        metric!($name, $timestamp, Counter, $value $( , $tag => $tagval )* )
    };
}

macro_rules! gauge {
    ( $name:expr, $timestamp:expr, $value:expr $( , $tag:literal => $tagval:literal )* ) => {
        metric!($name, $timestamp, Gauge, $value $( , $tag => $tagval )* )
    };
}

macro_rules! metric {
    ( $name:expr, $timestamp:expr, $type:ident, $value:expr $( , $tag:literal => $tagval:literal )* ) => {
        Metric {
            name: $name.into(),
            timestamp: $timestamp,
            kind: MetricKind::Absolute,
            value: MetricValue::$type {
                value: $value as f64,
            },
            tags: Some(btreemap![
                $( $tag.into() => $tagval.into() ),*
            ]),
        }
    };
}

async fn cpu_metrics() -> impl Iterator<Item = Metric> {
    match heim::cpu::times().await {
        Ok(times) => {
            times
                .map(Result::unwrap)
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
    .into_iter().map(add_collector("cpu"))
}

async fn memory_metrics() -> impl Iterator<Item = Metric> {
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
                // Missing: host_memory_compressed_bytes from ???
                // Missing: used, buffers, cached, shared, active,
                // inactive on Linux from
                // heim::memory::os::linux::MemoryExt
            ]
        }
        Err(error) => {
            error!(message = "Failed to load memory info", %error, rate_limit_secs = 60);
            vec![]
        }
    }
    .into_iter()
    .map(add_collector("memory"))
}

async fn swap_metrics() -> impl Iterator<Item = Metric> {
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
    .into_iter()
    .map(add_collector("memory"))
}

async fn loadavg_metrics() -> impl Iterator<Item = Metric> {
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
    result.into_iter().map(add_collector("cpu"))
}

fn add_collector(collector: &str) -> impl Fn(Metric) -> Metric + '_ {
    move |mut metric| {
        (metric.tags.as_mut().unwrap()).insert("collector".into(), collector.into());
        metric
    }
}

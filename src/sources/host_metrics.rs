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
use heim::{units::information::byte, units::time::second, Error};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::{select, time};

macro_rules! btreemap {
    ( $( $key:expr => $value:expr ),* ) => {{
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
        let interval = Duration::from_secs(self.scrape_interval_secs);
        let fut = run(out, shutdown, interval).boxed().compat();
        Ok(Box::new(fut))
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "host_metrics"
    }
}

async fn run(mut out: Pipeline, shutdown: ShutdownSignal, duration: Duration) -> Result<(), ()> {
    let mut interval = time::interval(duration).map(|_| ());
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

async fn capture_metrics() -> impl Iterator<Item = Event> {
    cpu_metrics()
        .await
        .chain(memory_metrics().await)
        .chain(swap_metrics().await)
        .map(Into::into)
}

macro_rules! counter {
    ( $name:expr, $timestamp:expr, $value:expr ) => {
        metric!($name, $timestamp, Counter, $value => None)
    };
    ( $name:expr, $timestamp:expr, $value:expr $( , $tag:literal => $tagval:literal )+ ) => {
        metric!($name, $timestamp, Counter, $value => Some(btreemap!( $( $tag.into() => $tagval.into() ),+ )))
    };
}

macro_rules! gauge {
    ( $name:expr, $timestamp:expr, $value:expr ) => {
        metric!($name, $timestamp, Gauge, $value => None)
    };
    ( $name:expr, $timestamp:expr, $value:expr $( , $tag:literal => $tagval:literal )+ ) => {
        metric!($name, $timestamp, Gauge, $value => Some(btreemap!( $( $tag.into() => $tagval.into() ),+ )))
    };
}

macro_rules! metric {
    ( $name:expr, $timestamp:expr, $type:ident, $value:expr => $tags:expr ) => {
        Metric {
            name: $name.into(),
            timestamp: $timestamp,
            kind: MetricKind::Absolute,
            value: MetricValue::$type {
                value: $value as f64,
            },
            tags: $tags,
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
    .into_iter()
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
            ]
        }
        Err(error) => {
            error!(message = "Failed to load swap info", %error, rate_limit_secs = 60);
            vec![]
        }
    }
    .into_iter()
}

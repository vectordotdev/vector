use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{DateTime, Utc};
use futures::StreamExt;
use glob::{Pattern, PatternError};
#[cfg(not(windows))]
use heim::units::ratio::ratio;
use heim::units::time::second;
use serde_with::serde_as;
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::config::LogNamespace;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol, Registered,
};
use vector_lib::EstimatedJsonEncodedSizeOf;

use crate::{
    config::{SourceConfig, SourceContext, SourceOutput},
    event::metric::{Metric, MetricKind, MetricTags, MetricValue},
    internal_events::{EventsReceived, HostMetricsScrapeDetailError, StreamClosedError},
    shutdown::ShutdownSignal,
    SourceSender,
};

#[cfg(target_os = "linux")]
mod cgroups;
mod cpu;
mod disk;
mod filesystem;
mod memory;
mod network;

/// Collector types.
#[serde_as]
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Collector {
    /// Metrics related to Linux control groups.
    ///
    /// Only available on Linux.
    CGroups,

    /// Metrics related to CPU utilization.
    Cpu,

    /// Metrics related to disk I/O utilization.
    Disk,

    /// Metrics related to filesystem space utilization.
    Filesystem,

    /// Metrics related to the system load average.
    Load,

    /// Metrics related to the host.
    Host,

    /// Metrics related to memory utilization.
    Memory,

    /// Metrics related to network utilization.
    Network,
}

/// Filtering configuration.
#[configurable_component]
#[derive(Clone, Debug, Default)]
struct FilterList {
    /// Any patterns which should be included.
    ///
    /// The patterns are matched using globbing.
    includes: Option<Vec<PatternWrapper>>,

    /// Any patterns which should be excluded.
    ///
    /// The patterns are matched using globbing.
    excludes: Option<Vec<PatternWrapper>>,
}

/// Configuration for the `host_metrics` source.
#[serde_as]
#[configurable_component(source("host_metrics", "Collect metric data from the local system."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct HostMetricsConfig {
    /// The interval between metric gathering, in seconds.
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(default = "default_scrape_interval")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    pub scrape_interval_secs: Duration,

    /// The list of host metric collector services to use.
    ///
    /// Defaults to all collectors.
    #[configurable(metadata(docs::examples = "example_collectors()"))]
    #[derivative(Default(value = "default_collectors()"))]
    #[serde(default = "default_collectors")]
    pub collectors: Option<Vec<Collector>>,

    /// Overrides the default namespace for the metrics emitted by the source.
    #[derivative(Default(value = "default_namespace()"))]
    #[serde(default = "default_namespace")]
    pub namespace: Option<String>,

    #[configurable(derived)]
    #[derivative(Default(value = "default_cgroups_config()"))]
    #[serde(default = "default_cgroups_config")]
    pub cgroups: Option<CGroupsConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub disk: disk::DiskConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub filesystem: filesystem::FilesystemConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub network: network::NetworkConfig,
}

/// Options for the cgroups (controller groups) metrics collector.
///
/// This collector is only available on Linux systems, and only supports either version 2 or hybrid cgroups.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(default)]
pub struct CGroupsConfig {
    /// The number of levels of the cgroups hierarchy for which to report metrics.
    ///
    /// A value of `1` means the root or named cgroup.
    #[derivative(Default(value = "default_levels()"))]
    #[serde(default = "default_levels")]
    #[configurable(metadata(docs::examples = 1))]
    #[configurable(metadata(docs::examples = 3))]
    levels: usize,

    /// The base cgroup name to provide metrics for.
    #[configurable(metadata(docs::examples = "/"))]
    #[configurable(metadata(docs::examples = "system.slice/snapd.service"))]
    pub(super) base: Option<PathBuf>,

    /// Lists of cgroup name patterns to include or exclude in gathering
    /// usage metrics.
    #[configurable(metadata(docs::examples = "example_cgroups()"))]
    #[serde(default = "default_all_devices")]
    groups: FilterList,

    /// Base cgroup directory, for testing use only
    #[serde(skip_serializing)]
    #[configurable(metadata(docs::hidden))]
    #[configurable(metadata(docs::human_name = "Base Directory"))]
    base_dir: Option<PathBuf>,
}

const fn default_scrape_interval() -> Duration {
    Duration::from_secs(15)
}

pub fn default_namespace() -> Option<String> {
    Some(String::from("host"))
}

const fn example_collectors() -> [&'static str; 8] {
    [
        "cgroups",
        "cpu",
        "disk",
        "filesystem",
        "load",
        "host",
        "memory",
        "network",
    ]
}

fn default_collectors() -> Option<Vec<Collector>> {
    let mut collectors = vec![
        Collector::Cpu,
        Collector::Disk,
        Collector::Filesystem,
        Collector::Load,
        Collector::Host,
        Collector::Memory,
        Collector::Network,
    ];

    #[cfg(target_os = "linux")]
    {
        collectors.push(Collector::CGroups);
    }
    #[cfg(not(target_os = "linux"))]
    if std::env::var("VECTOR_GENERATE_SCHEMA").is_ok() {
        collectors.push(Collector::CGroups);
    }

    Some(collectors)
}

fn example_devices() -> FilterList {
    FilterList {
        includes: Some(vec!["sda".try_into().unwrap()]),
        excludes: Some(vec!["dm-*".try_into().unwrap()]),
    }
}

fn default_all_devices() -> FilterList {
    FilterList {
        includes: Some(vec!["*".try_into().unwrap()]),
        excludes: None,
    }
}

const fn default_levels() -> usize {
    100
}

fn example_cgroups() -> FilterList {
    FilterList {
        includes: Some(vec!["user.slice/*".try_into().unwrap()]),
        excludes: Some(vec!["*.service".try_into().unwrap()]),
    }
}

fn default_cgroups_config() -> Option<CGroupsConfig> {
    // Check env variable to allow generating docs on non-linux systems.
    if std::env::var("VECTOR_GENERATE_SCHEMA").is_ok() {
        return Some(CGroupsConfig::default());
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }

    #[cfg(target_os = "linux")]
    {
        Some(CGroupsConfig::default())
    }
}

impl_generate_config_from_default!(HostMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "host_metrics")]
impl SourceConfig for HostMetricsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        init_roots();

        #[cfg(not(target_os = "linux"))]
        {
            if self.cgroups.is_some() || self.has_collector(Collector::CGroups) {
                return Err("CGroups collector is only available on Linux systems".into());
            }
        }

        let mut config = self.clone();
        config.namespace = config.namespace.filter(|namespace| !namespace.is_empty());

        Ok(Box::pin(config.run(cx.out, cx.shutdown)))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

impl HostMetricsConfig {
    /// Set the interval to collect internal metrics.
    pub fn scrape_interval_secs(&mut self, value: f64) {
        self.scrape_interval_secs = Duration::from_secs_f64(value);
    }

    async fn run(self, mut out: SourceSender, shutdown: ShutdownSignal) -> Result<(), ()> {
        let duration = self.scrape_interval_secs;
        let mut interval = IntervalStream::new(time::interval(duration)).take_until(shutdown);

        let generator = HostMetrics::new(self);

        let bytes_received = register!(BytesReceived::from(Protocol::NONE));

        while interval.next().await.is_some() {
            bytes_received.emit(ByteSize(0));
            let metrics = generator.capture_metrics().await;
            let count = metrics.len();
            if (out.send_batch(metrics).await).is_err() {
                emit!(StreamClosedError { count });
                return Err(());
            }
        }

        Ok(())
    }

    fn has_collector(&self, collector: Collector) -> bool {
        match &self.collectors {
            None => true,
            Some(collectors) => collectors.iter().any(|&c| c == collector),
        }
    }
}

pub struct HostMetrics {
    config: HostMetricsConfig,
    #[cfg(target_os = "linux")]
    root_cgroup: Option<cgroups::CGroupRoot>,
    events_received: Registered<EventsReceived>,
}

impl HostMetrics {
    #[cfg(not(target_os = "linux"))]
    pub fn new(config: HostMetricsConfig) -> Self {
        Self {
            config,
            events_received: register!(EventsReceived),
        }
    }

    #[cfg(target_os = "linux")]
    pub fn new(config: HostMetricsConfig) -> Self {
        let cgroups = config.cgroups.clone().unwrap_or_default();
        let root_cgroup = cgroups::CGroupRoot::new(&cgroups);
        Self {
            config,
            root_cgroup,
            events_received: register!(EventsReceived),
        }
    }

    pub fn buffer(&self) -> MetricsBuffer {
        MetricsBuffer::new(self.config.namespace.clone())
    }

    async fn capture_metrics(&self) -> Vec<Metric> {
        let mut buffer = self.buffer();

        #[cfg(target_os = "linux")]
        if self.config.has_collector(Collector::CGroups) {
            self.cgroups_metrics(&mut buffer).await;
        }
        if self.config.has_collector(Collector::Cpu) {
            self.cpu_metrics(&mut buffer).await;
        }
        if self.config.has_collector(Collector::Disk) {
            self.disk_metrics(&mut buffer).await;
        }
        if self.config.has_collector(Collector::Filesystem) {
            self.filesystem_metrics(&mut buffer).await;
        }
        if self.config.has_collector(Collector::Load) {
            self.loadavg_metrics(&mut buffer).await;
        }
        if self.config.has_collector(Collector::Host) {
            self.host_metrics(&mut buffer).await;
        }
        if self.config.has_collector(Collector::Memory) {
            self.memory_metrics(&mut buffer).await;
            self.swap_metrics(&mut buffer).await;
        }
        if self.config.has_collector(Collector::Network) {
            self.network_metrics(&mut buffer).await;
        }

        let metrics = buffer.metrics;
        self.events_received.emit(CountByteSize(
            metrics.len(),
            metrics.estimated_json_encoded_size_of(),
        ));
        metrics
    }

    pub async fn loadavg_metrics(&self, output: &mut MetricsBuffer) {
        output.name = "load";
        #[cfg(unix)]
        match heim::cpu::os::unix::loadavg().await {
            Ok(loadavg) => {
                output.gauge(
                    "load1",
                    loadavg.0.get::<ratio>() as f64,
                    MetricTags::default(),
                );
                output.gauge(
                    "load5",
                    loadavg.1.get::<ratio>() as f64,
                    MetricTags::default(),
                );
                output.gauge(
                    "load15",
                    loadavg.2.get::<ratio>() as f64,
                    MetricTags::default(),
                );
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load average info",
                    error,
                });
            }
        }
    }

    pub async fn host_metrics(&self, output: &mut MetricsBuffer) {
        output.name = "host";
        match heim::host::uptime().await {
            Ok(time) => output.gauge("uptime", time.get::<second>(), MetricTags::default()),
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load host uptime info",
                    error,
                });
            }
        }

        match heim::host::boot_time().await {
            Ok(time) => output.gauge("boot_time", time.get::<second>(), MetricTags::default()),
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load host boot time info",
                    error,
                });
            }
        }
    }
}

#[derive(Default)]
pub struct MetricsBuffer {
    pub metrics: Vec<Metric>,
    name: &'static str,
    host: Option<String>,
    timestamp: DateTime<Utc>,
    namespace: Option<String>,
}

impl MetricsBuffer {
    fn new(namespace: Option<String>) -> Self {
        Self {
            metrics: Vec::new(),
            name: "",
            host: crate::get_hostname().ok(),
            timestamp: Utc::now(),
            namespace,
        }
    }

    fn tags(&self, mut tags: MetricTags) -> MetricTags {
        tags.replace("collector".into(), self.name.to_string());
        if let Some(host) = &self.host {
            tags.replace("host".into(), host.clone());
        }
        tags
    }

    fn counter(&mut self, name: &str, value: f64, tags: MetricTags) {
        self.metrics.push(
            Metric::new(name, MetricKind::Absolute, MetricValue::Counter { value })
                .with_namespace(self.namespace.clone())
                .with_tags(Some(self.tags(tags)))
                .with_timestamp(Some(self.timestamp)),
        )
    }

    fn gauge(&mut self, name: &str, value: f64, tags: MetricTags) {
        self.metrics.push(
            Metric::new(name, MetricKind::Absolute, MetricValue::Gauge { value })
                .with_namespace(self.namespace.clone())
                .with_tags(Some(self.tags(tags)))
                .with_timestamp(Some(self.timestamp)),
        )
    }
}

fn filter_result_sync<T, E>(result: Result<T, E>, message: &'static str) -> Option<T>
where
    E: std::error::Error,
{
    result
        .map_err(|error| emit!(HostMetricsScrapeDetailError { message, error }))
        .ok()
}

async fn filter_result<T, E>(result: Result<T, E>, message: &'static str) -> Option<T>
where
    E: std::error::Error,
{
    filter_result_sync(result, message)
}

#[allow(clippy::missing_const_for_fn)]
fn init_roots() {
    #[cfg(target_os = "linux")]
    {
        use std::sync::Once;

        static INIT: Once = Once::new();

        INIT.call_once(|| {
            match std::env::var_os("PROCFS_ROOT") {
                Some(procfs_root) => {
                    info!(
                        message = "PROCFS_ROOT is set in envvars. Using custom for procfs.",
                        custom = ?procfs_root
                    );
                    heim::os::linux::set_procfs_root(std::path::PathBuf::from(&procfs_root));
                }
                None => info!("PROCFS_ROOT is unset. Using default '/proc' for procfs root."),
            };

            match std::env::var_os("SYSFS_ROOT") {
                Some(sysfs_root) => {
                    info!(
                        message = "SYSFS_ROOT is set in envvars. Using custom for sysfs.",
                        custom = ?sysfs_root
                    );
                    heim::os::linux::set_sysfs_root(std::path::PathBuf::from(&sysfs_root));
                }
                None => info!("SYSFS_ROOT is unset. Using default '/sys' for sysfs root."),
            }
        });
    };
}

impl FilterList {
    fn contains<T, M>(&self, value: &Option<T>, matches: M) -> bool
    where
        M: Fn(&PatternWrapper, &T) -> bool,
    {
        (match (&self.includes, value) {
            // No includes list includes everything
            (None, _) => true,
            // Includes list matched against empty value returns false
            (Some(_), None) => false,
            // Otherwise find the given value
            (Some(includes), Some(value)) => includes.iter().any(|pattern| matches(pattern, value)),
        }) && match (&self.excludes, value) {
            // No excludes, list excludes nothing
            (None, _) => true,
            // No value, never excluded
            (Some(_), None) => true,
            // Otherwise find the given value
            (Some(excludes), Some(value)) => {
                !excludes.iter().any(|pattern| matches(pattern, value))
            }
        }
    }

    fn contains_str(&self, value: Option<&str>) -> bool {
        self.contains(&value, |pattern, s| pattern.matches_str(s))
    }

    fn contains_path(&self, value: Option<&Path>) -> bool {
        self.contains(&value, |pattern, path| pattern.matches_path(path))
    }

    #[cfg(test)]
    fn contains_test(&self, value: Option<&str>) -> bool {
        let result = self.contains_str(value);
        assert_eq!(result, self.contains_path(value.map(std::path::Path::new)));
        result
    }
}

/// A compiled Unix shell-style pattern.
///
/// - `?` matches any single character.
/// - `*` matches any (possibly empty) sequence of characters.
/// - `**` matches the current directory and arbitrary subdirectories. This sequence must form a single path component,
///   so both `**a` and `b**` are invalid and will result in an error. A sequence of more than two consecutive `*`
///   characters is also invalid.
/// - `[...]` matches any character inside the brackets. Character sequences can also specify ranges of characters, as
///   ordered by Unicode, so e.g. `[0-9]` specifies any character between 0 and 9 inclusive. An unclosed bracket is
///   invalid.
/// - `[!...]` is the negation of `[...]`, i.e. it matches any characters not in the brackets.
///
/// The metacharacters `?`, `*`, `[`, `]` can be matched by using brackets (e.g. `[?]`). When a `]` occurs immediately
/// following `[` or `[!` then it is interpreted as being part of, rather then ending, the character set, so `]` and NOT
/// `]` can be matched by `[]]` and `[!]]` respectively. The `-` character can be specified inside a character sequence
/// pattern by placing it at the start or the end, e.g. `[abc-]`.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(try_from = "String", into = "String")]
struct PatternWrapper(Pattern);

impl PatternWrapper {
    fn matches_str(&self, s: &str) -> bool {
        self.0.matches(s)
    }

    fn matches_path(&self, p: &Path) -> bool {
        self.0.matches_path(p)
    }
}

impl TryFrom<String> for PatternWrapper {
    type Error = PatternError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Pattern::new(value.as_ref()).map(PatternWrapper)
    }
}

impl TryFrom<&str> for PatternWrapper {
    type Error = PatternError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.to_string().try_into()
    }
}

impl From<PatternWrapper> for String {
    fn from(pattern: PatternWrapper) -> Self {
        pattern.0.to_string()
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::components::{run_and_assert_source_compliance, SOURCE_TAGS};
    use std::{collections::HashSet, future::Future, time::Duration};

    use super::*;

    #[test]
    fn filterlist_default_includes_everything() {
        let filters = FilterList::default();
        assert!(filters.contains_test(Some("anything")));
        assert!(filters.contains_test(Some("should")));
        assert!(filters.contains_test(Some("work")));
        assert!(filters.contains_test(None));
    }

    #[test]
    fn filterlist_includes_works() {
        let filters = FilterList {
            includes: Some(vec![
                PatternWrapper::try_from("sda".to_string()).unwrap(),
                PatternWrapper::try_from("dm-*".to_string()).unwrap(),
            ]),
            excludes: None,
        };
        assert!(!filters.contains_test(Some("sd")));
        assert!(filters.contains_test(Some("sda")));
        assert!(!filters.contains_test(Some("sda1")));
        assert!(filters.contains_test(Some("dm-")));
        assert!(filters.contains_test(Some("dm-5")));
        assert!(!filters.contains_test(Some("xda")));
        assert!(!filters.contains_test(None));
    }

    #[test]
    fn filterlist_excludes_works() {
        let filters = FilterList {
            includes: None,
            excludes: Some(vec![
                PatternWrapper::try_from("sda".to_string()).unwrap(),
                PatternWrapper::try_from("dm-*".to_string()).unwrap(),
            ]),
        };
        assert!(filters.contains_test(Some("sd")));
        assert!(!filters.contains_test(Some("sda")));
        assert!(filters.contains_test(Some("sda1")));
        assert!(!filters.contains_test(Some("dm-")));
        assert!(!filters.contains_test(Some("dm-5")));
        assert!(filters.contains_test(Some("xda")));
        assert!(filters.contains_test(None));
    }

    #[test]
    fn filterlist_includes_and_excludes_works() {
        let filters = FilterList {
            includes: Some(vec![
                PatternWrapper::try_from("sda".to_string()).unwrap(),
                PatternWrapper::try_from("dm-*".to_string()).unwrap(),
            ]),
            excludes: Some(vec![PatternWrapper::try_from("dm-5".to_string()).unwrap()]),
        };
        assert!(!filters.contains_test(Some("sd")));
        assert!(filters.contains_test(Some("sda")));
        assert!(!filters.contains_test(Some("sda1")));
        assert!(filters.contains_test(Some("dm-")));
        assert!(filters.contains_test(Some("dm-1")));
        assert!(!filters.contains_test(Some("dm-5")));
        assert!(!filters.contains_test(Some("xda")));
        assert!(!filters.contains_test(None));
    }

    #[tokio::test]
    async fn filters_on_collectors() {
        let all_metrics_count = HostMetrics::new(HostMetricsConfig::default())
            .capture_metrics()
            .await
            .len();

        for collector in &[
            #[cfg(target_os = "linux")]
            Collector::CGroups,
            Collector::Cpu,
            Collector::Disk,
            Collector::Filesystem,
            Collector::Load,
            Collector::Host,
            Collector::Memory,
            Collector::Network,
        ] {
            let some_metrics = HostMetrics::new(HostMetricsConfig {
                collectors: Some(vec![*collector]),
                ..Default::default()
            })
            .capture_metrics()
            .await;

            assert!(
                all_metrics_count > some_metrics.len(),
                "collector={:?}",
                collector
            );
        }
    }

    #[tokio::test]
    async fn are_tagged_with_hostname() {
        let metrics = HostMetrics::new(HostMetricsConfig::default())
            .capture_metrics()
            .await;
        let hostname = crate::get_hostname().expect("Broken hostname");
        assert!(!metrics.into_iter().any(|event| event
            .tags()
            .expect("Missing tags")
            .get("host")
            .expect("Missing \"host\" tag")
            != hostname));
    }

    #[tokio::test]
    async fn uses_custom_namespace() {
        let metrics = HostMetrics::new(HostMetricsConfig {
            namespace: Some("other".into()),
            ..Default::default()
        })
        .capture_metrics()
        .await;

        assert!(metrics
            .into_iter()
            .all(|event| event.namespace() == Some("other")));
    }

    #[tokio::test]
    async fn uses_default_namespace() {
        let metrics = HostMetrics::new(HostMetricsConfig::default())
            .capture_metrics()
            .await;

        assert!(metrics
            .iter()
            .all(|event| event.namespace() == Some("host")));
    }

    // Windows does not produce load average metrics.
    #[cfg(not(windows))]
    #[tokio::test]
    async fn generates_loadavg_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .loadavg_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;
        assert_eq!(metrics.len(), 3);
        assert!(all_gauges(&metrics));

        // All metrics are named load*
        assert!(!metrics
            .iter()
            .any(|metric| !metric.name().starts_with("load")));
    }

    #[tokio::test]
    async fn generates_host_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .host_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;
        assert_eq!(metrics.len(), 2);
        assert!(all_gauges(&metrics));
    }

    pub(super) fn all_counters(metrics: &[Metric]) -> bool {
        !metrics
            .iter()
            .any(|metric| !matches!(metric.value(), &MetricValue::Counter { .. }))
    }

    pub(super) fn all_gauges(metrics: &[Metric]) -> bool {
        !metrics
            .iter()
            .any(|metric| !matches!(metric.value(), &MetricValue::Gauge { .. }))
    }

    fn all_tags_match(metrics: &[Metric], tag: &str, matches: impl Fn(&str) -> bool) -> bool {
        !metrics.iter().any(|metric| {
            metric
                .tags()
                .unwrap()
                .get(tag)
                .map(|value| !matches(value))
                .unwrap_or(false)
        })
    }

    pub(super) fn count_name(metrics: &[Metric], name: &str) -> usize {
        metrics
            .iter()
            .filter(|metric| metric.name() == name)
            .count()
    }

    pub(super) fn count_tag(metrics: &[Metric], tag: &str) -> usize {
        metrics
            .iter()
            .filter(|metric| {
                metric
                    .tags()
                    .expect("Metric is missing tags")
                    .contains_key(tag)
            })
            .count()
    }

    fn collect_tag_values(metrics: &[Metric], tag: &str) -> HashSet<String> {
        metrics
            .iter()
            .filter_map(|metric| metric.tags().unwrap().get(tag).map(ToOwned::to_owned))
            .collect::<HashSet<_>>()
    }

    // Run a series of tests using filters to ensure they are obeyed
    pub(super) async fn assert_filtered_metrics<Get, Fut>(tag: &str, get_metrics: Get)
    where
        Get: Fn(FilterList) -> Fut,
        Fut: Future<Output = Vec<Metric>>,
    {
        let all_metrics = get_metrics(FilterList::default()).await;

        let keys = collect_tag_values(&all_metrics, tag);
        // Pick an arbitrary key value
        if let Some(key) = keys.into_iter().next() {
            let key_prefix = &key[..key.len() - 1].to_string();
            let key_prefix_pattern = PatternWrapper::try_from(format!("{}*", key_prefix)).unwrap();
            let key_pattern = PatternWrapper::try_from(key.clone()).unwrap();

            let filter = FilterList {
                includes: Some(vec![key_pattern.clone()]),
                excludes: None,
            };
            let filtered_metrics_with = get_metrics(filter).await;

            assert!(filtered_metrics_with.len() <= all_metrics.len());
            assert!(!filtered_metrics_with.is_empty());
            assert!(all_tags_match(&filtered_metrics_with, tag, |s| s == key));

            let filter = FilterList {
                includes: Some(vec![key_prefix_pattern.clone()]),
                excludes: None,
            };
            let filtered_metrics_with_match = get_metrics(filter).await;

            assert!(filtered_metrics_with_match.len() >= filtered_metrics_with.len());
            assert!(all_tags_match(&filtered_metrics_with_match, tag, |s| {
                s.starts_with(key_prefix)
            }));

            let filter = FilterList {
                includes: None,
                excludes: Some(vec![key_pattern]),
            };
            let filtered_metrics_without = get_metrics(filter).await;

            assert!(filtered_metrics_without.len() <= all_metrics.len());
            assert!(all_tags_match(&filtered_metrics_without, tag, |s| s != key));

            let filter = FilterList {
                includes: None,
                excludes: Some(vec![key_prefix_pattern]),
            };
            let filtered_metrics_without_match = get_metrics(filter).await;

            assert!(filtered_metrics_without_match.len() <= filtered_metrics_without.len());
            assert!(all_tags_match(&filtered_metrics_without_match, tag, |s| {
                !s.starts_with(key_prefix)
            }));

            assert!(
                filtered_metrics_with.len() + filtered_metrics_without.len() <= all_metrics.len()
            );
        }
    }

    #[tokio::test]
    async fn source_compliance() {
        let config = HostMetricsConfig {
            scrape_interval_secs: Duration::from_secs(1),
            ..Default::default()
        };

        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(2), &SOURCE_TAGS).await;

        assert!(!events.is_empty());
    }
}

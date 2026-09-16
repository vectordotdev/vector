use vector_lib::{
    NamedInternalEvent,
    configurable::configurable_component,
    counter, gauge,
    internal_event::{CounterName, GaugeName, InternalEvent},
};

/// Configuration of internal metrics for enrichment memory table.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct InternalMetricsConfig {
    /// Determines whether to include the key tag on internal metrics.
    ///
    /// This is useful for distinguishing between different keys while monitoring. However, the tag's
    /// cardinality is unbounded.
    #[serde(default = "crate::serde::default_false")]
    pub include_key_tag: bool,
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct MemoryEnrichmentTableRead<'a> {
    pub key: &'a str,
    pub include_key_metric_tag: bool,
}

impl InternalEvent for MemoryEnrichmentTableRead<'_> {
    fn emit(self) {
        if self.include_key_metric_tag {
            counter!(
                CounterName::MemoryEnrichmentTableReadsTotal,
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!(CounterName::MemoryEnrichmentTableReadsTotal,).increment(1);
        }
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct MemoryEnrichmentTableInserted<'a> {
    pub key: &'a str,
    pub include_key_metric_tag: bool,
}

impl InternalEvent for MemoryEnrichmentTableInserted<'_> {
    fn emit(self) {
        if self.include_key_metric_tag {
            counter!(
                CounterName::MemoryEnrichmentTableInsertionsTotal,
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!(CounterName::MemoryEnrichmentTableInsertionsTotal,).increment(1);
        }
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct MemoryEnrichmentTableRemoved<'a> {
    pub key: &'a str,
    pub include_key_metric_tag: bool,
}

impl InternalEvent for MemoryEnrichmentTableRemoved<'_> {
    fn emit(self) {
        if self.include_key_metric_tag {
            counter!(
                CounterName::MemoryEnrichmentTableRemovedTotal,
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!(CounterName::MemoryEnrichmentTableRemovedTotal).increment(1);
        }
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct MemoryEnrichmentTableFlushed {
    pub new_objects_count: usize,
    pub new_byte_size: usize,
}

impl InternalEvent for MemoryEnrichmentTableFlushed {
    fn emit(self) {
        counter!(CounterName::MemoryEnrichmentTableFlushesTotal,).increment(1);
        gauge!(GaugeName::MemoryEnrichmentTableObjectsCount,).set(self.new_objects_count as f64);
        gauge!(GaugeName::MemoryEnrichmentTableByteSize,).set(self.new_byte_size as f64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct MemoryEnrichmentTableTtlExpired<'a> {
    pub key: &'a str,
    pub include_key_metric_tag: bool,
}

impl InternalEvent for MemoryEnrichmentTableTtlExpired<'_> {
    fn emit(self) {
        if self.include_key_metric_tag {
            counter!(
                CounterName::MemoryEnrichmentTableTtlExpirationsTotal,
                "key" => self.key.to_owned()
            )
            .increment(1);
            counter!(
                CounterName::MemoryEnrichmentTableTtlExpirations,
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!(CounterName::MemoryEnrichmentTableTtlExpirationsTotal,).increment(1);
            counter!(CounterName::MemoryEnrichmentTableTtlExpirations,).increment(1);
        }
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct MemoryEnrichmentTableTtlExpiredCount {
    pub count: u64,
}

impl InternalEvent for MemoryEnrichmentTableTtlExpiredCount {
    fn emit(self) {
        counter!(CounterName::MemoryEnrichmentTableTtlExpirations,).increment(self.count);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct MemoryEnrichmentTableReadFailed<'a> {
    pub key: &'a str,
    pub include_key_metric_tag: bool,
}

impl InternalEvent for MemoryEnrichmentTableReadFailed<'_> {
    fn emit(self) {
        if self.include_key_metric_tag {
            counter!(
                CounterName::MemoryEnrichmentTableFailedReadsTotal,
                "key" => self.key.to_owned()
            )
            .increment(1);
            counter!(
                CounterName::MemoryEnrichmentTableFailedReads,
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!(CounterName::MemoryEnrichmentTableFailedReadsTotal,).increment(1);
            counter!(CounterName::MemoryEnrichmentTableFailedReads,).increment(1);
        }
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct MemoryEnrichmentTableInsertFailed<'a> {
    pub key: &'a str,
    pub include_key_metric_tag: bool,
}

impl InternalEvent for MemoryEnrichmentTableInsertFailed<'_> {
    fn emit(self) {
        if self.include_key_metric_tag {
            counter!(
                CounterName::MemoryEnrichmentTableFailedInsertionsTotal,
                "key" => self.key.to_owned()
            )
            .increment(1);
            counter!(
                CounterName::MemoryEnrichmentTableFailedInsertions,
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!(CounterName::MemoryEnrichmentTableFailedInsertionsTotal,).increment(1);
            counter!(CounterName::MemoryEnrichmentTableFailedInsertions,).increment(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use vector_lib::{
        event::{Metric, MetricValue},
        metrics::Controller,
    };

    use super::*;

    const KEY: &str = "test_key";

    fn capture_metrics(emit: impl FnOnce()) -> Vec<Metric> {
        vector_lib::metrics::init_test();
        let controller = Controller::get().unwrap();
        controller.reset();

        emit();

        controller.capture_metrics()
    }

    /// Look up metrics by name only; panic unless exactly one matches.
    fn assert_single_metric<'a>(metrics: &'a [Metric], name: &str) -> &'a Metric {
        let matches = metrics
            .iter()
            .filter(|metric| metric.name() == name)
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "expected exactly one metric named {name}");
        matches[0]
    }

    /// Validate a retrieved metric's type, tags, and value.
    fn assert_counter(metric: &Metric, key: Option<&str>) {
        assert!(matches!(metric.value(), MetricValue::Counter { value } if *value == 1.0));
        assert_eq!(metric.tag_value("key").as_deref(), key);
    }

    #[rstest]
    #[case::ttl_expired(
        "memory_enrichment_table_ttl_expirations",
        |include_key_metric_tag| {
            MemoryEnrichmentTableTtlExpired {
                key: KEY,
                include_key_metric_tag,
            }
            .emit();
        },
    )]
    #[case::read_failed(
        "memory_enrichment_table_failed_reads",
        |include_key_metric_tag| {
            MemoryEnrichmentTableReadFailed {
                key: KEY,
                include_key_metric_tag,
            }
            .emit();
        },
    )]
    #[case::insert_failed(
        "memory_enrichment_table_failed_insertions",
        |include_key_metric_tag| {
            MemoryEnrichmentTableInsertFailed {
                key: KEY,
                include_key_metric_tag,
            }
            .emit();
        },
    )]
    fn emits_total_and_legacy_counters(
        #[case] base_name: &str,
        #[case] emit: fn(bool),
        #[values(false, true)] include_key_metric_tag: bool,
    ) {
        let total_name = format!("{base_name}_total");
        let metrics = capture_metrics(|| emit(include_key_metric_tag));

        // Identify the expected metrics by name, then validate each in turn.
        let total = assert_single_metric(&metrics, &total_name);
        let legacy = assert_single_metric(&metrics, base_name);

        let key = include_key_metric_tag.then_some(KEY);
        assert_counter(total, key);
        assert_counter(legacy, key);
    }
}

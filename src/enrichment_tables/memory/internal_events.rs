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
    use vector_lib::{
        event::{Metric, MetricValue},
        metrics::Controller,
    };

    use super::*;

    const KEY: &str = "test_key";

    fn emit_compatibility_events(include_key_metric_tag: bool) {
        MemoryEnrichmentTableTtlExpired {
            key: KEY,
            include_key_metric_tag,
        }
        .emit();
        MemoryEnrichmentTableReadFailed {
            key: KEY,
            include_key_metric_tag,
        }
        .emit();
        MemoryEnrichmentTableInsertFailed {
            key: KEY,
            include_key_metric_tag,
        }
        .emit();
    }

    fn assert_counter(metrics: &[Metric], name: &str, key: Option<&str>) {
        let metric = metrics
            .iter()
            .find(|metric| {
                matches!(metric.value(), MetricValue::Counter { value } if *value == 1.0)
                    && metric.name() == name
                    && metric.tag_value("key").as_deref() == key
            })
            .unwrap_or_else(|| panic!("missing counter {name} with key {key:?}"));

        if key.is_none() {
            assert!(metric.tag_value("key").is_none());
        }
    }

    fn assert_compatibility_counter_names(metrics: &[Metric], key: Option<&str>) {
        for name in [
            "memory_enrichment_table_ttl_expirations_total",
            "memory_enrichment_table_ttl_expirations",
            "memory_enrichment_table_failed_reads_total",
            "memory_enrichment_table_failed_reads",
            "memory_enrichment_table_failed_insertions_total",
            "memory_enrichment_table_failed_insertions",
        ] {
            assert_counter(metrics, name, key);
        }
    }

    #[test]
    fn compatibility_counters_emit_total_and_legacy_names_without_key() {
        vector_lib::metrics::init_test();
        let controller = Controller::get().unwrap();
        controller.reset();

        emit_compatibility_events(false);

        let metrics = controller.capture_metrics();
        assert_compatibility_counter_names(&metrics, None);
    }

    #[test]
    fn compatibility_counters_emit_total_and_legacy_names_with_key() {
        vector_lib::metrics::init_test();
        let controller = Controller::get().unwrap();
        controller.reset();

        emit_compatibility_events(true);

        let metrics = controller.capture_metrics();
        assert_compatibility_counter_names(&metrics, Some(KEY));
    }
}

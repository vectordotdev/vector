use vector_lib::{
    NamedInternalEvent, configurable::configurable_component,
    counter, gauge,
    internal_event::{InternalEvent, MetricName},
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
                MetricName::MemoryEnrichmentTableReadsTotal,
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!(MetricName::MemoryEnrichmentTableReadsTotal,).increment(1);
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
                MetricName::MemoryEnrichmentTableInsertionsTotal,
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!(MetricName::MemoryEnrichmentTableInsertionsTotal,).increment(1);
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
        counter!(MetricName::MemoryEnrichmentTableFlushesTotal,).increment(1);
        gauge!(MetricName::MemoryEnrichmentTableObjectsCount,).set(self.new_objects_count as f64);
        gauge!(MetricName::MemoryEnrichmentTableByteSize,).set(self.new_byte_size as f64);
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
                MetricName::MemoryEnrichmentTableTtlExpirations,
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!(MetricName::MemoryEnrichmentTableTtlExpirations,).increment(1);
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
                MetricName::MemoryEnrichmentTableFailedReads,
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!(MetricName::MemoryEnrichmentTableFailedReads,).increment(1);
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
                MetricName::MemoryEnrichmentTableFailedInsertions,
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!(MetricName::MemoryEnrichmentTableFailedInsertions,).increment(1);
        }
    }
}

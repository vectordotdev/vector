use metrics::{counter, gauge};
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::InternalEvent;

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

#[derive(Debug)]
pub(crate) struct MemoryEnrichmentTableRead<'a> {
    pub key: &'a str,
    pub include_key_metric_tag: bool,
}

impl InternalEvent for MemoryEnrichmentTableRead<'_> {
    fn emit(self) {
        if self.include_key_metric_tag {
            counter!(
                "memory_enrichment_table_reads_total",
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!("memory_enrichment_table_reads_total",).increment(1);
        }
    }

    fn name(&self) -> Option<&'static str> {
        Some("MemoryEnrichmentTableRead")
    }
}

#[derive(Debug)]
pub(crate) struct MemoryEnrichmentTableInserted<'a> {
    pub key: &'a str,
    pub include_key_metric_tag: bool,
}

impl InternalEvent for MemoryEnrichmentTableInserted<'_> {
    fn emit(self) {
        if self.include_key_metric_tag {
            counter!(
                "memory_enrichment_table_insertions_total",
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!("memory_enrichment_table_insertions_total",).increment(1);
        }
    }

    fn name(&self) -> Option<&'static str> {
        Some("MemoryEnrichmentTableInserted")
    }
}

#[derive(Debug)]
pub(crate) struct MemoryEnrichmentTableFlushed {
    pub new_objects_count: usize,
    pub new_byte_size: usize,
}

impl InternalEvent for MemoryEnrichmentTableFlushed {
    fn emit(self) {
        counter!("memory_enrichment_table_flushes_total",).increment(1);
        gauge!("memory_enrichment_table_objects_count",).set(self.new_objects_count as f64);
        gauge!("memory_enrichment_table_byte_size",).set(self.new_byte_size as f64);
    }

    fn name(&self) -> Option<&'static str> {
        Some("MemoryEnrichmentTableFlushed")
    }
}

#[derive(Debug)]
pub(crate) struct MemoryEnrichmentTableTtlExpired<'a> {
    pub key: &'a str,
    pub include_key_metric_tag: bool,
}

impl InternalEvent for MemoryEnrichmentTableTtlExpired<'_> {
    fn emit(self) {
        if self.include_key_metric_tag {
            counter!(
                "memory_enrichment_table_ttl_expirations",
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!("memory_enrichment_table_ttl_expirations",).increment(1);
        }
    }

    fn name(&self) -> Option<&'static str> {
        Some("MemoryEnrichmentTableTtlExpired")
    }
}

#[derive(Debug)]
pub(crate) struct MemoryEnrichmentTableReadFailed<'a> {
    pub key: &'a str,
    pub include_key_metric_tag: bool,
}

impl InternalEvent for MemoryEnrichmentTableReadFailed<'_> {
    fn emit(self) {
        if self.include_key_metric_tag {
            counter!(
                "memory_enrichment_table_failed_reads",
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!("memory_enrichment_table_failed_reads",).increment(1);
        }
    }

    fn name(&self) -> Option<&'static str> {
        Some("MemoryEnrichmentTableReadFailed")
    }
}

#[derive(Debug)]
pub(crate) struct MemoryEnrichmentTableInsertFailed<'a> {
    pub key: &'a str,
    pub include_key_metric_tag: bool,
}

impl InternalEvent for MemoryEnrichmentTableInsertFailed<'_> {
    fn emit(self) {
        if self.include_key_metric_tag {
            counter!(
                "memory_enrichment_table_failed_insertions",
                "key" => self.key.to_owned()
            )
            .increment(1);
        } else {
            counter!("memory_enrichment_table_failed_insertions",).increment(1);
        }
    }

    fn name(&self) -> Option<&'static str> {
        Some("MemoryEnrichmentTableInsertFailed")
    }
}

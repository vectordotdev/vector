use metrics::{counter, gauge};
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub(crate) struct MemoryEnrichmentTableRead {
    pub key: String,
}

impl InternalEvent for MemoryEnrichmentTableRead {
    fn emit(self) {
        counter!(
            "memory_enrichment_table_reads_total",
            "key" => self.key
        )
        .increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("MemoryEnrichmentTableRead")
    }
}

#[derive(Debug)]
pub(crate) struct MemoryEnrichmentTableInserted {
    pub key: String,
}

impl InternalEvent for MemoryEnrichmentTableInserted {
    fn emit(self) {
        counter!(
            "memory_enrichment_table_insertions_total",
            "key" => self.key
        )
        .increment(1);
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
pub(crate) struct MemoryEnrichmentTableTtlExpired {
    pub key: String,
}

impl InternalEvent for MemoryEnrichmentTableTtlExpired {
    fn emit(self) {
        counter!(
            "memory_enrichment_table_ttl_expirations",
            "key" => self.key
        )
        .increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("MemoryEnrichmentTableTtlExpired")
    }
}

#[derive(Debug)]
pub(crate) struct MemoryEnrichmentTableReadFailed {
    pub key: String,
}

impl InternalEvent for MemoryEnrichmentTableReadFailed {
    fn emit(self) {
        counter!(
            "memory_enrichment_table_failed_reads",
            "key" => self.key
        )
        .increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("MemoryEnrichmentTableReadFailed")
    }
}

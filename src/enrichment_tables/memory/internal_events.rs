use metrics::{counter, gauge};
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub(crate) struct MemoryEnrichmentTableRead {
    pub table: String,
    pub key: String,
}

impl InternalEvent for MemoryEnrichmentTableRead {
    fn emit(self) {
        counter!(
            "memory_enrichment_table_reads_total",
            "table" => self.table,
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
    pub table: String,
    pub key: String,
}

impl InternalEvent for MemoryEnrichmentTableInserted {
    fn emit(self) {
        counter!(
            "memory_enrichment_table_insertions_total",
            "table" => self.table.clone(),
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
    pub table: String,
    pub new_objects_count: usize,
    pub new_byte_size: usize,
}

impl InternalEvent for MemoryEnrichmentTableFlushed {
    fn emit(self) {
        counter!(
            "memory_enrichment_table_flushes_total",
            "table" => self.table.clone(),
        )
        .increment(1);
        gauge!(
            "memory_enrichment_table_objects_count",
            "table" => self.table.clone()
        )
        .set(self.new_objects_count as f64);
        gauge!(
            "memory_enrichment_table_byte_size",
            "table" => self.table
        )
        .set(self.new_byte_size as f64);
    }

    fn name(&self) -> Option<&'static str> {
        Some("MemoryEnrichmentTableFlushed")
    }
}

#[derive(Debug)]
pub(crate) struct MemoryEnrichmentTableTtlExpired {
    pub table: String,
    pub key: String,
}

impl InternalEvent for MemoryEnrichmentTableTtlExpired {
    fn emit(self) {
        counter!(
            "memory_enrichment_table_ttl_expirations",
            "table" => self.table.clone(),
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
    pub table: String,
    pub key: String,
}

impl InternalEvent for MemoryEnrichmentTableReadFailed {
    fn emit(self) {
        counter!(
            "memory_enrichment_table_failed_reads",
            "table" => self.table,
            "key" => self.key
        )
        .increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("MemoryEnrichmentTableReadFailed")
    }
}

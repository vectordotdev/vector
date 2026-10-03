use metrics::counter;
use vector_lib::{NamedInternalEvent, internal_event::InternalEvent};

/// Emitted once per successful ADX ingest call (streaming or queued).
///
/// Exposes `azure_data_explorer_events_ingested_total` with `database` and `table`
/// labels so operators can track per-table throughput independently of the sink's
/// aggregated `component_sent_events_total`.
#[derive(Debug, NamedInternalEvent)]
pub struct AdxEventsIngested<'a> {
    pub database: &'a str,
    pub table: &'a str,
    pub event_count: usize,
    pub byte_size: usize,
}

impl InternalEvent for AdxEventsIngested<'_> {
    fn emit(self) {
        trace!(
            message = "Azure Data Explorer batch ingested.",
            database = %self.database,
            table = %self.table,
            event_count = %self.event_count,
            byte_size = %self.byte_size,
        );

        counter!(
            "azure_data_explorer_events_ingested_total",
            "database" => self.database.to_string(),
            "table" => self.table.to_string(),
        )
        .increment(self.event_count as u64);

        counter!(
            "azure_data_explorer_bytes_ingested_total",
            "database" => self.database.to_string(),
            "table" => self.table.to_string(),
        )
        .increment(self.byte_size as u64);
    }
}

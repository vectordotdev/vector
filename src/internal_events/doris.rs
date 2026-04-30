use vector_common::counter;
use vector_lib::{NamedInternalEvent, internal_event::InternalEvent};

/// Emitted when rows are successfully loaded into Doris.
#[derive(Debug, NamedInternalEvent)]
pub struct DorisRowsLoaded {
    pub loaded_rows: i64,
    pub load_bytes: i64,
}

impl InternalEvent for DorisRowsLoaded {
    fn emit(self) {
        trace!(
            message = "Doris rows loaded successfully.",
            loaded_rows = %self.loaded_rows,
            load_bytes = %self.load_bytes
        );

        // Record the number of rows loaded
        counter!(MetricName::DorisRowsLoadedTotal).increment(self.loaded_rows as u64);

        // Record the number of bytes loaded
        counter!(MetricName::DorisBytesLoadedTotal).increment(self.load_bytes as u64);
    }
}

/// Emitted when rows are filtered by Doris during loading.
#[derive(Debug, NamedInternalEvent)]
pub struct DorisRowsFiltered {
    pub filtered_rows: i64,
}

impl InternalEvent for DorisRowsFiltered {
    fn emit(self) {
        warn!(
            message = "Doris rows filtered during loading.",
            filtered_rows = %self.filtered_rows
        );

        counter!(MetricName::DorisRowsFilteredTotal).increment(self.filtered_rows as u64);
    }
}

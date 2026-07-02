use std::time::Duration;

use vector_lib::{
    NamedInternalEvent, histogram,
    internal_event::{HistogramName, InternalEvent},
};

#[derive(Debug, NamedInternalEvent)]
pub struct DuckdbRequestProcessed {
    pub rows: usize,
    pub encode_duration: Duration,
    pub lock_wait_duration: Duration,
    pub transaction_begin_duration: Duration,
    pub appender_create_duration: Duration,
    pub append_duration: Duration,
    pub flush_duration: Duration,
    pub commit_duration: Duration,
    pub total_duration: Duration,
}

impl InternalEvent for DuckdbRequestProcessed {
    fn emit(self) {
        trace!(
            message = "DuckDB sink request processed.",
            rows = self.rows,
            encode_duration_secs = self.encode_duration.as_secs_f64(),
            lock_wait_duration_secs = self.lock_wait_duration.as_secs_f64(),
            transaction_begin_duration_secs = self.transaction_begin_duration.as_secs_f64(),
            appender_create_duration_secs = self.appender_create_duration.as_secs_f64(),
            append_duration_secs = self.append_duration.as_secs_f64(),
            flush_duration_secs = self.flush_duration.as_secs_f64(),
            commit_duration_secs = self.commit_duration.as_secs_f64(),
            total_duration_secs = self.total_duration.as_secs_f64(),
        );

        let histogram = |stage: &'static str| histogram!(HistogramName::DuckdbRequestStageDurationSeconds, "stage" => stage);

        histogram("encode").record(self.encode_duration.as_secs_f64());
        histogram("lock_wait").record(self.lock_wait_duration.as_secs_f64());
        histogram("transaction_begin").record(self.transaction_begin_duration.as_secs_f64());
        histogram("appender_create").record(self.appender_create_duration.as_secs_f64());
        histogram("append").record(self.append_duration.as_secs_f64());
        histogram("flush").record(self.flush_duration.as_secs_f64());
        histogram("commit").record(self.commit_duration.as_secs_f64());
        histogram("total").record(self.total_duration.as_secs_f64());
    }
}

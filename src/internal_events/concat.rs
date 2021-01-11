use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ConcatSubstringError<'a> {
    pub source: &'a str,
    pub condition: &'a str,
    pub start: usize,
    pub end: usize,
    pub length: usize,
}

impl<'a> InternalEvent for ConcatSubstringError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Substring error.",
            self.condition,
            self.source,
            self.start,
            self.end,
            self.length,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct ConcatSubstringSourceMissing<'a> {
    pub source: &'a str,
}

impl<'a> InternalEvent for ConcatSubstringSourceMissing<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "Substring source missing.",
            self.source,
            internal_log_rate_secs = 30
        );
    }
}

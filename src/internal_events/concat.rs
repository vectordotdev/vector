use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ConcatEventProcessed;

impl InternalEvent for ConcatEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

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
            rate_limit_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1);
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
            rate_limit_secs = 30
        );
    }
}

use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct DecoderFramingFailed<'a> {
    pub error: &'a crate::codec::BoxedFramingError,
}

impl<'a> InternalEvent for DecoderFramingFailed<'a> {
    fn emit_logs(&self) {
        warn!(message = "Failed framing bytes.", error = %self.error, internal_log_rate_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("decoder_framing_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct DecoderParseFailed<'a> {
    pub error: &'a crate::Error,
}

impl<'a> InternalEvent for DecoderParseFailed<'a> {
    fn emit_logs(&self) {
        warn!(message = "Failed parsing frame.", error = %self.error, internal_log_rate_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("decoder_parse_errors_total", 1);
    }
}

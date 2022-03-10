use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct EncoderFramingFailed<'a> {
    pub error: &'a crate::codecs::encoding::BoxedFramingError,
}

impl<'a> InternalEvent for EncoderFramingFailed<'a> {
    fn emit_logs(&self) {
        warn!(message = "Failed framing bytes.", error = %self.error, internal_log_rate_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("encoder_framing_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct EncoderSerializeFailed<'a> {
    pub error: &'a crate::Error,
}

impl<'a> InternalEvent for EncoderSerializeFailed<'a> {
    fn emit_logs(&self) {
        warn!(message = "Failed serializing frame.", error = %self.error, internal_log_rate_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("encoder_serialize_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct RawMessageEmpty;

impl InternalEvent for RawMessageEmpty {
    fn emit_logs(&self) {
        warn!(
            message = "Received empty message while encoding message key.",
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("raw_message_empty_total", 1);
    }
}

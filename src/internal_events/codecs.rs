use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct DecoderFramingFailed<'a> {
    pub error: &'a codecs::decoding::BoxedFramingError,
}

impl<'a> InternalEvent for DecoderFramingFailed<'a> {
    fn emit(self) {
        warn!(message = "Failed framing bytes.", error = %self.error, internal_log_rate_secs = 10);
        counter!("decoder_framing_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct DecoderDeserializeFailed<'a> {
    pub error: &'a crate::Error,
}

impl<'a> InternalEvent for DecoderDeserializeFailed<'a> {
    fn emit(self) {
        warn!(message = "Failed deserializing frame.", error = %self.error, internal_log_rate_secs = 10);
        counter!("decoder_deserialize_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct EncoderFramingError<'a> {
    pub error: &'a codecs::encoding::BoxedFramingError,
}

impl<'a> InternalEvent for EncoderFramingError<'a> {
    fn emit(self) {
        warn!(message = "Failed framing bytes.", error = %self.error, internal_log_rate_secs = 10);
        error!(
            message = "Events dropped.",
            count = 1,
            error = %self.error,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::SENDING,
            intentional = "false",
            reason = "Failed framing bytes.",
        );
        counter!("encoder_framing_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::SENDING,
        );
        counter!(
            "component_discarded_events_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::SENDING,
            "intentional" => "false",
        );
    }
}

#[derive(Debug)]
pub struct EncoderSerializeError<'a> {
    pub error: &'a crate::Error,
}

impl<'a> InternalEvent for EncoderSerializeError<'a> {
    fn emit(self) {
        warn!(message = "Failed serializing frame.", error = %self.error, internal_log_rate_secs = 10);
        error!(
            message = "Events dropped.",
            count = 1,
            error = %self.error,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::SENDING,
            intentional = "false",
            reason = "Failed serializing frame.",
        );
        counter!("encoder_serialize_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::SENDING,
        );
        counter!(
            "component_discarded_events_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::SENDING,
            "intentional" => "false",
        );
    }
}

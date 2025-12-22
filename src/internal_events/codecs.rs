use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{
    ComponentEventsDropped, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};

#[derive(Debug, NamedInternalEvent)]
pub struct DecoderFramingError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for DecoderFramingError<E> {
    fn emit(self) {
        error!(
            message = "Failed framing bytes.",
            error = %self.error,
            error_code = "decoder_frame",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "decoder_frame",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct DecoderDeserializeError<'a> {
    pub error: &'a crate::Error,
}

impl InternalEvent for DecoderDeserializeError<'_> {
    fn emit(self) {
        error!(
            message = "Failed deserializing frame.",
            error = %self.error,
            error_code = "decoder_deserialize",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "decoder_deserialize",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct EncoderFramingError<'a> {
    pub error: &'a vector_lib::codecs::encoding::BoxedFramingError,
}

impl InternalEvent for EncoderFramingError<'_> {
    fn emit(self) {
        let reason = "Failed framing bytes.";
        error!(
            message = reason,
            error = %self.error,
            error_code = "encoder_frame",
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "encoder_frame",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct EncoderSerializeError<'a> {
    pub error: &'a crate::Error,
}

impl InternalEvent for EncoderSerializeError<'_> {
    fn emit(self) {
        const SERIALIZE_REASON: &str = "Failed serializing frame.";
        error!(
            message = SERIALIZE_REASON,
            error = %self.error,
            error_code = "encoder_serialize",
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "encoder_serialize",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: 1,
            reason: SERIALIZE_REASON
        });
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct EncoderWriteError<'a, E> {
    pub error: &'a E,
    pub count: usize,
}

impl<E: std::fmt::Display> InternalEvent for EncoderWriteError<'_, E> {
    fn emit(self) {
        let reason = "Failed writing bytes.";
        error!(
            message = reason,
            error = %self.error,
            error_type = error_type::IO_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
        if self.count > 0 {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: self.count,
                reason,
            });
        }
    }
}

#[cfg(feature = "codecs-arrow")]
#[derive(Debug, NamedInternalEvent)]
pub struct EncoderNullConstraintError<'a> {
    pub error: &'a crate::Error,
}

#[cfg(feature = "codecs-arrow")]
impl InternalEvent for EncoderNullConstraintError<'_> {
    fn emit(self) {
        const CONSTRAINT_REASON: &str = "Schema constraint violation.";
        error!(
            message = CONSTRAINT_REASON,
            error = %self.error,
            error_code = "encoding_null_constraint",
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "encoding_null_constraint",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: 1,
            reason: CONSTRAINT_REASON
        });
    }
}

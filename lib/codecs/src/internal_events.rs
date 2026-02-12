//! Internal events for codecs.

use metrics::counter;
use tracing::error;
use vector_common::internal_event::{
    ComponentEventsDropped, InternalEvent, UNINTENTIONAL, emit, error_stage, error_type,
};
use vector_common_macros::NamedInternalEvent;

#[derive(Debug, NamedInternalEvent)]
/// Emitted when a decoder framing error occurs.
pub struct DecoderFramingError<E> {
    /// The framing error that occurred.
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
/// Emitted when a decoder fails to deserialize a frame.
pub struct DecoderDeserializeError<'a> {
    /// The deserialize error that occurred.
    pub error: &'a vector_common::Error,
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
/// Emitted when an encoder framing error occurs.
pub struct EncoderFramingError<'a> {
    /// The framing error that occurred.
    pub error: &'a crate::encoding::BoxedFramingError,
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
        emit(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}

#[derive(Debug, NamedInternalEvent)]
/// Emitted when an encoder fails to serialize a frame.
pub struct EncoderSerializeError<'a> {
    /// The serialization error that occurred.
    pub error: &'a vector_common::Error,
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
        emit(ComponentEventsDropped::<UNINTENTIONAL> {
            count: 1,
            reason: SERIALIZE_REASON,
        });
    }
}

#[derive(Debug, NamedInternalEvent)]
/// Emitted when writing encoded bytes fails.
pub struct EncoderWriteError<'a, E> {
    /// The write error that occurred.
    pub error: &'a E,
    /// The number of events dropped by the failed write.
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
            emit(ComponentEventsDropped::<UNINTENTIONAL> {
                count: self.count,
                reason,
            });
        }
    }
}

#[cfg(feature = "arrow")]
#[derive(Debug, NamedInternalEvent)]
/// Emitted when encoding violates a schema constraint.
pub struct EncoderNullConstraintError<'a> {
    /// The schema constraint error that occurred.
    pub error: &'a vector_common::Error,
}

#[cfg(feature = "arrow")]
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
        emit(ComponentEventsDropped::<UNINTENTIONAL> {
            count: 1,
            reason: CONSTRAINT_REASON,
        });
    }
}

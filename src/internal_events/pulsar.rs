use metrics::counter;
#[cfg(feature = "sources-pulsar")]
use metrics::Counter;
use vector_lib::internal_event::{
    error_stage, error_type, ComponentEventsDropped, InternalEvent, UNINTENTIONAL,
};

#[derive(Debug)]
pub struct PulsarSendingError {
    pub count: usize,
    pub error: vector_lib::Error,
}

impl InternalEvent for PulsarSendingError {
    fn emit(self) {
        let reason = "A Pulsar sink generated an error.";
        error!(
            message = reason,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason,
        });
    }
}

pub struct PulsarPropertyExtractionError<F: std::fmt::Display> {
    pub property_field: F,
}

impl<F: std::fmt::Display> InternalEvent for PulsarPropertyExtractionError<F> {
    fn emit(self) {
        error!(
            message = "Failed to extract properties. Value should be a map of String -> Bytes.",
            error_code = "extracting_property",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            property_field = %self.property_field,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "extracting_property",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[cfg(feature = "sources-pulsar")]
pub enum PulsarErrorEventType {
    Read,
    Ack,
    NAck,
}

#[cfg(feature = "sources-pulsar")]
pub struct PulsarErrorEventData {
    pub msg: String,
    pub error_type: PulsarErrorEventType,
}

#[cfg(feature = "sources-pulsar")]
registered_event!(
    PulsarErrorEvent => {
        ack_errors: Counter = counter!(
            "component_errors_total",
            "error_code" => "acknowledge_message",
            "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
            "stage" => error_stage::RECEIVING,
        ),

        nack_errors: Counter = counter!(
            "component_errors_total",
            "error_code" => "negative_acknowledge_message",
            "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
            "stage" => error_stage::RECEIVING,
        ),

        read_errors: Counter = counter!(
            "component_errors_total",
            "error_code" => "reading_message",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        ),
    }

    fn emit(&self,error:PulsarErrorEventData) {
        match error.error_type{
            PulsarErrorEventType::Read => {
                error!(
                    message = "Failed to read message.",
                    error = error.msg,
                    error_code = "reading_message",
                    error_type = error_type::READER_FAILED,
                    stage = error_stage::RECEIVING,
                    internal_log_rate_limit = true,
                );

                self.read_errors.increment(1_u64);
            }
            PulsarErrorEventType::Ack => {
                error!(
                    message = "Failed to acknowledge message.",
                    error = error.msg,
                    error_code = "acknowledge_message",
                    error_type = error_type::ACKNOWLEDGMENT_FAILED,
                    stage = error_stage::RECEIVING,
                    internal_log_rate_limit = true,
                );

                self.ack_errors.increment(1_u64);
            }
            PulsarErrorEventType::NAck => {
                error!(
                    message = "Failed to negatively acknowledge message.",
                    error = error.msg,
                    error_code = "negative_acknowledge_message",
                    error_type = error_type::ACKNOWLEDGMENT_FAILED,
                    stage = error_stage::RECEIVING,
                    internal_log_rate_limit = true,
                );

                self.nack_errors.increment(1_u64);
            }
        }
    }
);

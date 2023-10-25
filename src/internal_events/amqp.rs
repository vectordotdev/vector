#[cfg(feature = "sources-amqp")]
pub mod source {
    use metrics::counter;
    use vector_lib::internal_event::InternalEvent;
    use vector_lib::internal_event::{error_stage, error_type};

    #[derive(Debug)]
    pub struct AmqpBytesReceived {
        pub byte_size: usize,
        pub protocol: &'static str,
    }

    impl InternalEvent for AmqpBytesReceived {
        fn emit(self) {
            trace!(
                message = "Bytes received.",
                byte_size = %self.byte_size,
                protocol = %self.protocol,
            );
            counter!(
                "component_received_bytes_total",
                self.byte_size as u64,
                "protocol" => self.protocol,
            );
        }
    }

    #[derive(Debug)]
    pub struct AmqpEventError {
        pub error: lapin::Error,
    }

    impl InternalEvent for AmqpEventError {
        fn emit(self) {
            error!(message = "Failed to read message.",
                   error = ?self.error,
                   error_type = error_type::REQUEST_FAILED,
                   stage = error_stage::RECEIVING,
                   internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::RECEIVING,
            );
        }
    }

    #[derive(Debug)]
    pub struct AmqpAckError {
        pub error: lapin::Error,
    }

    impl InternalEvent for AmqpAckError {
        fn emit(self) {
            error!(message = "Unable to ack.",
                   error = ?self.error,
                   error_type = error_type::ACKNOWLEDGMENT_FAILED,
                   stage = error_stage::RECEIVING,
                   internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::RECEIVING,
            );
        }
    }

    #[derive(Debug)]
    pub struct AmqpRejectError {
        pub error: lapin::Error,
    }

    impl InternalEvent for AmqpRejectError {
        fn emit(self) {
            error!(message = "Unable to reject.",
                   error = ?self.error,
                   error_type = error_type::COMMAND_FAILED,
                   stage = error_stage::RECEIVING,
                   internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::COMMAND_FAILED,
                "stage" => error_stage::RECEIVING,
            );
        }
    }
}

#[cfg(feature = "sinks-amqp")]
pub mod sink {
    use crate::emit;
    use metrics::counter;
    use vector_lib::internal_event::InternalEvent;
    use vector_lib::internal_event::{
        error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL,
    };

    #[derive(Debug)]
    pub struct AmqpDeliveryError<'a> {
        pub error: &'a lapin::Error,
    }

    impl InternalEvent for AmqpDeliveryError<'_> {
        fn emit(self) {
            const DELIVER_REASON: &str = "Unable to deliver.";

            error!(message = DELIVER_REASON,
                   error = ?self.error,
                   error_type = error_type::REQUEST_FAILED,
                   stage = error_stage::SENDING,
                   internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::SENDING,
            );
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: 1,
                reason: DELIVER_REASON
            });
        }
    }

    #[derive(Debug)]
    pub struct AmqpAcknowledgementError<'a> {
        pub error: &'a lapin::Error,
    }

    impl InternalEvent for AmqpAcknowledgementError<'_> {
        fn emit(self) {
            const ACK_REASON: &str = "Acknowledgement failed.";

            error!(message = ACK_REASON,
                   error = ?self.error,
                   error_type = error_type::REQUEST_FAILED,
                   stage = error_stage::SENDING,
                   internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::SENDING,
            );
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: 1,
                reason: ACK_REASON
            });
        }
    }

    #[derive(Debug)]
    pub struct AmqpNackError;

    impl InternalEvent for AmqpNackError {
        fn emit(self) {
            const DELIVER_REASON: &str = "Received Negative Acknowledgement from AMQP broker.";
            error!(
                message = DELIVER_REASON,
                error_type = error_type::ACKNOWLEDGMENT_FAILED,
                stage = error_stage::SENDING,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::SENDING,
            );
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: 1,
                reason: DELIVER_REASON
            });
        }
    }
}

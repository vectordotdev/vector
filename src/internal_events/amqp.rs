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
                "protocol" => self.protocol,
            )
            .increment(self.byte_size as u64);
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
                "component_errors_total",
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::RECEIVING,
            )
            .increment(1);
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
                "component_errors_total",
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::RECEIVING,
            )
            .increment(1);
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
                "component_errors_total",
                "error_type" => error_type::COMMAND_FAILED,
                "stage" => error_stage::RECEIVING,
            )
            .increment(1);
        }
    }
}

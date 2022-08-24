#[cfg(feature = "sources-amqp")]
pub mod source {
    use crate::internal_events::prelude::{error_stage, error_type};
    use metrics::counter;
    use vector_core::internal_event::InternalEvent;

    #[derive(Debug)]
    pub struct AMQPEventReceived {
        pub byte_size: usize,
    }

    impl InternalEvent for AMQPEventReceived {
        fn emit(self) {
            trace!(message = "Received one event.", internal_log_rate_secs = 10);
            counter!("processed_events_total", 1);
            counter!("processed_bytes_total", self.byte_size as u64);
        }
    }

    #[derive(Debug)]
    pub struct AMQPEventError {
        pub error: lapin::Error,
    }

    impl InternalEvent for AMQPEventError {
        fn emit(self) {
            error!(message = "Failed to read message.",
                   error = ?self.error,
                   error_type = error_type::REQUEST_FAILED,
                   stage = error_stage::RECEIVING,
                   internal_log_rate_secs = 10
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::RECEIVING,
            );
        }
    }
}

#[cfg(feature = "sinks-amqp")]
pub mod sink {
    use crate::internal_events::prelude::{error_stage, error_type};
    use metrics::counter;
    use vector_core::internal_event::InternalEvent;

    #[derive(Debug)]
    pub struct AMQPDeliveryError<'a> {
        pub error: &'a lapin::Error,
    }

    impl InternalEvent for AMQPDeliveryError<'_> {
        fn emit(self) {
            error!(message = "Unable to deliver.",
                   error = ?self.error,
                   error_type = error_type::REQUEST_FAILED,
                   stage = error_stage::SENDING,
                   internal_log_rate_secs = 10
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::SENDING,
            );
            counter!(
                "component_discarded_events_total", 1,
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::SENDING,
                "intentional" => "false",
            );
        }
    }

    #[derive(Debug)]
    pub struct AMQPAcknowledgementError<'a> {
        pub error: &'a lapin::Error,
    }

    impl InternalEvent for AMQPAcknowledgementError<'_> {
        fn emit(self) {
            error!(message = "Acknowledgement failed.",
                   error = ?self.error,
                   error_type = error_type::REQUEST_FAILED,
                   stage = error_stage::SENDING,
                   internal_log_rate_secs = 10
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::SENDING,
            );
            counter!(
                "component_discarded_events_total", 1,
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::SENDING,
                "intentional" => "false",
            );
        }

        fn name(&self) -> Option<&'static str> {
            None
        }
    }
}

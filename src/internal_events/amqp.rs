#[cfg(feature = "sources-amqp")]
pub mod source {
    use crate::internal_events::prelude::{error_stage, error_type};
    use metrics::counter;
    use vector_core::internal_event::InternalEvent;

    #[derive(Debug)]
    pub struct AMQPBytesReceived {
        pub byte_size: usize,
        pub protocol: &'static str,
    }

    impl InternalEvent for AMQPBytesReceived {
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
    pub struct AMQPEventsReceived {
        pub byte_size: usize,
    }

    impl InternalEvent for AMQPEventsReceived {
        fn emit(self) {
            trace!(message = "Events received.", internal_log_rate_secs = 10);
            counter!("component_received_events_total", 1);
            counter!(
                "component_received_event_bytes_total",
                self.byte_size as u64
            );

            // deprecated
            counter!("events_in_total", 1);
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

    #[derive(Debug)]
    pub struct AMQPAckError {
        pub error: lapin::Error,
    }

    impl InternalEvent for AMQPAckError {
        fn emit(self) {
            error!(message = "Unable to ack.",
                   error = ?self.error,
                   error_type = error_type::ACKNOWLEDGMENT_FAILED,
                   stage = error_stage::RECEIVING,
                   internal_log_rate_secs = 10
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::RECEIVING,
            );
        }
    }

    #[derive(Debug)]
    pub struct AMQPRejectError {
        pub error: lapin::Error,
    }

    impl InternalEvent for AMQPRejectError {
        fn emit(self) {
            error!(message = "Unable to reject.",
                   error = ?self.error,
                   error_type = error_type::COMMAND_FAILED,
                   stage = error_stage::RECEIVING,
                   internal_log_rate_secs = 10
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
    use crate::{
        emit,
        internal_events::{
            prelude::{error_stage, error_type},
            ComponentEventsDropped, UNINTENTIONAL,
        },
    };
    use metrics::counter;
    use vector_core::internal_event::InternalEvent;

    #[derive(Debug)]
    pub struct AMQPDeliveryError<'a> {
        pub error: &'a lapin::Error,
    }

    impl InternalEvent for AMQPDeliveryError<'_> {
        fn emit(self) {
            let reason = "Unable to deliver.";

            error!(message = reason,
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
            emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
        }
    }

    #[derive(Debug)]
    pub struct AMQPAcknowledgementError<'a> {
        pub error: &'a lapin::Error,
    }

    impl InternalEvent for AMQPAcknowledgementError<'_> {
        fn emit(self) {
            let reason = "Acknowledgement failed.";

            error!(message = reason,
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
            emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
        }
    }
}

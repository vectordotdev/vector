#[cfg(feature = "sources-amqp")]
pub mod source {
    use crate::internal_events::prelude::{error_stage, error_type};
    use metrics::counter;
    use vector_core::internal_event::InternalEvent;

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
    pub struct AmqpEventsReceived {
        pub byte_size: usize,
        pub count: usize,
    }

    impl InternalEvent for AmqpEventsReceived {
        fn emit(self) {
            trace!(
                message = "Events received.",
                count = self.count,
                byte_size = self.byte_size
            );
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
    pub struct AmqpEventError {
        pub error: lapin::Error,
    }

    impl InternalEvent for AmqpEventError {
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
    pub struct AmqpAckError {
        pub error: lapin::Error,
    }

    impl InternalEvent for AmqpAckError {
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
    pub struct AmqpRejectError {
        pub error: lapin::Error,
    }

    impl InternalEvent for AmqpRejectError {
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
    pub struct AmqpDeliveryError<'a> {
        pub error: &'a lapin::Error,
    }

    impl InternalEvent for AmqpDeliveryError<'_> {
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
    pub struct AmqpAcknowledgementError<'a> {
        pub error: &'a lapin::Error,
    }

    impl InternalEvent for AmqpAcknowledgementError<'_> {
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

#[cfg(feature = "sources-amqp")]
pub mod source {
    use crate::source_sender::ClosedError;
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
    pub struct AMQPConsumerFailed {
        pub error: lapin::Error,
    }

    impl InternalEvent for AMQPConsumerFailed {
        fn emit(self) {
            error!(message = "Failed to consume.", error = ?self.error, internal_log_rate_secs = 10);
            counter!("events_consume_failed_total", 1);
        }
    }

    #[derive(Debug)]
    pub struct AMQPEventFailed {
        pub error: lapin::Error,
    }

    impl InternalEvent for AMQPEventFailed {
        fn emit(self) {
            error!(message = "Failed to read message.", error = ?self.error, internal_log_rate_secs = 10);
            counter!("events_failed_total", 1);
        }
    }

    #[derive(Debug)]
    pub struct AMQPKeyExtractionFailed<'a> {
        pub key_field: &'a str,
    }

    impl InternalEvent for AMQPKeyExtractionFailed<'_> {
        fn emit(self) {
            error!(message = "Failed to extract key.", key_field = %self.key_field, internal_log_rate_secs = 10);
        }
    }

    #[derive(Debug)]
    pub struct AMQPDeliveryFailed {
        pub error: ClosedError,
    }

    impl InternalEvent for AMQPDeliveryFailed {
        fn emit(self) {
            error!(message = "Unable to deliver", error = ?self.error, internal_log_rate_secs = 10);
            counter!("consumer_delivery_failed_total", 1);
        }
    }

    #[derive(Debug)]
    pub struct AMQPCommitFailed {
        pub error: lapin::Error,
    }

    impl InternalEvent for AMQPCommitFailed {
        fn emit(self) {
            error!(message = "Unable to ack", error = ?self.error, internal_log_rate_secs = 10);
            counter!("consumer_ack_failed_total", 1);
        }
    }
}

#[cfg(feature = "sinks-amqp")]
pub mod sink {
    use metrics::counter;
    use vector_core::internal_event::InternalEvent;

    #[derive(Debug)]
    pub struct AMQPDeliveryFailed {
        pub error: lapin::Error,
    }

    impl InternalEvent for AMQPDeliveryFailed {
        fn emit(self) {
            error!(message = "Unable to deliver", error = ?self.error, internal_log_rate_secs = 10);
            counter!("events_deliver_failed_total", 1);
        }
    }

    #[derive(Debug)]
    pub struct AMQPAcknowledgementFailed {
        pub error: lapin::Error,
    }

    impl InternalEvent for AMQPAcknowledgementFailed {
        fn emit(self) {
            error!(message = "Acknowledgement failed", error = ?self.error, internal_log_rate_secs = 10);
            counter!("events_acknowledgement_failed_total", 1);
        }
    }

    #[derive(Debug, Default)]
    pub struct AMQPNoAcknowledgement;

    impl InternalEvent for AMQPNoAcknowledgement {
        fn emit(self) {
            error!(message = "No acknowledgement", internal_log_rate_secs = 10);
            counter!("events_acknowledgement_failed_total", 1);
        }
    }
}

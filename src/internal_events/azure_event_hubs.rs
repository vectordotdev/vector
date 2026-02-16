#[cfg(feature = "sources-azure_event_hubs")]
pub mod source {
    use metrics::counter;
    use vector_lib::NamedInternalEvent;
    use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

    #[derive(Debug, NamedInternalEvent)]
    pub struct AzureEventHubsReceiveError {
        pub error: String,
    }

    impl InternalEvent for AzureEventHubsReceiveError {
        fn emit(self) {
            error!(
                message = "Error receiving from Event Hubs.",
                error = %self.error,
                error_type = error_type::REQUEST_FAILED,
                stage = error_stage::RECEIVING,
            );
            counter!(
                "component_errors_total",
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::RECEIVING,
            )
            .increment(1);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct AzureEventHubsConnectError {
        pub error: String,
    }

    impl InternalEvent for AzureEventHubsConnectError {
        fn emit(self) {
            error!(
                message = "Failed to open Event Hubs receiver.",
                error = %self.error,
                error_type = error_type::CONNECTION_FAILED,
                stage = error_stage::RECEIVING,
            );
            counter!(
                "component_errors_total",
                "error_type" => error_type::CONNECTION_FAILED,
                "stage" => error_stage::RECEIVING,
            )
            .increment(1);
        }
    }
}

#[cfg(feature = "sinks-azure_event_hubs")]
pub mod sink {
    use metrics::counter;
    use vector_lib::NamedInternalEvent;
    use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

    #[derive(Debug, NamedInternalEvent)]
    pub struct AzureEventHubsSendError {
        pub error: String,
    }

    impl InternalEvent for AzureEventHubsSendError {
        fn emit(self) {
            error!(
                message = "Failed to send event to Event Hubs.",
                error = %self.error,
                error_type = error_type::REQUEST_FAILED,
                stage = error_stage::SENDING,
            );
            counter!(
                "component_errors_total",
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::SENDING,
            )
            .increment(1);
        }
    }
}

#[cfg(feature = "sources-azure_event_hubs")]
pub mod source {
    use metrics::counter;
    use vector_lib::NamedInternalEvent;
    use vector_lib::internal_event::{InternalEvent, error_stage, error_type};
    use vector_lib::json_size::JsonSize;

    #[derive(Debug, NamedInternalEvent)]
    pub struct AzureEventHubsBytesReceived<'a> {
        pub byte_size: usize,
        pub protocol: &'static str,
        pub event_hub_name: &'a str,
        pub partition_id: &'a str,
    }

    impl InternalEvent for AzureEventHubsBytesReceived<'_> {
        fn emit(self) {
            trace!(
                message = "Bytes received.",
                byte_size = %self.byte_size,
                protocol = %self.protocol,
                event_hub_name = self.event_hub_name,
                partition_id = self.partition_id,
            );
            counter!(
                "component_received_bytes_total",
                "protocol" => self.protocol,
                "event_hub_name" => self.event_hub_name.to_string(),
                "partition_id" => self.partition_id.to_string(),
            )
            .increment(self.byte_size as u64);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct AzureEventHubsEventsReceived<'a> {
        pub byte_size: JsonSize,
        pub count: usize,
        pub event_hub_name: &'a str,
        pub partition_id: &'a str,
    }

    impl InternalEvent for AzureEventHubsEventsReceived<'_> {
        fn emit(self) {
            trace!(
                message = "Events received.",
                count = %self.count,
                byte_size = %self.byte_size,
                event_hub_name = self.event_hub_name,
                partition_id = self.partition_id,
            );
            counter!(
                "component_received_events_total",
                "event_hub_name" => self.event_hub_name.to_string(),
                "partition_id" => self.partition_id.to_string(),
            )
            .increment(self.count as u64);
            counter!(
                "component_received_event_bytes_total",
                "event_hub_name" => self.event_hub_name.to_string(),
                "partition_id" => self.partition_id.to_string(),
            )
            .increment(self.byte_size.get() as u64);
        }
    }

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
    use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

    /// Emit Event Hubs-specific labeled metrics (separate from standard Vector sink telemetry).
    pub fn emit_eventhubs_sent_metrics(count: usize, byte_size: usize, event_hub_name: &str, partition_id: &str) {
        counter!(
            "azure_event_hubs_events_sent_total",
            "event_hub_name" => event_hub_name.to_string(),
            "partition_id" => partition_id.to_string(),
        )
        .increment(count as u64);
        counter!(
            "azure_event_hubs_bytes_sent_total",
            "event_hub_name" => event_hub_name.to_string(),
            "partition_id" => partition_id.to_string(),
        )
        .increment(byte_size as u64);
    }

    use vector_lib::NamedInternalEvent;

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

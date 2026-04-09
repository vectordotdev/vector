#[cfg(feature = "arrow")]
mod arrow_events {
    use metrics::counter;
    use tracing::error;
    use vector_common::NamedInternalEvent;
    use vector_common::internal_event::{InternalEvent, error_stage, error_type};

    #[derive(NamedInternalEvent)]
    pub(crate) struct JsonSerializationError<'a> {
        pub error: &'a serde_json::Error,
    }

    impl InternalEvent for JsonSerializationError<'_> {
        fn emit(self) {
            error!(
                message = "Could not serialize event to JSON.",
                error = %self.error,
                error_type = error_type::ENCODER_FAILED,
                stage = error_stage::SENDING,
                internal_log_rate_limit = true,
            );

            counter!(
                "component_errors_total",
                "error_type" => error_type::ENCODER_FAILED,
                "stage" => error_stage::SENDING,
            )
            .increment(1);
        }
    }
}

#[cfg(feature = "arrow")]
pub(crate) use arrow_events::JsonSerializationError;

#[cfg(feature = "parquet")]
mod parquet_events {
    use metrics::counter;
    use tracing::error;
    use vector_common::NamedInternalEvent;
    use vector_common::internal_event::{InternalEvent, error_stage, error_type};

    #[derive(NamedInternalEvent)]
    pub(crate) struct SchemaGenerationError<'a> {
        pub error: &'a arrow::error::ArrowError,
    }

    impl InternalEvent for SchemaGenerationError<'_> {
        fn emit(self) {
            error!(
                message = "Could not generate schema for batched events",
                error = %self.error,
                error_type = error_type::ENCODER_FAILED,
                stage = error_stage::SENDING,
                internal_log_rate_limit = false,
            );

            counter!(
                "component_errors_total",
                "error_type" => error_type::ENCODER_FAILED,
                "stage" => error_stage::SENDING,
            )
            .increment(1);
        }
    }
}

#[cfg(feature = "parquet")]
pub(crate) use parquet_events::SchemaGenerationError;

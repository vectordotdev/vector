use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent, error_stage, error_type},
};

#[derive(Debug, NamedInternalEvent)]
pub struct AwsEcsMetadataRefreshSuccessful;

impl InternalEvent for AwsEcsMetadataRefreshSuccessful {
    fn emit(self) {
        debug!(message = "AWS ECS metadata refreshed.");
        counter!(CounterName::MetadataRefreshSuccessfulTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AwsEcsMetadataRefreshError {
    pub error: crate::Error,
}

impl InternalEvent for AwsEcsMetadataRefreshError {
    fn emit(self) {
        error!(
            message = "AWS ECS metadata refresh failed.",
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        // deprecated
        counter!(CounterName::MetadataRefreshFailedTotal).increment(1);
    }
}

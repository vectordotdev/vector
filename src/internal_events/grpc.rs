use metrics::counter;
use vector_common::internal_event::{error_stage, error_type};
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct GrpcInvalidCompressionSchemeError<'a> {
    pub status: &'a tonic::Status,
}

impl InternalEvent for GrpcInvalidCompressionSchemeError<'_> {
    fn emit(self) {
        error!(
            message = "Invalid compression scheme.",
            error = ?self.status.message(),
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}

#[derive(Debug)]
pub struct GrpcError<E> {
    pub error: E,
}

impl<E> InternalEvent for GrpcError<E>
where
    E: std::fmt::Display,
{
    fn emit(self) {
        error!(
            message = "Grpc error.",
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}

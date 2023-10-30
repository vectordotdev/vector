use std::time::Duration;

use http::response::Response;
use metrics::{counter, histogram};
use tonic::Code;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

const GRPC_STATUS_LABEL: &str = "grpc_status";

#[derive(Debug)]
pub struct GrpcServerRequestReceived;

impl InternalEvent for GrpcServerRequestReceived {
    fn emit(self) {
        counter!("grpc_server_messages_received_total", 1);
    }
}

#[derive(Debug)]
pub struct GrpcServerResponseSent<'a, B> {
    pub response: &'a Response<B>,
    pub latency: Duration,
}

impl<'a, B> InternalEvent for GrpcServerResponseSent<'a, B> {
    fn emit(self) {
        let grpc_code = self
            .response
            .headers()
            .get("grpc-status")
            // The header value is missing on success.
            .map_or(tonic::Code::Ok, |v| tonic::Code::from_bytes(v.as_bytes()));
        let grpc_code = grpc_code_to_name(grpc_code);

        let labels = &[(GRPC_STATUS_LABEL, grpc_code)];
        counter!("grpc_server_messages_sent_total", 1, labels);
        histogram!("grpc_server_handler_duration_seconds", self.latency, labels);
    }
}

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

const fn grpc_code_to_name(code: Code) -> &'static str {
    match code {
        Code::Ok => "Ok",
        Code::Cancelled => "Cancelled",
        Code::Unknown => "Unknown",
        Code::InvalidArgument => "InvalidArgument",
        Code::DeadlineExceeded => "DeadlineExceeded",
        Code::NotFound => "NotFound",
        Code::AlreadyExists => "AlreadyExists",
        Code::PermissionDenied => "PermissionDenied",
        Code::ResourceExhausted => "ResourceExhausted",
        Code::FailedPrecondition => "FailedPrecondition",
        Code::Aborted => "Aborted",
        Code::OutOfRange => "OutOfRange",
        Code::Unimplemented => "Unimplemented",
        Code::Internal => "Internal",
        Code::Unavailable => "Unavailable",
        Code::DataLoss => "DataLoss",
        Code::Unauthenticated => "Unauthenticated",
    }
}

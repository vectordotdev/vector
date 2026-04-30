use std::time::Duration;

use http::response::Response;
use tonic::Code;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{InternalEvent, MetricName, error_stage, error_type};
use vector_lib::{counter, histogram};

const GRPC_STATUS_LABEL: &str = "grpc_status";

#[derive(Debug, NamedInternalEvent)]
pub struct GrpcServerRequestReceived;

impl InternalEvent for GrpcServerRequestReceived {
    fn emit(self) {
        counter!(MetricName::GrpcServerMessagesReceivedTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GrpcServerResponseSent<'a, B> {
    pub response: &'a Response<B>,
    pub latency: Duration,
}

impl<B> InternalEvent for GrpcServerResponseSent<'_, B> {
    fn emit(self) {
        let grpc_code = self
            .response
            .headers()
            .get("grpc-status")
            // The header value is missing on success.
            .map_or(tonic::Code::Ok, |v| tonic::Code::from_bytes(v.as_bytes()));
        let grpc_code = grpc_code_to_name(grpc_code);

        let labels = &[(GRPC_STATUS_LABEL, grpc_code)];
        counter!(MetricName::GrpcServerMessagesSentTotal, labels).increment(1);
        histogram!(MetricName::GrpcServerHandlerDurationSeconds, labels).record(self.latency);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GrpcInvalidCompressionSchemeError<'a> {
    pub status: &'a tonic::Status,
}

impl InternalEvent for GrpcInvalidCompressionSchemeError<'_> {
    fn emit(self) {
        error!(
            message = "Invalid compression scheme.",
            error = ?self.status.message(),
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING
        );
        counter!(
            MetricName::ComponentErrorsTotal,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
            stage = error_stage::RECEIVING
        );
        counter!(
            MetricName::ComponentErrorsTotal,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
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

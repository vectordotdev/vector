#[derive(Debug, Clone)]
pub struct VectorGrpcRetryLogic;

impl crate::sinks::util::retries::RetryLogic for VectorGrpcRetryLogic {
    type Error = super::sink_error::VectorSinkError;
    type Request = super::service::VectorRequest;
    type Response = super::service::VectorResponse;

    fn is_retriable_error(&self, err: &Self::Error) -> bool {
        match err {
            super::sink_error::VectorSinkError::Request { source } => !matches!(
                source.code(),
                // List taken from
                //
                // <https://github.com/grpc/grpc/blob/ed1b20777c69bd47e730a63271eafc1b299f6ca0/doc/statuscodes.md>
                tonic::Code::NotFound
                    | tonic::Code::InvalidArgument
                    | tonic::Code::AlreadyExists
                    | tonic::Code::PermissionDenied
                    | tonic::Code::OutOfRange
                    | tonic::Code::Unimplemented
                    | tonic::Code::Unauthenticated
                    | tonic::Code::DataLoss
            ),
            _ => true,
        }
    }
}

pub fn is_retriable_vector_error(error: &crate::Error) -> bool {
    error
        .downcast_ref::<super::sink_error::VectorSinkError>()
        .is_none_or(|error| {
            crate::sinks::util::retries::RetryLogic::is_retriable_error(
                &VectorGrpcRetryLogic,
                error,
            )
        })
}

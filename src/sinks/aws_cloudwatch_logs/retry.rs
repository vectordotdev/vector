use std::marker::PhantomData;

use aws_sdk_cloudwatchlogs::operation::create_log_stream::CreateLogStreamError;
use aws_sdk_cloudwatchlogs::operation::describe_log_streams::DescribeLogStreamsError;
use aws_sdk_cloudwatchlogs::operation::put_log_events::PutLogEventsError;
use aws_smithy_runtime_api::client::result::SdkError;

use crate::aws::is_retriable_error;
use crate::sinks::{aws_cloudwatch_logs::service::CloudwatchError, util::retries::RetryLogic};

#[derive(Debug)]
pub struct CloudwatchRetryLogic<Request, Response> {
    request: PhantomData<Request>,
    response: PhantomData<Response>,
}
impl<Request, Response> CloudwatchRetryLogic<Request, Response> {
    pub const fn new() -> CloudwatchRetryLogic<Request, Response> {
        CloudwatchRetryLogic {
            request: PhantomData,
            response: PhantomData,
        }
    }
}

impl<Request, Response> Clone for CloudwatchRetryLogic<Request, Response> {
    fn clone(&self) -> Self {
        CloudwatchRetryLogic {
            request: PhantomData,
            response: PhantomData,
        }
    }
}

impl<Request: Send + Sync + 'static, Response: Send + Sync + 'static> RetryLogic
    for CloudwatchRetryLogic<Request, Response>
{
    type Error = CloudwatchError;
    type Request = Request;
    type Response = Response;

    // TODO this match may not be necessary given the logic in `is_retriable_error()`
    #[allow(clippy::cognitive_complexity)] // long, but just a hair over our limit
    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            CloudwatchError::Put(err) => {
                if let SdkError::ServiceError(inner) = err {
                    let err = inner.err();
                    if matches!(err, PutLogEventsError::ServiceUnavailableException(_)) {
                        return true;
                    }
                }
                is_retriable_error(err)
            }
            CloudwatchError::DescribeLogStreams(err) => {
                if let SdkError::ServiceError(inner) = err {
                    let err = inner.err();
                    if matches!(err, DescribeLogStreamsError::ServiceUnavailableException(_)) {
                        return true;
                    }
                }
                is_retriable_error(err)
            }
            CloudwatchError::CreateStream(err) => {
                if let SdkError::ServiceError(inner) = err {
                    let err = inner.err();
                    if matches!(err, CreateLogStreamError::ServiceUnavailableException(_)) {
                        return true;
                    }
                }
                is_retriable_error(err)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod test {
    use aws_sdk_cloudwatchlogs::operation::put_log_events::PutLogEventsError;
    use aws_smithy_runtime_api::{
        client::{orchestrator::HttpResponse, result::SdkError},
        http::StatusCode,
    };
    use aws_smithy_types::body::SdkBody;

    use crate::sinks::aws_cloudwatch_logs::{
        retry::CloudwatchRetryLogic, service::CloudwatchError,
    };
    use crate::sinks::util::retries::RetryLogic;

    #[test]
    fn test_throttle_retry() {
        let retry_logic: CloudwatchRetryLogic<(), ()> = CloudwatchRetryLogic::new();

        let meta_err = aws_smithy_types::error::ErrorMetadata::builder()
            .code("ThrottlingException")
            .message("Rate exceeded for logStreamName log-test-1.us-east-1.compute.internal")
            .build();

        let body = SdkBody::from("{\"__type\":\"ThrottlingException\",\"message\":\"Rate exceeded for logStreamName log-test-1.us-east-1.compute.internal\"}");

        let raw = HttpResponse::new(StatusCode::try_from(400_u16).unwrap(), body);

        let err = CloudwatchError::Put(SdkError::service_error(
            PutLogEventsError::generic(meta_err),
            raw,
        ));
        assert!(retry_logic.is_retriable_error(&err));
    }
}

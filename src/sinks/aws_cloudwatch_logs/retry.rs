use std::marker::PhantomData;

use aws_sdk_cloudwatchlogs::error::{
    CreateLogStreamErrorKind, DescribeLogStreamsErrorKind, PutLogEventsErrorKind,
};
use aws_sdk_cloudwatchlogs::types::SdkError;

use crate::aws::is_retriable_error;
use crate::sinks::{aws_cloudwatch_logs::service::CloudwatchError, util::retries::RetryLogic};

#[derive(Debug)]
pub struct CloudwatchRetryLogic<T> {
    phantom: PhantomData<T>,
}
impl<T> CloudwatchRetryLogic<T> {
    pub const fn new() -> CloudwatchRetryLogic<T> {
        CloudwatchRetryLogic {
            phantom: PhantomData,
        }
    }
}

impl<T> Clone for CloudwatchRetryLogic<T> {
    fn clone(&self) -> Self {
        CloudwatchRetryLogic {
            phantom: PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static> RetryLogic for CloudwatchRetryLogic<T> {
    type Error = CloudwatchError;
    type Response = T;

    #[allow(clippy::cognitive_complexity)] // long, but just a hair over our limit
    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            CloudwatchError::Put(err) => {
                if let SdkError::ServiceError(inner) = err {
                    let err = inner.err();
                    if let PutLogEventsErrorKind::ServiceUnavailableException(_) = err.kind {
                        return true;
                    }
                }
                is_retriable_error(err)
            }
            CloudwatchError::Describe(err) => {
                if let SdkError::ServiceError(inner) = err {
                    let err = inner.err();
                    if let DescribeLogStreamsErrorKind::ServiceUnavailableException(_) = err.kind {
                        return true;
                    }
                }
                is_retriable_error(err)
            }
            CloudwatchError::CreateStream(err) => {
                if let SdkError::ServiceError(inner) = err {
                    let err = inner.err();
                    if let CreateLogStreamErrorKind::ServiceUnavailableException(_) = err.kind {
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
    use aws_sdk_cloudwatchlogs::error::PutLogEventsError;
    use aws_sdk_cloudwatchlogs::types::SdkError;
    use aws_smithy_http::body::SdkBody;
    use aws_smithy_http::operation::Response;

    use crate::sinks::aws_cloudwatch_logs::retry::CloudwatchRetryLogic;
    use crate::sinks::aws_cloudwatch_logs::service::CloudwatchError;
    use crate::sinks::util::retries::RetryLogic;

    #[test]
    fn test_throttle_retry() {
        let retry_logic: CloudwatchRetryLogic<()> = CloudwatchRetryLogic::new();

        let meta_err = aws_smithy_types::Error::builder()
            .code("ThrottlingException")
            .message("Rate exceeded for logStreamName log-test-1.us-east-1.compute.internal")
            .request_id("0ac34e43-f6ff-4e1b-96be-7d03b2be8376")
            .build();

        let mut http_response = http::Response::new(SdkBody::from("{\"__type\":\"ThrottlingException\",\"message\":\"Rate exceeded for logStreamName log-test-1.us-east-1.compute.internal\"}"));
        *http_response.status_mut() = http::StatusCode::BAD_REQUEST;
        let raw = Response::new(http_response);

        let err = CloudwatchError::Put(SdkError::service_error(
            PutLogEventsError::unhandled(meta_err),
            raw,
        ));
        assert!(retry_logic.is_retriable_error(&err));
    }
}

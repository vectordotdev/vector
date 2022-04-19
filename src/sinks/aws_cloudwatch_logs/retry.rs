use crate::aws::is_retriable_error;
use aws_sdk_cloudwatchlogs::error::{
    CreateLogStreamErrorKind, DescribeLogStreamsErrorKind, PutLogEventsErrorKind,
};
use aws_sdk_cloudwatchlogs::types::SdkError;
use std::marker::PhantomData;

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
            CloudwatchError::Put(sdk_err) => match sdk_err {
                SdkError::ServiceError { err, raw: _ } => match err.kind {
                    PutLogEventsErrorKind::ServiceUnavailableException(_) => {
                        error!(message = "Put logs service unavailable.", %error);
                        true
                    }
                    _ => is_retriable_error(sdk_err),
                },
                SdkError::DispatchFailure(_err) => {
                    error!(message = "Put logs HTTP dispatch.", %error);
                    true
                }

                SdkError::ResponseError { err: _, raw } => {
                    let status = raw.http().status();
                    if status.is_server_error() || status == http::StatusCode::TOO_MANY_REQUESTS {
                        let body = raw.http().body().bytes().unwrap_or(&[]);
                        let truncated_body = String::from_utf8_lossy(&body[..body.len().min(50)]);
                        error!(message = "Put logs HTTP error.", status = %status, body = %truncated_body);
                        true
                    } else {
                        false
                    }
                }
                SdkError::ConstructionFailure(_) => false,
                SdkError::TimeoutError(_) => true,
            },
            CloudwatchError::Describe(sdk_err) => match sdk_err {
                SdkError::ServiceError { err, raw: _ } => match err.kind {
                    DescribeLogStreamsErrorKind::ServiceUnavailableException(_) => {
                        error!(message = "Describe streams service unavailable.", %error);
                        true
                    }
                    _ => is_retriable_error(sdk_err),
                },
                SdkError::TimeoutError(_) => true,
                SdkError::DispatchFailure(_) => {
                    error!(message = "Describe streams HTTP dispatch.", %error);
                    true
                }
                SdkError::ResponseError { err: _, raw } => {
                    let status = raw.http().status();
                    if status.is_server_error() || status == http::StatusCode::TOO_MANY_REQUESTS {
                        let body = raw.http().body().bytes().unwrap_or(&[]);
                        let truncated_body = String::from_utf8_lossy(&body[..body.len().min(50)]);
                        error!(message = "Describe streams HTTP error.", status = %status, body = %truncated_body);
                        true
                    } else {
                        false
                    }
                }
                SdkError::ConstructionFailure(_) => false,
            },
            CloudwatchError::CreateStream(sdk_err) => match sdk_err {
                SdkError::ServiceError { err, raw: _ } => match err.kind {
                    CreateLogStreamErrorKind::ServiceUnavailableException(_) => {
                        error!(message = "Create stream service unavailable.", %error);
                        true
                    }
                    _ => is_retriable_error(sdk_err),
                },
                SdkError::TimeoutError(_) => true,
                SdkError::DispatchFailure(_) => {
                    error!(message = "Create stream HTTP dispatch.", %error);
                    true
                }
                SdkError::ResponseError { err: _, raw } => {
                    let status = raw.http().status();
                    if status.is_server_error() || status == http::StatusCode::TOO_MANY_REQUESTS {
                        let body = raw.http().body().bytes().unwrap_or(&[]);
                        let truncated_body = String::from_utf8_lossy(&body[..body.len().min(50)]);
                        error!(message = "Create stream HTTP error.", status = %status, body = %truncated_body);
                        true
                    } else {
                        false
                    }
                }
                SdkError::ConstructionFailure(_) => false,
            },
            _ => false,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::sinks::aws_cloudwatch_logs::retry::CloudwatchRetryLogic;
    use crate::sinks::aws_cloudwatch_logs::service::CloudwatchError;
    use crate::sinks::util::retries::RetryLogic;
    use aws_sdk_cloudwatchlogs::error::{PutLogEventsError, PutLogEventsErrorKind};
    use aws_sdk_cloudwatchlogs::types::SdkError;
    use aws_smithy_http::body::SdkBody;
    use aws_smithy_http::operation::Response;

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

        let err = CloudwatchError::Put(SdkError::ServiceError {
            err: PutLogEventsError::new(
                PutLogEventsErrorKind::Unhandled(Box::new(meta_err.clone())),
                meta_err,
            ),
            raw,
        });
        assert!(retry_logic.is_retriable_error(&err));
    }
}

use aws_sdk_sqs::error::SendMessageError;
use aws_sdk_sqs::types::SdkError;

use super::service::SendMessageResponse;
use crate::aws::aws_sdk::is_retriable_error;
use crate::sinks::util::retries::RetryLogic;

#[derive(Debug, Clone)]
pub(super) struct SqsRetryLogic;

impl RetryLogic for SqsRetryLogic {
    type Error = SdkError<SendMessageError>;
    type Response = SendMessageResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        is_retriable_error(error)
    }
}

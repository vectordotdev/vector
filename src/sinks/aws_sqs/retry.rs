use rusoto_core::RusotoError;
use rusoto_sqs::SendMessageError;

use super::service::SendMessageResponse;
use crate::aws::rusoto;
use crate::sinks::util::retries::RetryLogic;

#[derive(Debug, Clone)]
pub(super) struct SqsRetryLogic;

impl RetryLogic for SqsRetryLogic {
    type Error = RusotoError<SendMessageError>;
    type Response = SendMessageResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        rusoto::is_retriable_error(error)
    }
}

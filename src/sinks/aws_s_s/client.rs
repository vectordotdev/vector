use super::{request_builder::SendMessageEntry, service::SendMessageResponse};
use aws_sdk_sqs::types::SdkError;

#[async_trait::async_trait]
pub(super) trait Client<R>
where
    R: std::fmt::Debug + std::fmt::Display + std::error::Error,
{
    async fn send_message(
        &self,
        entry: SendMessageEntry,
        byte_size: usize,
    ) -> Result<SendMessageResponse, SdkError<R>>;
}

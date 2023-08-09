use super::{request_builder::SendMessageEntry, service::SendMessageResponse};
use aws_sdk_sqs::{error::SendMessageError, types::SdkError};

#[async_trait::async_trait]
pub trait Client {
    async fn send_message(
        &self,
        entry: SendMessageEntry,
        byte_size: usize,
    ) -> Result<SendMessageResponse, SdkError<SendMessageError>>;
}

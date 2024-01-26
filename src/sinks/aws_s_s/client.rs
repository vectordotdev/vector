use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};

use super::{request_builder::SendMessageEntry, service::SendMessageResponse};

#[async_trait::async_trait]
pub(super) trait Client<R>
where
    R: std::fmt::Debug + std::fmt::Display + std::error::Error,
{
    async fn send_message(
        &self,
        entry: SendMessageEntry,
        byte_size: usize,
    ) -> Result<SendMessageResponse, SdkError<R, HttpResponse>>;
}

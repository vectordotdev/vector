use std::future::Future;

use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};

use super::{request_builder::SendMessageEntry, service::SendMessageResponse};

pub(super) trait Client<R>
where
    R: std::fmt::Debug + std::fmt::Display + std::error::Error,
{
    fn send_message(
        &self,
        entry: SendMessageEntry,
        byte_size: usize,
    ) -> impl Future<Output = Result<SendMessageResponse, SdkError<R, HttpResponse>>> + Send;
}

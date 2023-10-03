use aws_sdk_sqs::types::SdkError;
use std::marker::PhantomData;

use super::service::SendMessageResponse;
use crate::{aws::is_retriable_error, sinks::util::retries::RetryLogic};

#[derive(Debug)]
pub(super) struct SSRetryLogic<E> {
    _phantom: PhantomData<fn() -> E>,
}

impl<E> SSRetryLogic<E>
where
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    pub(super) fn new() -> SSRetryLogic<E> {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<E> RetryLogic for SSRetryLogic<E>
where
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    type Error = SdkError<E>;
    type Response = SendMessageResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        is_retriable_error(error)
    }
}

impl<E> Clone for SSRetryLogic<E>
where
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    fn clone(&self) -> SSRetryLogic<E> {
        SSRetryLogic {
            _phantom: PhantomData,
        }
    }
}

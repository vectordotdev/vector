use std::marker::PhantomData;

use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};

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
    type Error = SdkError<E, HttpResponse>;
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

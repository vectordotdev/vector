use std::{
    sync::Arc,
    task::{Context, Poll},
};

use iggy::prelude::{IggyMessage, IggyProducer};
use snafu::ResultExt;

use super::{IggyError, ProducerSnafu, request_builder::IggyRequest};
use crate::{internal_events::IggySendError, sinks::prelude::*};

#[derive(Clone)]
pub(super) struct IggyService {
    pub(super) producer: Arc<IggyProducer>,
}

pub(super) struct IggyResponse {
    metadata: RequestMetadata,
}

impl DriverResponse for IggyResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        self.metadata.events_estimated_json_encoded_byte_size()
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.metadata.request_encoded_size())
    }
}

impl Service<IggyRequest> for IggyService {
    type Response = IggyResponse;
    type Error = IggyError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: IggyRequest) -> Self::Future {
        let producer = Arc::clone(&self.producer);

        Box::pin(async move {
            let event_count = req.metadata.event_count();
            let messages = req
                .payloads
                .into_iter()
                .map(|payload| IggyMessage::builder().payload(payload).build())
                .collect::<Result<Vec<_>, _>>()
                .inspect_err(|source| {
                    emit!(IggySendError {
                        count: event_count,
                        error: source,
                    });
                })
                .context(ProducerSnafu)?;

            producer
                .send(messages)
                .await
                .inspect_err(|source| {
                    emit!(IggySendError {
                        count: event_count,
                        error: source,
                    });
                })
                .context(ProducerSnafu)?;

            Ok(IggyResponse {
                metadata: req.metadata,
            })
        })
    }
}

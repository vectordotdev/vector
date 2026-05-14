use std::{
    sync::Arc,
    task::{Context, Poll},
};

use iggy::prelude::{IggyMessage, IggyProducer};
use snafu::ResultExt;

use super::{IggyError, ProducerSnafu, request_builder::IggyRequest};
use crate::sinks::prelude::*;

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
            // Errors propagate to the Driver, which emits `CallError` (and
            // `ComponentEventsDropped::<UNINTENTIONAL>`) on the terminal
            // failure after Tower retries are exhausted. Emitting here would
            // double-count every transient failure that succeeds on retry.
            let messages = req
                .payloads
                .into_iter()
                .map(|payload| IggyMessage::builder().payload(payload).build())
                .collect::<Result<Vec<_>, _>>()
                .context(ProducerSnafu)?;

            producer.send(messages).await.context(ProducerSnafu)?;

            Ok(IggyResponse {
                metadata: req.metadata,
            })
        })
    }
}

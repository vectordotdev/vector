use std::{
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};

use bytes::Bytes;
use iggy::prelude::{Client, IggyMessage};
use snafu::ResultExt;

use super::{
    ConnectSnafu, IggyError, ProducerSnafu, config::Connection, request_builder::IggyRequest,
};
use crate::common::iggy::{
    RESOLVE_TIMEOUT, is_unauthenticated, login_if_http, logs_in_over_http, retained_operation,
};
use crate::sinks::prelude::*;

#[derive(Clone)]
pub(super) struct IggyService {
    connection: Arc<Connection>,
}

impl IggyService {
    pub(super) const fn new(connection: Arc<Connection>) -> Self {
        Self { connection }
    }
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
        let connection = Arc::clone(&self.connection);

        Box::pin(async move {
            // The sink counts a failed request once, after the retry layer
            // gives up; emitting here would count every attempt.
            let IggyRequest {
                payloads, metadata, ..
            } = req;

            let (client, producer) = match connection.get().await {
                Ok(pair) => pair,
                Err(IggyError::Connect { source } | IggyError::Producer { source })
                    if is_unauthenticated(&source) && logs_in_over_http(connection.url()) =>
                {
                    // The producer is created on the first event, so an HTTP
                    // token that expired since the healthcheck fails in
                    // `init()` before any send can reach the re-login branch.
                    let client = connection.client().await?;
                    login_if_http(&client, connection.url())
                        .await
                        .map_err(|source| IggyError::Connect { source })?;
                    connection.get().await?
                }
                Err(error) => return Err(error),
            };

            let result = producer.send(build_messages(&payloads)?).await;
            match result {
                Ok(_) => Ok(IggyResponse { metadata }),
                Err(error) if is_unauthenticated(&error) && logs_in_over_http(connection.url()) => {
                    // HTTP access tokens expire and the SDK never renews them,
                    // so log in again and resend once instead of failing every
                    // later batch permanently.
                    login_if_http(&client, connection.url())
                        .await
                        .context(ConnectSnafu)?;
                    producer
                        .send(build_messages(&payloads)?)
                        .await
                        .context(ProducerSnafu)?;
                    Ok(IggyResponse { metadata })
                }
                Err(error) => match retained_operation(&error) {
                    Some(op) => {
                        // A timed-out send leaves its chunk retained in the
                        // partition lane; the SDK resolves only operations it
                        // registered, and a cancelled send never does.
                        client
                            .resolve_operation(op, Instant::now() + RESOLVE_TIMEOUT)
                            .await
                            .or_else(|error| match error {
                                // Already released by a session change or a
                                // concurrent resolve.
                                iggy::prelude::IggyError::OperationNotRetained(_) => Ok(()),
                                error => Err(error),
                            })
                            .context(ProducerSnafu)?;
                        producer
                            .send(build_messages(&payloads)?)
                            .await
                            .context(ProducerSnafu)?;
                        Ok(IggyResponse { metadata })
                    }
                    None => Err(error).context(ProducerSnafu),
                },
            }
        })
    }
}

fn build_messages(payloads: &[Bytes]) -> Result<Vec<IggyMessage>, IggyError> {
    payloads
        .iter()
        .map(|payload| {
            IggyMessage::builder()
                .payload(payload.clone())
                .build()
                .context(ProducerSnafu)
        })
        .collect()
}

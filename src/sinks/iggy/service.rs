use std::{
    sync::{Arc, atomic::Ordering},
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

/// Send rounds one request may use while draining a confirmed prefix; past
/// that the retry layer takes over.
const MAX_SEND_ROUNDS: usize = 4;

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
                payloads,
                metadata,
                sent,
                ..
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

            // The SDK commits a send in chunks and reports the confirmed prefix
            // with a failure. Resend only the unconfirmed tail, and keep the
            // cursor on the request so the retry layer's clone skips the
            // chunks an earlier attempt already committed.
            let total = payloads.len();
            let mut start = sent.load(Ordering::Relaxed).min(total);
            let mut last_error = None;
            for _ in 0..MAX_SEND_ROUNDS {
                let remaining = &payloads[start..];
                match producer.send(build_messages(remaining)?).await {
                    Ok(_) => {
                        sent.store(total, Ordering::Relaxed);
                        return Ok(IggyResponse { metadata });
                    }
                    Err(error)
                        if is_unauthenticated(&error) && logs_in_over_http(connection.url()) =>
                    {
                        // HTTP access tokens expire and the SDK never renews
                        // them, so log in again instead of failing every later
                        // batch permanently.
                        login_if_http(&client, connection.url())
                            .await
                            .context(ConnectSnafu)?;
                        last_error = Some(error);
                    }
                    Err(error) => {
                        // A send split into chunks reports the commits before
                        // the failure. They need no resend; only the tail does.
                        let (committed, uncertain) = match &error {
                            iggy::prelude::IggyError::ProducerSendFailed {
                                failed,
                                uncertain,
                                ..
                            } => (remaining.len().saturating_sub(failed.len()), *uncertain),
                            _ => return Err(error).context(ProducerSnafu),
                        };
                        let mut delivered = committed;
                        if let Some(op) = retained_operation(&error) {
                            // A timed-out send leaves its chunk retained in the
                            // partition lane. Resolving it frees the lane, and
                            // on success proves the chunk's `uncertain` head
                            // committed, so those messages need no resend.
                            match client
                                .resolve_operation(op, Instant::now() + RESOLVE_TIMEOUT)
                                .await
                            {
                                Ok(()) => delivered += uncertain,
                                // Already released by a session change or a
                                // concurrent resolve; the lane is free, the
                                // chunk's outcome stays unknown.
                                Err(iggy::prelude::IggyError::OperationNotRetained(_)) => {}
                                Err(error) => return Err(error).context(ProducerSnafu),
                            }
                        }
                        if delivered == 0 {
                            return Err(error).context(ProducerSnafu);
                        }
                        start += delivered;
                        sent.store(start, Ordering::Relaxed);
                        if start == total {
                            return Ok(IggyResponse { metadata });
                        }
                        last_error = Some(error);
                    }
                }
            }
            // The retry layer owns further attempts; hand it the last failure.
            Err(last_error.expect("a round that does not return records its failure"))
                .context(ProducerSnafu)
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

use std::{
    sync::Arc,
    task::{Context, Poll},
};

use iggy::prelude::Client;
use snafu::ResultExt;
use vector_lib::internal_event::emit;

use super::{
    EncodingSnafu, IggyError,
    config::{Connection, IggySinkConfig, IggyTowerRequestConfigDefaults},
    request_builder::{IggyRequest, request_builder},
    service::{IggyResponse, IggyService},
};
use crate::{internal_events::IggySinkRequestFailed, sinks::prelude::*};

pub(super) struct IggySink {
    request: TowerRequestConfig<IggyTowerRequestConfigDefaults>,
    transformer: Transformer,
    encoder: Encoder<()>,
    connection: Arc<Connection>,
    batcher_settings: BatcherSettings,
}

impl IggySink {
    pub(super) fn new(
        config: &IggySinkConfig,
        connection: Arc<Connection>,
    ) -> Result<Self, IggyError> {
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build().context(EncodingSnafu)?;
        let batcher_settings = config
            .batch
            .validate()
            .map_err(|_| IggyError::InvalidBatchSettings)?
            .into_batcher_settings()
            .map_err(|_| IggyError::InvalidBatchSettings)?;
        Ok(IggySink {
            request: config.request,
            transformer,
            encoder: Encoder::<()>::new(serializer),
            connection,
            batcher_settings,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request = self.request.into_settings();

        let service = ServiceBuilder::new()
            .settings(request, IggyRetryLogic)
            .service(IggyService::new(Arc::clone(&self.connection)));
        let service = CountFailedRequests { inner: service };

        let mut encoder = self.encoder.clone();
        let transformer = self.transformer.clone();
        let batcher_settings = self.batcher_settings.as_byte_size_config();

        let result = input
            .batched(batcher_settings)
            .filter_map(|events| {
                futures::future::ready(request_builder(events, &transformer, &mut encoder))
            })
            .into_driver(service)
            .protocol("iggy")
            // The driver finalizes exhausted service failures as `Rejected`,
            // which an acknowledgement-enabled source treats as poison and
            // commits past after three replays. `Errored` keeps an outage
            // retriable; the request builder still reports deterministic
            // payload failures as `Rejected`.
            .with_request_error_status(EventStatus::Errored)
            .run()
            .await;

        if let Some(client) = self.connection.connected_client()
            && let Err(error) = client.disconnect().await
        {
            warn!(message = "Failed to disconnect Iggy client on sink shutdown.", %error);
        }

        result
    }
}

#[async_trait]
impl StreamSink<Event> for IggySink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

/// Counts the requests the retry layer gave up on. It sits outside that layer,
/// so a request that succeeds after a retry is not counted, and it sees the
/// failure the driver finalizes as `Errored` without reporting.
#[derive(Clone)]
struct CountFailedRequests<S> {
    inner: S,
}

impl<S> Service<IggyRequest> for CountFailedRequests<S>
where
    S: Service<IggyRequest> + Send,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: IggyRequest) -> Self::Future {
        let response = self.inner.call(request);
        Box::pin(async move {
            let result = response.await;
            if result.is_err() {
                emit(IggySinkRequestFailed);
            }
            result
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct IggyRetryLogic;

impl RetryLogic for IggyRetryLogic {
    type Error = IggyError;
    type Request = IggyRequest;
    type Response = IggyResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        use iggy::prelude::IggyError as Sdk;
        let source = match error {
            IggyError::Encoding { .. } | IggyError::InvalidBatchSettings => return false,
            IggyError::Connect { source } | IggyError::Producer { source } => source,
        };
        // The SDK's producer wraps send failures from its background send
        // path in `ProducerSendFailed`; unwrap the underlying cause so
        // transient errors (e.g. `Disconnected`, `TcpError`) are still
        // classified as retriable by the allowlist below.
        let source = match source {
            Sdk::ProducerSendFailed { cause, .. } => &**cause,
            source => source,
        };
        // `OperationUncertain` and `TransientNotCommitted` may have applied, so
        // a retry can duplicate the affected messages; Vector's at-least-once
        // delivery accepts that rather than dropping the batch. Unknown
        // variants stay non-retriable so a future fatal SDK error cannot spin
        // in a retry loop; add new transient variants when bumping `iggy`.
        matches!(
            source,
            Sdk::Disconnected
                | Sdk::CannotEstablishConnection
                | Sdk::NotConnected
                | Sdk::ConnectionClosed
                | Sdk::StaleClient
                | Sdk::TcpError
                | Sdk::QuicError
                // Concurrent creators race the SDK's lookup-then-create: the
                // loser sees a stream or topic another instance just created,
                // so a retry finds the resource it was about to create.
                | Sdk::StreamNameAlreadyExists(_)
                | Sdk::TopicNameAlreadyExists(_, _)
                // The HTTP client maps connection and send/read failures to
                // `InvalidHttpRequest` and untyped statuses to
                // `HttpResponseError`; only these statuses are worth another
                // attempt.
                | Sdk::InvalidHttpRequest
                | Sdk::HttpResponseError(408 | 429 | 500..=599, _)
                | Sdk::CannotSendMessagesDueToClientDisconnection
                | Sdk::BackgroundWorkerDisconnected
                | Sdk::BackgroundSendTimeout
                | Sdk::TaskTimeout
                | Sdk::LifecycleBusy
                | Sdk::TransientNotAccepted
                | Sdk::TransientNotCommitted
                | Sdk::OperationUncertain(_)
                // A blocked partition lane leaves its operation retained; the
                // service resolves it and resends before reporting the error,
                // so reaching here means a second send met another retained
                // operation. Each attempt resolves one, so retrying drains
                // them instead of dropping the request.
                | Sdk::LaneBlocked(_)
                | Sdk::PartitionLaneBlocked(_, _)
        )
    }
}

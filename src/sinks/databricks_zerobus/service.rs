//! Zerobus service wrapper for Vector sink integration.

use crate::config::ProxyConfig;
use crate::event::Event;
use crate::http::HttpClient;
use crate::sinks::util::retries::RetryLogic;
use crate::tls::TlsSettings;
use databricks_zerobus_ingest_sdk::{
    ConnectorFactory, ProxyConnector, ZerobusArrowStream, ZerobusSdk,
};
use futures::future::BoxFuture;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell, RwLock};
use tower::{Layer, Service};
use tracing::warn;
use vector_lib::codecs::encoding::{
    ArrowStreamSerializerConfig, BatchEncoder, BatchOutput, BatchSerializerConfig,
};
use vector_lib::finalization::{EventFinalizers, Finalizable};
use vector_lib::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_lib::stream::DriverResponse;

use super::{config::ZerobusSinkConfig, error::ZerobusSinkError, unity_catalog_schema};

/// Build a connector factory that routes Zerobus gRPC traffic through
/// Vector's configured proxy, honoring `no_proxy` rules.
///
/// The Zerobus endpoint is always HTTPS gRPC, so the `https` proxy is
/// preferred; the `http` proxy is used as a fallback if only that is set.
/// The returned factory fully replaces the SDK's default env-var proxy
/// detection — Vector's `ProxyConfig` has already merged the process
/// environment at a higher layer and is the single source of truth.
///
/// When proxying is disabled or no proxy URL is configured, returns a
/// factory that unconditionally yields `None`, forcing direct connections.
/// Returns an error if the configured proxy URL is malformed, so the
/// problem surfaces at sink startup rather than per-connection.
fn build_connector_factory(proxy: &ProxyConfig) -> Result<ConnectorFactory, ZerobusSinkError> {
    let proxy_url = if proxy.enabled {
        proxy.https.clone().or_else(|| proxy.http.clone())
    } else {
        None
    };
    let Some(proxy_url) = proxy_url else {
        return Ok(Arc::new(|_host: &str| None));
    };
    // Validate the proxy URL once up-front so a malformed value surfaces at
    // sink startup rather than per-connection.
    ProxyConnector::new(&proxy_url).map_err(|e| ZerobusSinkError::ConfigError {
        message: format!("Invalid proxy URL '{}': {}", proxy_url, e),
    })?;
    let no_proxy = proxy.no_proxy.clone();
    Ok(Arc::new(move |host: &str| {
        if no_proxy.matches(host) {
            return None;
        }
        ProxyConnector::new(&proxy_url).ok()
    }))
}

/// Request type for the Zerobus service.
///
/// Carries the *unencoded* batch — encoding happens inside `Service::call` so
/// that schema-fetch failures flow through the Tower retry layer. Events live
/// behind an `Arc` because Tower's retry policy clones the request before
/// every call (not just on retry), and a deep clone of `Vec<Event>` per call
/// would be wasteful.
#[derive(Clone)]
pub struct ZerobusRequest {
    pub events: Arc<Vec<Event>>,
    pub metadata: RequestMetadata,
    pub finalizers: EventFinalizers,
}

/// Response type for the Zerobus service.
///
/// Carries the final `EventStatus` so the driver can mark finalizers correctly:
/// `Delivered` on success, `Errored` when the retry budget was exhausted on a
/// transient failure (asking the source / disk buffer to replay), and `Err`
/// from `Service::call` reserved for permanent failures (driver maps to
/// `Rejected`).
#[derive(Debug)]
pub struct ZerobusResponse {
    pub events_byte_size: GroupedCountByteSize,
    pub status: vector_lib::event::EventStatus,
}

impl ZerobusResponse {
    const fn delivered(events_byte_size: GroupedCountByteSize) -> Self {
        Self {
            events_byte_size,
            status: vector_lib::event::EventStatus::Delivered,
        }
    }

    /// Synthesize a response signalling a transient failure that exhausted the
    /// retry budget. Carries a telemetry-aware zero `events_byte_size` because
    /// the driver only consumes `events_sent()` on the `Delivered` path.
    fn errored() -> Self {
        Self {
            events_byte_size: vector_lib::config::telemetry().create_request_count_byte_size(),
            status: vector_lib::event::EventStatus::Errored,
        }
    }
}

impl DriverResponse for ZerobusResponse {
    fn event_status(&self) -> vector_lib::event::EventStatus {
        self.status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }
}

impl Finalizable for ZerobusRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for ZerobusRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

/// The active stream.
///
/// The SDK's `ZerobusArrowStream::close()` requires `&mut self`, but ingests need
/// shared access to call `&self` methods concurrently. We resolve this with an
/// `RwLock`: ingests hold a read guard across `ingest_batch`, and
/// `close()` takes the write guard, pulls the stream out of the `Option`, and
/// awaits its SDK-level close on the owned value. Any holder of an `Arc` can
/// invoke `close()`, so the graceful path always runs — there is no
/// `try_unwrap`/`get_mut` race.
enum ActiveStream {
    Arrow(RwLock<Option<Box<ZerobusArrowStream>>>),
    /// Test-only variant that returns a pre-configured error on ingest.
    #[cfg(test)]
    Mock(MockStream),
}

impl ActiveStream {
    fn arrow(stream: ZerobusArrowStream) -> Self {
        ActiveStream::Arrow(RwLock::new(Some(Box::new(stream))))
    }

    /// Gracefully flush and close the underlying SDK stream.
    ///
    /// Waits for any in-flight ingests (read-lock holders) to complete, then
    /// pulls the stream out of the slot and runs the SDK's awaitable `close()`
    /// on the owned value (released-lock so further ingests fail fast with
    /// `StreamClosed` rather than blocking).
    ///
    /// Idempotent: a second call after the stream has been taken is a no-op.
    /// The SDK's own `Drop` is also a no-op once close has run.
    async fn close(&self) {
        let result = match self {
            ActiveStream::Arrow(lock) => {
                let taken = lock.write().await.take();
                match taken {
                    Some(mut stream) => stream.close().await,
                    None => return,
                }
            }
            #[cfg(test)]
            ActiveStream::Mock(m) => {
                m.closed.store(true, std::sync::atomic::Ordering::Relaxed);
                Ok(())
            }
        };
        if let Err(e) = result {
            warn!(message = "Failed to close Zerobus stream.", error = %e);
        }
    }
}

/// A mock stream that returns a configurable error on the next ingest call.
#[cfg(test)]
pub struct MockStream {
    /// When `Some`, the next ingest returns this error; when `None`, ingest succeeds.
    next_error: std::sync::Mutex<Option<databricks_zerobus_ingest_sdk::ZerobusError>>,
    /// Shared flag set to `true` when `ActiveStream::close()` is called.
    closed: Arc<std::sync::atomic::AtomicBool>,
    /// Optional gate: when set, each ingest call signals `started` and then
    /// waits to acquire a `release` permit before returning. Lets tests
    /// deterministically force two ingests to overlap (each holding an `Arc`
    /// clone of the `ActiveStream`) before they fail.
    gate: Option<MockGate>,
}

#[cfg(test)]
struct MockGate {
    started: Arc<tokio::sync::Semaphore>,
    release: Arc<tokio::sync::Semaphore>,
}

#[cfg(test)]
#[derive(Clone)]
pub struct MockGateHandle {
    started: Arc<tokio::sync::Semaphore>,
    release: Arc<tokio::sync::Semaphore>,
}

#[cfg(test)]
impl MockGateHandle {
    /// Wait until `n` ingests have entered the gated region.
    pub async fn wait_for_started(&self, n: u32) {
        let permit = self.started.acquire_many(n).await.unwrap();
        permit.forget();
    }

    /// Release `n` queued ingests so they can return their result.
    pub fn release(&self, n: u32) {
        self.release.add_permits(n as usize);
    }
}

#[cfg(test)]
impl MockStream {
    pub fn succeeding() -> Self {
        Self {
            next_error: std::sync::Mutex::new(None),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            gate: None,
        }
    }

    pub fn failing(error: databricks_zerobus_ingest_sdk::ZerobusError) -> Self {
        Self {
            next_error: std::sync::Mutex::new(Some(error)),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            gate: None,
        }
    }

    /// Install a gate so ingests block until the test releases them.
    /// Returns a handle the test uses to coordinate.
    pub fn with_gate(mut self) -> (Self, MockGateHandle) {
        let started = Arc::new(tokio::sync::Semaphore::new(0));
        let release = Arc::new(tokio::sync::Semaphore::new(0));
        self.gate = Some(MockGate {
            started: Arc::clone(&started),
            release: Arc::clone(&release),
        });
        (self, MockGateHandle { started, release })
    }

    /// Returns a shared handle to the closed flag for test assertions.
    pub fn closed_flag(&self) -> Arc<std::sync::atomic::AtomicBool> {
        Arc::clone(&self.closed)
    }

    /// Set the error that will be returned on the next ingest call.
    pub fn set_next_error(&self, error: databricks_zerobus_ingest_sdk::ZerobusError) {
        *self.next_error.lock().unwrap() = Some(error);
    }

    async fn try_ingest(&self) -> Result<(), databricks_zerobus_ingest_sdk::ZerobusError> {
        if let Some(gate) = &self.gate {
            gate.started.add_permits(1);
            // Acquire and immediately forget — we don't need to release the
            // permit on drop, the test's `release()` call hands them out.
            gate.release.acquire().await.unwrap().forget();
        }
        match self.next_error.lock().unwrap().take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// Schema and encoding state derived from the Unity Catalog table.
pub(super) struct ResolvedSchema {
    encoder: BatchEncoder,
    /// Arrow schema used to declare the Zerobus stream. Held behind an `Arc` so
    /// each stream rebuild after a retryable failure clones it cheaply, and so it
    /// matches the schema the Arrow batch encoder produces.
    arrow_schema: Arc<arrow::datatypes::Schema>,
}

#[cfg(test)]
impl ResolvedSchema {
    /// Build a `ResolvedSchema` directly from an Arrow schema, mirroring what
    /// `ensure_schema` does after a Unity Catalog fetch. Lets encoding tests
    /// exercise the real `BatchEncoder` without a network round-trip.
    fn for_test(schema: arrow::datatypes::Schema) -> Self {
        let batch_serializer =
            BatchSerializerConfig::ArrowStream(ArrowStreamSerializerConfig::new(schema.clone()))
                .build_batch_serializer()
                .expect("arrow batch serializer should build");
        Self {
            encoder: BatchEncoder::new(batch_serializer),
            arrow_schema: Arc::new(schema),
        }
    }
}

/// Service for handling Zerobus requests.
pub struct ZerobusService {
    sdk: Arc<ZerobusSdk>,
    config: Arc<ZerobusSinkConfig>,
    http_client: HttpClient,
    stream: Arc<Mutex<Option<Arc<ActiveStream>>>>,
    schema: Arc<OnceCell<ResolvedSchema>>,
}

impl ZerobusService {
    pub async fn new(
        config: ZerobusSinkConfig,
        proxy: &ProxyConfig,
    ) -> Result<Self, ZerobusSinkError> {
        let mut builder = ZerobusSdk::builder()
            .endpoint(&config.ingestion_endpoint)
            .unity_catalog_url(&config.unity_catalog_endpoint)
            .application_name(config.user_agent_suffix());
        builder = builder.connector_factory(build_connector_factory(proxy)?);
        let sdk = builder.build().map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to create Zerobus SDK: {}", e),
        })?;

        let http_client = HttpClient::new(TlsSettings::default(), proxy).map_err(|e| {
            ZerobusSinkError::ConfigError {
                message: format!("Failed to create HTTP client: {}", e),
            }
        })?;

        Ok(Self {
            sdk: Arc::new(sdk),
            config: Arc::new(config),
            http_client,
            stream: Arc::new(Mutex::new(None)),
            schema: Arc::new(OnceCell::new()),
        })
    }

    /// Resolve the Arrow schema for the target table from Unity Catalog.
    ///
    /// The returned schema is used both to declare the Zerobus Arrow stream and
    /// to drive the Arrow batch encoder, keeping the encoded `RecordBatch` schema
    /// in lock-step with the stream's declared schema.
    async fn resolve_arrow_schema(
        config: &ZerobusSinkConfig,
        http_client: &HttpClient,
    ) -> Result<arrow::datatypes::Schema, ZerobusSinkError> {
        let (client_id, client_secret) = config.auth.credentials();

        let table_schema = unity_catalog_schema::fetch_table_schema(
            &config.unity_catalog_endpoint,
            &config.table_name,
            client_id,
            client_secret,
            http_client,
        )
        .await?;

        unity_catalog_schema::generate_arrow_schema_from_schema(&table_schema)
    }

    /// Resolve the schema on first use; cache the result.
    pub(super) async fn ensure_schema(&self) -> Result<&ResolvedSchema, ZerobusSinkError> {
        self.schema
            .get_or_try_init(|| async {
                let arrow_schema =
                    Self::resolve_arrow_schema(&self.config, &self.http_client).await?;

                let batch_serializer = BatchSerializerConfig::ArrowStream(
                    ArrowStreamSerializerConfig::new(arrow_schema.clone()),
                )
                .build_batch_serializer()
                .map_err(|e| ZerobusSinkError::ConfigError {
                    message: format!("Failed to build batch serializer: {}", e),
                })?;

                Ok(ResolvedSchema {
                    encoder: BatchEncoder::new(batch_serializer),
                    arrow_schema: Arc::new(arrow_schema),
                })
            })
            .await
    }

    /// Encode the whole batch into a single Arrow `RecordBatch`.
    ///
    /// Encoding is all-or-nothing: if any event fails to encode against the
    /// table's Arrow schema — most commonly an event missing (or null on) a
    /// column the Unity Catalog table declares `NOT NULL` — the entire batch
    /// fails with a non-retryable `EncodingError` and is dropped. UC columns are
    /// nullable by default, so this only affects tables with explicit `NOT NULL`
    /// columns. The underlying codec emits `EncoderNullConstraintError` naming
    /// the offending field(s).
    pub(super) fn encode_batch(
        schema: &ResolvedSchema,
        events: &[Event],
    ) -> Result<arrow::record_batch::RecordBatch, ZerobusSinkError> {
        let BatchOutput::Arrow(batch) =
            schema
                .encoder
                .encode_batch(events)
                .map_err(|e| ZerobusSinkError::EncodingError {
                    message: format!("Failed to encode batch: {}", e),
                })?;
        Ok(batch)
    }

    /// Ensure we have an active stream, creating one if necessary.
    ///
    /// Also used as the healthcheck: resolving the schema verifies the table
    /// and credentials against Unity Catalog, and creating the stream verifies
    /// connectivity to the Zerobus endpoint.
    pub async fn ensure_stream(&self) -> Result<(), ZerobusSinkError> {
        let schema = self.ensure_schema().await?;
        self.get_or_create_stream(schema).await.map(|_| ())
    }

    /// Return an `Arc` handle to the active stream, creating one if needed.
    ///
    /// The lock is held only while checking/creating the stream; callers can
    /// then use the returned `Arc` without holding the lock.
    async fn get_or_create_stream(
        &self,
        schema: &ResolvedSchema,
    ) -> Result<Arc<ActiveStream>, ZerobusSinkError> {
        let mut stream_guard = self.stream.lock().await;

        if stream_guard.is_none() {
            let (client_id, client_secret) = self.config.auth.credentials();
            let (client_id, client_secret) = (client_id.to_string(), client_secret.to_string());

            // We override only the two timeouts that `stream_options` exposes and
            // otherwise accept the SDK's Arrow-stream defaults — notably
            // `recovery = true`, so the SDK transparently reconnects and replays
            // in-flight batches on transient stream errors. That layers under
            // Vector's own retry: the SDK absorbs brief blips, and only surfaces a
            // retryable error (triggering a fresh stream via Tower retry) once its
            // own recovery budget is exhausted. Both layers are at-least-once, so
            // a reconnect may re-send unacknowledged batches.
            let stream_options = &self.config.stream_options;
            let builder = self
                .sdk
                .stream_builder()
                .table(self.config.table_name.clone())
                .oauth(client_id, client_secret)
                .arrow(Arc::clone(&schema.arrow_schema))
                .server_lack_of_ack_timeout_ms(stream_options.server_lack_of_ack_timeout_ms)
                .flush_timeout_ms(stream_options.flush_timeout_ms)
                .ipc_compression(stream_options.compression.into());
            let stream = builder
                .build_arrow()
                .await
                .map_err(|e| ZerobusSinkError::StreamInitError { source: e })?;

            *stream_guard = Some(Arc::new(ActiveStream::arrow(stream)));
        }

        Ok(Arc::clone(stream_guard.as_ref().unwrap()))
    }

    /// Gracefully close and remove the active stream.
    ///
    /// `ActiveStream::close()` takes `&self`, so this works regardless of how
    /// many `Arc` clones are still in flight: the inner write lock waits for
    /// any concurrent ingests to release their read guards before the SDK
    /// flush + close runs. The slot lock is released before close starts so
    /// concurrent `get_or_create_stream` calls aren't blocked on the SDK
    /// shutdown path.
    pub async fn close_stream(&self) {
        let stream = self.stream.lock().await.take();
        if let Some(stream) = stream {
            stream.close().await;
        }
    }

    /// Send encoded records to an already-resolved stream.
    ///
    /// On retryable errors the active stream is removed from the slot so that
    /// the next attempt (driven by Tower retry) creates a fresh one.
    async fn ingest(
        &self,
        stream: Arc<ActiveStream>,
        batch: arrow::record_batch::RecordBatch,
        events_byte_size: GroupedCountByteSize,
    ) -> Result<ZerobusResponse, ZerobusSinkError> {
        // Slot lock is not held here — concurrent ingests acquire read guards
        // on the inner `RwLock` and run truly in parallel.
        let result = match stream.as_ref() {
            ActiveStream::Arrow(lock) => {
                let guard = lock.read().await;
                let Some(s) = guard.as_ref() else {
                    return Err(ZerobusSinkError::StreamClosed);
                };
                match s.ingest_batch(batch).await {
                    Ok(offset) => s.wait_for_offset(offset).await.map(|_| ()),
                    Err(e) => Err(e),
                }
            }
            #[cfg(test)]
            ActiveStream::Mock(mock) => mock.try_ingest().await,
        };

        match result {
            Ok(()) => Ok(ZerobusResponse::delivered(events_byte_size)),
            Err(e) => {
                if e.is_retryable() {
                    // Clear the slot so the next attempt creates a fresh stream,
                    // but only if it still points to the same stream that failed —
                    // a concurrent task may have already replaced it.
                    {
                        let mut guard = self.stream.lock().await;
                        if guard.as_ref().is_some_and(|s| Arc::ptr_eq(s, &stream)) {
                            guard.take();
                        }
                    }
                    // `close()` takes `&self`, so we can always run the graceful
                    // path here regardless of how many other `Arc` clones are in
                    // flight. The write lock will wait for any concurrent ingests
                    // holding read guards to drain before flushing.
                    stream.close().await;
                }
                Err(ZerobusSinkError::IngestionError { source: e })
            }
        }
    }
}

impl Service<ZerobusRequest> for ZerobusService {
    type Response = ZerobusResponse;
    type Error = ZerobusSinkError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: ZerobusRequest) -> Self::Future {
        let service = self.clone();
        let events_byte_size =
            std::mem::take(request.metadata_mut()).into_events_estimated_json_encoded_byte_size();

        Box::pin(async move {
            let schema = service.ensure_schema().await?;
            let batch = Self::encode_batch(schema, &request.events)?;
            let stream = service.get_or_create_stream(schema).await?;
            service.ingest(stream, batch, events_byte_size).await
        })
    }
}

impl Clone for ZerobusService {
    fn clone(&self) -> Self {
        Self {
            sdk: Arc::clone(&self.sdk),
            config: Arc::clone(&self.config),
            http_client: self.http_client.clone(),
            stream: Arc::clone(&self.stream),
            schema: Arc::clone(&self.schema),
        }
    }
}

/// Retry logic for the Zerobus service.
///
/// For SDK errors (`ZerobusError`), delegates to the SDK's `is_retryable()` which
/// correctly marks transient errors (stream closed, channel issues) as retriable
/// and permanent errors (invalid table name, invalid argument, invalid endpoint)
/// as non-retriable.
#[derive(Debug, Default, Clone)]
pub struct ZerobusRetryLogic;

#[cfg(test)]
impl ZerobusService {
    /// Create a service with a mock stream already installed for testing.
    pub async fn new_with_mock(
        config: ZerobusSinkConfig,
        mock: MockStream,
    ) -> Result<Self, ZerobusSinkError> {
        config.validate()?;

        let sdk = ZerobusSdk::builder()
            .endpoint(&config.ingestion_endpoint)
            .unity_catalog_url(&config.unity_catalog_endpoint)
            .build()
            .map_err(|e| ZerobusSinkError::ConfigError {
                message: format!("Failed to create Zerobus SDK: {}", e),
            })?;

        let http_client = HttpClient::new(TlsSettings::default(), &ProxyConfig::default())
            .map_err(|e| ZerobusSinkError::ConfigError {
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        Ok(Self {
            sdk: Arc::new(sdk),
            config: Arc::new(config),
            http_client,
            stream: Arc::new(Mutex::new(Some(Arc::new(ActiveStream::Mock(mock))))),
            schema: Arc::new(OnceCell::new()),
        })
    }

    /// Returns true if the service currently has an active stream.
    pub async fn has_active_stream(&self) -> bool {
        self.stream.lock().await.is_some()
    }
}

impl RetryLogic for ZerobusRetryLogic {
    type Error = ZerobusSinkError;
    type Request = ZerobusRequest;
    type Response = ZerobusResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_retryable()
    }
}

/// Tower layer that converts retry-budget-exhausted retryable errors into a
/// successful `ZerobusResponse` carrying `EventStatus::Errored`.
///
/// Wraps the retry layer from the outside. When the retry layer returns:
/// - `Ok(resp)` — pass through unchanged.
/// - `Err(e)` where `e.is_retryable()` — convert to `Ok(ZerobusResponse::errored())`
///   so the driver marks finalizers `Errored` (transient — source / disk
///   buffer may replay) rather than `Rejected` (permanent drop).
/// - `Err(e)` permanent — propagate so the driver maps to `Rejected`.
///
/// Without this layer the driver maps every `Err` from `Service::call` to
/// `EventStatus::Rejected`, which would drop transient-but-exhausted failures
/// as if they were permanent.
#[derive(Clone, Debug, Default)]
pub struct RetryableErrorAsErroredLayer;

impl<S> Layer<S> for RetryableErrorAsErroredLayer {
    type Service = RetryableErrorAsErrored<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RetryableErrorAsErrored { inner }
    }
}

#[derive(Clone, Debug)]
pub struct RetryableErrorAsErrored<S> {
    inner: S,
}

impl<S> Service<ZerobusRequest> for RetryableErrorAsErrored<S>
where
    S: Service<ZerobusRequest, Response = ZerobusResponse, Error = crate::Error>,
    S::Future: Send + 'static,
{
    type Response = ZerobusResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: ZerobusRequest) -> Self::Future {
        let fut = self.inner.call(req);
        Box::pin(async move {
            match fut.await {
                Ok(resp) => Ok(resp),
                Err(e) => {
                    // The Tower stack boxes errors above us (retry, timeout,
                    // adaptive-concurrency). Downcast to inspect retryability;
                    // anything that isn't a `ZerobusSinkError` (e.g. a timeout
                    // `Elapsed`) is conservatively treated as transient.
                    let retryable = match e.downcast_ref::<ZerobusSinkError>() {
                        Some(zb) => zb.is_retryable(),
                        None => true,
                    };
                    if retryable {
                        warn!(
                            message = "Zerobus retry budget exhausted on transient error; signaling Errored so source or buffer may replay.",
                            error = %e,
                        );
                        Ok(ZerobusResponse::errored())
                    } else {
                        Err(e)
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::databricks_zerobus::config::{
        DatabricksAuthentication, ZerobusStreamOptions,
    };
    use databricks_zerobus_ingest_sdk::ZerobusError;
    use vector_lib::sensitive_string::SensitiveString;

    fn test_config() -> ZerobusSinkConfig {
        ZerobusSinkConfig {
            ingestion_endpoint: "https://127.0.0.1:1".to_string(),
            table_name: "test.default.logs".to_string(),
            unity_catalog_endpoint: "https://127.0.0.1:1".to_string(),
            auth: DatabricksAuthentication::OAuth {
                client_id: SensitiveString::from("id".to_string()),
                client_secret: SensitiveString::from("secret".to_string()),
            },
            user_agent: None,
            stream_options: ZerobusStreamOptions::default(),
            batch: Default::default(),
            request: Default::default(),
            acknowledgements: Default::default(),
        }
    }

    fn dummy_batch() -> arrow::record_batch::RecordBatch {
        // The mock stream ignores the batch contents, so an empty batch with an
        // empty schema is sufficient for the ingest-path tests.
        arrow::record_batch::RecordBatch::new_empty(Arc::new(arrow::datatypes::Schema::empty()))
    }

    async fn current_stream(service: &ZerobusService) -> Arc<ActiveStream> {
        Arc::clone(service.stream.lock().await.as_ref().unwrap())
    }

    #[tokio::test]
    async fn ingest_succeeds_with_mock_stream() {
        let service = ZerobusService::new_with_mock(test_config(), MockStream::succeeding())
            .await
            .unwrap();

        let stream = current_stream(&service).await;
        let result = service
            .ingest(stream, dummy_batch(), GroupedCountByteSize::new_untagged())
            .await;

        assert!(result.is_ok());
        assert!(service.has_active_stream().await);
    }

    #[tokio::test]
    async fn retryable_error_clears_stream() {
        let mock = MockStream::failing(ZerobusError::ChannelCreationError(
            "connection reset".to_string(),
        ));
        let service = ZerobusService::new_with_mock(test_config(), mock)
            .await
            .unwrap();

        assert!(service.has_active_stream().await);

        let stream = current_stream(&service).await;
        let err = service
            .ingest(stream, dummy_batch(), GroupedCountByteSize::new_untagged())
            .await
            .unwrap_err();

        assert!(matches!(err, ZerobusSinkError::IngestionError { .. }));
        assert!(ZerobusRetryLogic.is_retriable_error(&err));
        // Stream must have been cleared for the next retry.
        assert!(!service.has_active_stream().await);
    }

    #[tokio::test]
    async fn non_retryable_error_keeps_stream() {
        let mock = MockStream::failing(ZerobusError::InvalidArgument("bad field".to_string()));
        let service = ZerobusService::new_with_mock(test_config(), mock)
            .await
            .unwrap();

        assert!(service.has_active_stream().await);

        let stream = current_stream(&service).await;
        let err = service
            .ingest(stream, dummy_batch(), GroupedCountByteSize::new_untagged())
            .await
            .unwrap_err();

        assert!(matches!(err, ZerobusSinkError::IngestionError { .. }));
        assert!(!ZerobusRetryLogic.is_retriable_error(&err));
        // Stream should NOT be cleared for non-retryable errors.
        assert!(service.has_active_stream().await);
    }

    #[tokio::test]
    async fn stream_recovers_after_retryable_failure() {
        // Simulate: success → retryable failure → success again.
        let mock = MockStream::succeeding();
        let service = ZerobusService::new_with_mock(test_config(), mock)
            .await
            .unwrap();

        // First ingest succeeds.
        let stream = current_stream(&service).await;
        assert!(
            service
                .ingest(stream, dummy_batch(), GroupedCountByteSize::new_untagged())
                .await
                .is_ok()
        );
        assert!(service.has_active_stream().await);

        // Inject a retryable error for the next call.
        {
            let guard = service.stream.lock().await;
            if let Some(arc) = guard.as_ref()
                && let ActiveStream::Mock(mock) = arc.as_ref()
            {
                mock.set_next_error(ZerobusError::ChannelCreationError("reset".to_string()));
            }
        }

        // Second ingest fails and clears the stream.
        let stream = current_stream(&service).await;
        let err = service
            .ingest(stream, dummy_batch(), GroupedCountByteSize::new_untagged())
            .await
            .unwrap_err();
        assert!(ZerobusRetryLogic.is_retriable_error(&err));
        assert!(!service.has_active_stream().await);

        // Simulate Tower retry: re-inject a fresh mock stream
        // (in production, ensure_stream() would create a new real stream).
        *service.stream.lock().await = Some(Arc::new(ActiveStream::Mock(MockStream::succeeding())));

        // Third ingest succeeds on the new stream.
        let stream = current_stream(&service).await;
        assert!(
            service
                .ingest(stream, dummy_batch(), GroupedCountByteSize::new_untagged())
                .await
                .is_ok()
        );
        assert!(service.has_active_stream().await);
    }

    #[tokio::test]
    async fn close_stream_calls_close_on_active_stream() {
        let mock = MockStream::succeeding();
        let closed = mock.closed_flag();

        let service = ZerobusService::new_with_mock(test_config(), mock)
            .await
            .unwrap();

        assert!(service.has_active_stream().await);
        assert!(!closed.load(std::sync::atomic::Ordering::Relaxed));

        service.close_stream().await;

        assert!(!service.has_active_stream().await);
        assert!(closed.load(std::sync::atomic::Ordering::Relaxed));
    }

    /// Regression test for the "silent abort-only Drop" issue: when two
    /// ingests are in flight (each holding an `Arc<ActiveStream>`) and one
    /// fails retryably, the failing task must still run the graceful close
    /// path. Under the previous design `Arc::get_mut` returned `None` here
    /// because the second task held a clone, so close was skipped and the
    /// stream fell to abort-only Drop.
    #[tokio::test]
    async fn retryable_failure_with_concurrent_ingest_still_closes() {
        let (mock, gate) = MockStream::failing(ZerobusError::ChannelCreationError(
            "connection reset".to_string(),
        ))
        .with_gate();
        let closed = mock.closed_flag();

        let service = ZerobusService::new_with_mock(test_config(), mock)
            .await
            .unwrap();

        // Spawn two concurrent ingests. Each clones the same stream `Arc`,
        // then blocks in the gate.
        let s1 = service.clone();
        let t1 = tokio::spawn(async move {
            let stream = current_stream(&s1).await;
            s1.ingest(stream, dummy_batch(), GroupedCountByteSize::new_untagged())
                .await
        });
        let s2 = service.clone();
        let t2 = tokio::spawn(async move {
            let stream = current_stream(&s2).await;
            s2.ingest(stream, dummy_batch(), GroupedCountByteSize::new_untagged())
                .await
        });

        // Wait until both ingests are inside the gate (both `Arc`s alive).
        gate.wait_for_started(2).await;

        // Release both. The failing one will go through the retry-cleanup
        // path while the other still holds an `Arc`. Under the old design
        // `Arc::get_mut` would return `None` and close would be skipped.
        gate.release(2);

        let r1 = t1.await.unwrap();
        let r2 = t2.await.unwrap();

        // At least one task observed the retryable error (the mock only
        // produces a single error, but ordering between tasks is undefined).
        assert!(r1.is_err() || r2.is_err());

        // The graceful close path must have run despite concurrent `Arc`s.
        assert!(
            closed.load(std::sync::atomic::Ordering::Relaxed),
            "graceful close did not run; stream would have leaked under old design"
        );
        // And the slot was cleared so the next ingest creates a fresh stream.
        assert!(!service.has_active_stream().await);
    }

    fn dummy_request() -> ZerobusRequest {
        ZerobusRequest {
            events: Arc::new(vec![]),
            metadata: RequestMetadata::default(),
            finalizers: EventFinalizers::default(),
        }
    }

    #[tokio::test]
    async fn retryable_err_after_exhaustion_becomes_ok_errored() {
        use tower::ServiceExt;
        let inner = tower::service_fn(|_req: ZerobusRequest| async move {
            let err: crate::Error = Box::new(ZerobusSinkError::SchemaError {
                message: "UC 503".to_string(),
                retryable: true,
            });
            Err::<ZerobusResponse, _>(err)
        });
        let mut svc = RetryableErrorAsErrored { inner };
        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(dummy_request())
            .await
            .unwrap();
        assert_eq!(resp.status, vector_lib::event::EventStatus::Errored);
    }

    #[tokio::test]
    async fn non_retryable_err_propagates() {
        use tower::ServiceExt;
        let inner = tower::service_fn(|_req: ZerobusRequest| async move {
            let err: crate::Error = Box::new(ZerobusSinkError::EncodingError {
                message: "bad".to_string(),
            });
            Err::<ZerobusResponse, _>(err)
        });
        let mut svc = RetryableErrorAsErrored { inner };
        let err = svc
            .ready()
            .await
            .unwrap()
            .call(dummy_request())
            .await
            .unwrap_err();
        let zb = err.downcast_ref::<ZerobusSinkError>().unwrap();
        assert!(matches!(zb, ZerobusSinkError::EncodingError { .. }));
    }

    #[tokio::test]
    async fn unknown_err_treated_as_transient() {
        use tower::ServiceExt;
        // Simulate a Tower-layer error that isn't a ZerobusSinkError (e.g.
        // timeout `Elapsed`): conservatively becomes Errored, not Rejected.
        #[derive(Debug)]
        struct Other;
        impl std::fmt::Display for Other {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "other")
            }
        }
        impl std::error::Error for Other {}

        let inner = tower::service_fn(|_req: ZerobusRequest| async move {
            let err: crate::Error = Box::new(Other);
            Err::<ZerobusResponse, _>(err)
        });
        let mut svc = RetryableErrorAsErrored { inner };
        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(dummy_request())
            .await
            .unwrap();
        assert_eq!(resp.status, vector_lib::event::EventStatus::Errored);
    }

    #[tokio::test]
    async fn ok_response_passes_through() {
        use tower::ServiceExt;
        let inner = tower::service_fn(|_req: ZerobusRequest| async move {
            Ok::<_, crate::Error>(ZerobusResponse::delivered(
                GroupedCountByteSize::new_untagged(),
            ))
        });
        let mut svc = RetryableErrorAsErrored { inner };
        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(dummy_request())
            .await
            .unwrap();
        assert_eq!(resp.status, vector_lib::event::EventStatus::Delivered);
    }

    /// Encode real log events against a schema covering the common UC→Arrow
    /// types and assert the resulting `RecordBatch` columns, types, and a null
    /// in a nullable column. Exercises the production `encode_batch` path
    /// (`ArrowStreamSerializer` → `RecordBatch`), which the mock stream tests
    /// bypass.
    #[test]
    fn encode_batch_maps_events_to_record_batch() {
        use crate::event::LogEvent;
        use arrow::array::{Array, AsArray};
        use arrow::datatypes::{
            DataType, Field, Int64Type, Schema, TimeUnit, TimestampMicrosecondType,
        };
        use chrono::Utc;

        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("body", DataType::LargeUtf8, true),
            Field::new(
                "ts",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                true,
            ),
        ]);
        let resolved = ResolvedSchema::for_test(schema);

        let mut e1 = LogEvent::default();
        e1.insert(vrl::event_path!("id"), 1i64);
        e1.insert(vrl::event_path!("body"), "hello");
        e1.insert(vrl::event_path!("ts"), Utc::now());

        let mut e2 = LogEvent::default();
        e2.insert(vrl::event_path!("id"), 2i64);
        // `body` and `ts` omitted — both nullable, so they encode as null.

        let batch =
            ZerobusService::encode_batch(&resolved, &[Event::Log(e1), Event::Log(e2)]).unwrap();

        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 3);

        let ids = batch.column(0).as_primitive::<Int64Type>();
        assert_eq!(ids.value(0), 1);
        assert_eq!(ids.value(1), 2);

        // LargeUtf8 -> LargeStringArray (i64 offsets).
        let body = batch.column(1).as_string::<i64>();
        assert_eq!(body.value(0), "hello");
        assert!(body.is_null(1));

        let ts = batch.column(2).as_primitive::<TimestampMicrosecondType>();
        assert!(!ts.is_null(0));
        assert!(ts.is_null(1));
    }

    /// An event missing a column the table declares `NOT NULL` fails the whole
    /// batch with a non-retryable `EncodingError` (the batch is dropped, not
    /// replayed). Locks in the documented strict-null behavior.
    #[test]
    fn encode_batch_rejects_event_missing_non_nullable_field() {
        use crate::event::LogEvent;
        use arrow::datatypes::{DataType, Field, Schema};

        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false), // NOT NULL
            Field::new("body", DataType::LargeUtf8, true),
        ]);
        let resolved = ResolvedSchema::for_test(schema);

        let mut e = LogEvent::default();
        e.insert(vrl::event_path!("body"), "no id here"); // `id` omitted

        let err = ZerobusService::encode_batch(&resolved, &[Event::Log(e)]).unwrap_err();
        assert!(matches!(err, ZerobusSinkError::EncodingError { .. }));
        assert!(!err.is_retryable());
    }
}

//! Zerobus service wrapper for Vector sink integration.

use crate::config::ProxyConfig;
use crate::sinks::util::retries::RetryLogic;
use databricks_zerobus_ingest_sdk::{ConnectorFactory, ProxyConnector, ZerobusSdk, ZerobusStream};
use futures::future::BoxFuture;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tower::Service;
use tracing::warn;
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

/// The payload for a Zerobus request.
///
/// The zerobus sink only supports proto-encoded records.
#[derive(Clone, Debug)]
pub enum ZerobusPayload {
    /// Pre-encoded protobuf records (one byte buffer per event).
    Records(Vec<Vec<u8>>),
}

/// Request type for the Zerobus service.
#[derive(Clone, Debug)]
pub struct ZerobusRequest {
    pub payload: ZerobusPayload,
    pub metadata: RequestMetadata,
    pub finalizers: EventFinalizers,
}

/// Response type for the Zerobus service.
#[derive(Debug)]
pub struct ZerobusResponse {
    pub events_byte_size: GroupedCountByteSize,
}

impl DriverResponse for ZerobusResponse {
    fn event_status(&self) -> vector_lib::event::EventStatus {
        vector_lib::event::EventStatus::Delivered
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

/// Determines what kind of stream the service creates and how payloads are ingested.
///
/// The sink only supports proto streams today; this is kept as an enum to
/// leave room for future stream modes without reshaping all the call sites.
#[derive(Clone)]
pub enum StreamMode {
    /// Proto stream using `ZerobusStream::ingest_records_offset`.
    Proto {
        descriptor_proto: Arc<prost_reflect::prost_types::DescriptorProto>,
    },
}

/// The active stream.
///
/// The SDK's `ZerobusStream::close()` requires `&mut self`, but ingests need
/// shared access to call `&self` methods concurrently. We resolve this with an
/// `RwLock`: ingests hold a read guard across `ingest_records_offset`, and
/// `close()` takes the write guard, pulls the stream out of the `Option`, and
/// awaits its SDK-level close on the owned value. Any holder of an `Arc` can
/// invoke `close()`, so the graceful path always runs — there is no
/// `try_unwrap`/`get_mut` race.
enum ActiveStream {
    Proto(RwLock<Option<Box<ZerobusStream>>>),
    /// Test-only variant that returns a pre-configured error on ingest.
    #[cfg(test)]
    Mock(MockStream),
}

impl ActiveStream {
    fn proto(stream: ZerobusStream) -> Self {
        ActiveStream::Proto(RwLock::new(Some(Box::new(stream))))
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
            ActiveStream::Proto(lock) => {
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

/// Service for handling Zerobus requests.
pub struct ZerobusService {
    sdk: Arc<ZerobusSdk>,
    config: Arc<ZerobusSinkConfig>,
    stream: Arc<Mutex<Option<Arc<ActiveStream>>>>,
    stream_mode: StreamMode,
}

impl ZerobusService {
    pub async fn new(
        config: ZerobusSinkConfig,
        stream_mode: StreamMode,
        proxy: &ProxyConfig,
    ) -> Result<Self, ZerobusSinkError> {
        let mut builder = ZerobusSdk::builder()
            .endpoint(&config.ingestion_endpoint)
            .unity_catalog_url(&config.unity_catalog_endpoint);
        builder = builder.connector_factory(build_connector_factory(proxy)?);
        let sdk = builder.build().map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to create Zerobus SDK: {}", e),
        })?;

        Ok(Self {
            sdk: Arc::new(sdk),
            config: Arc::new(config),
            stream: Arc::new(Mutex::new(None)),
            stream_mode,
        })
    }

    /// Resolve the protobuf message descriptor from the schema configuration.
    pub async fn resolve_descriptor(
        config: &ZerobusSinkConfig,
        proxy: &crate::config::ProxyConfig,
    ) -> Result<prost_reflect::MessageDescriptor, ZerobusSinkError> {
        let (client_id, client_secret) = config.auth.credentials();

        let table_schema = unity_catalog_schema::fetch_table_schema(
            &config.unity_catalog_endpoint,
            &config.table_name,
            client_id,
            client_secret,
            proxy,
        )
        .await?;

        unity_catalog_schema::generate_descriptor_from_schema(&table_schema)
    }

    /// Ensure we have an active stream, creating one if necessary.
    ///
    /// Also used as the healthcheck: eagerly creating a stream verifies
    /// OAuth credentials, endpoint connectivity, and table validity.
    pub async fn ensure_stream(&self) -> Result<(), ZerobusSinkError> {
        self.get_or_create_stream().await.map(|_| ())
    }

    /// Return an `Arc` handle to the active stream, creating one if needed.
    ///
    /// The lock is held only while checking/creating the stream; callers can
    /// then use the returned `Arc` without holding the lock.
    async fn get_or_create_stream(&self) -> Result<Arc<ActiveStream>, ZerobusSinkError> {
        let mut stream_guard = self.stream.lock().await;

        if stream_guard.is_none() {
            let (client_id, client_secret) = self.config.auth.credentials();
            let (client_id, client_secret) = (client_id.to_string(), client_secret.to_string());

            let active_stream = match &self.stream_mode {
                StreamMode::Proto { descriptor_proto } => {
                    let stream_options = &self.config.stream_options;
                    let stream = self
                        .sdk
                        .stream_builder()
                        .table(self.config.table_name.clone())
                        .oauth(client_id, client_secret)
                        .compiled_proto((**descriptor_proto).clone())
                        .server_lack_of_ack_timeout_ms(stream_options.server_lack_of_ack_timeout_ms)
                        .flush_timeout_ms(stream_options.flush_timeout_ms)
                        .build()
                        .await
                        .map_err(|e| ZerobusSinkError::StreamInitError { source: e })?;
                    ActiveStream::proto(stream)
                }
            };

            *stream_guard = Some(Arc::new(active_stream));
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

    /// Ingest a payload (proto records or Arrow batch).
    ///
    /// Obtains an `Arc` handle to the stream (creating one if needed) and
    /// then releases the lock before calling into the SDK so that concurrent
    /// ingests are not serialized.
    ///
    /// On retryable errors the active stream is removed from the slot so that
    /// the next attempt (driven by Tower retry) creates a fresh one.
    pub async fn ingest(
        &self,
        payload: ZerobusPayload,
        events_byte_size: GroupedCountByteSize,
    ) -> Result<ZerobusResponse, ZerobusSinkError> {
        let stream = self.get_or_create_stream().await?;

        // Slot lock is not held here — concurrent ingests acquire read guards
        // on the inner `RwLock` and run truly in parallel.
        let result = match (payload, stream.as_ref()) {
            (ZerobusPayload::Records(records), ActiveStream::Proto(lock)) => {
                let guard = lock.read().await;
                let Some(s) = guard.as_ref() else {
                    return Err(ZerobusSinkError::StreamClosed);
                };
                match s.ingest_records_offset(records).await {
                    Ok(Some(offset)) => s.wait_for_offset(offset).await.map(|_| ()),
                    Ok(None) => {
                        return Err(ZerobusSinkError::MissingAckOffset);
                    }
                    Err(e) => Err(e),
                }
            }
            #[cfg(test)]
            (ZerobusPayload::Records(_), ActiveStream::Mock(mock)) => mock.try_ingest().await,
        };

        match result {
            Ok(()) => Ok(ZerobusResponse { events_byte_size }),
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

        Box::pin(async move { service.ingest(request.payload, events_byte_size).await })
    }
}

impl Clone for ZerobusService {
    fn clone(&self) -> Self {
        Self {
            sdk: Arc::clone(&self.sdk),
            config: Arc::clone(&self.config),
            stream: Arc::clone(&self.stream),
            stream_mode: self.stream_mode.clone(),
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

        Ok(Self {
            sdk: Arc::new(sdk),
            config: Arc::new(config),
            stream: Arc::new(Mutex::new(Some(Arc::new(ActiveStream::Mock(mock))))),
            stream_mode: StreamMode::Proto {
                descriptor_proto: Arc::new(Default::default()),
            },
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
        match error {
            ZerobusSinkError::ZerobusError { source }
            | ZerobusSinkError::StreamInitError { source }
            | ZerobusSinkError::IngestionError { source } => source.is_retryable(),
            ZerobusSinkError::StreamClosed => true,
            ZerobusSinkError::ConfigError { .. }
            | ZerobusSinkError::EncodingError { .. }
            | ZerobusSinkError::MissingAckOffset => false,
        }
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
            stream_options: ZerobusStreamOptions::default(),
            batch: Default::default(),
            request: Default::default(),
            acknowledgements: Default::default(),
        }
    }

    fn dummy_payload() -> ZerobusPayload {
        ZerobusPayload::Records(vec![vec![1, 2, 3]])
    }

    #[tokio::test]
    async fn ingest_succeeds_with_mock_stream() {
        let service = ZerobusService::new_with_mock(test_config(), MockStream::succeeding())
            .await
            .unwrap();

        let result = service
            .ingest(dummy_payload(), GroupedCountByteSize::new_untagged())
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

        let err = service
            .ingest(dummy_payload(), GroupedCountByteSize::new_untagged())
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

        let err = service
            .ingest(dummy_payload(), GroupedCountByteSize::new_untagged())
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
        assert!(
            service
                .ingest(dummy_payload(), GroupedCountByteSize::new_untagged())
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
        let err = service
            .ingest(dummy_payload(), GroupedCountByteSize::new_untagged())
            .await
            .unwrap_err();
        assert!(ZerobusRetryLogic.is_retriable_error(&err));
        assert!(!service.has_active_stream().await);

        // Simulate Tower retry: re-inject a fresh mock stream
        // (in production, ensure_stream() would create a new real stream).
        *service.stream.lock().await = Some(Arc::new(ActiveStream::Mock(MockStream::succeeding())));

        // Third ingest succeeds on the new stream.
        assert!(
            service
                .ingest(dummy_payload(), GroupedCountByteSize::new_untagged())
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

        // Spawn two concurrent ingests. Each will obtain its own `Arc` clone
        // from `get_or_create_stream`, then block in the gate.
        let s1 = service.clone();
        let t1 = tokio::spawn(async move {
            s1.ingest(dummy_payload(), GroupedCountByteSize::new_untagged())
                .await
        });
        let s2 = service.clone();
        let t2 = tokio::spawn(async move {
            s2.ingest(dummy_payload(), GroupedCountByteSize::new_untagged())
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
}

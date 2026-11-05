use futures::future::BoxFuture;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use snafu::Snafu;
use std::task::{Context, Poll};
use tonic::metadata::{Ascii, MetadataValue};
use tonic::{Request, Status};
use tower::Service;
use vector_http::{GrpcTransport, HttpError};
use vector_lib::{
    event::EventStatus,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use crate::sinks::util::retries::{RetryAction, RetryLogic};

use super::proto::google::cloud::bigquery::storage::v1 as proto;
use crate::event::{EventFinalizers, Finalizable};
use crate::gcp::GcpAuthenticator;
use crate::internal_events::GcpBigqueryRetriesExhaustedError;

/// Characters left unescaped in the `x-goog-request-params` header.
const REQUEST_PARAMS_ESCAPE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'/')
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

#[derive(Clone)]
pub(super) struct RequestAuth {
    auth: GcpAuthenticator,
    request_params: MetadataValue<Ascii>,
}

impl RequestAuth {
    /// Metadata for `AppendRows`, which routes on the `write_stream` field.
    pub(super) fn for_append_rows(
        auth: GcpAuthenticator,
        write_stream: &str,
    ) -> crate::Result<Self> {
        Self::new(auth, "write_stream", write_stream)
    }

    /// Metadata for `GetWriteStream`, which routes on the `name` field.
    pub(super) fn for_get_write_stream(
        auth: GcpAuthenticator,
        write_stream: &str,
    ) -> crate::Result<Self> {
        Self::new(auth, "name", write_stream)
    }

    /// `param` is the request field BigQuery routes on which varies depending on the API.
    /// gRPC callers must send it so that requests to datasets outside the US multi-region
    /// reach the correct region.
    fn new(auth: GcpAuthenticator, param: &str, write_stream: &str) -> crate::Result<Self> {
        let request_params = format!(
            "{param}={}",
            utf8_percent_encode(write_stream, REQUEST_PARAMS_ESCAPE)
        )
        .parse()?;
        Ok(Self {
            auth,
            request_params,
        })
    }

    /// Wraps `message` in a request carrying the current token and the routing metadata.
    pub(super) fn request<T>(&self, message: T) -> Result<Request<T>, Status> {
        let mut request = Request::new(message);
        if let Some(token) = self.auth.make_token() {
            let value: MetadataValue<_> = token
                .parse()
                .map_err(|e| tonic::Status::unauthenticated(format!("{e:?}")))?;
            request.metadata_mut().insert("authorization", value);
        }
        request
            .metadata_mut()
            .insert("x-goog-request-params", self.request_params.clone());
        Ok(request)
    }
}

#[derive(Clone)]
pub(super) struct BigqueryRequest {
    pub request: proto::AppendRowsRequest,
    pub metadata: RequestMetadata,
    pub finalizers: EventFinalizers,
    pub uncompressed_size: usize,
}

impl Finalizable for BigqueryRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for BigqueryRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

#[derive(Debug)]
pub(super) struct BigqueryResponse {
    /// `None` when the request failed with a transient error.
    body: Option<proto::AppendRowsResponse>,
    request_byte_size: GroupedCountByteSize,
    request_uncompressed_size: usize,
}

impl BigqueryResponse {
    /// A response for a request whose transient failure exhausted its retries.
    fn retries_exhausted() -> Self {
        Self {
            body: None,
            request_byte_size: vector_lib::config::telemetry().create_request_count_byte_size(),
            request_uncompressed_size: 0,
        }
    }

    /// Short description of why BigQuery didn't accept the rows, for logging.
    fn error_summary(&self) -> String {
        let Some(body) = &self.body else {
            return "retries exhausted".to_owned();
        };
        if !body.row_errors.is_empty() {
            let shown: Vec<String> = body
                .row_errors
                .iter()
                .take(5)
                .map(|error| format!("row {}: {}", error.index, error.message))
                .collect();
            return format!(
                "{} row error(s): {}",
                body.row_errors.len(),
                shown.join("; ")
            );
        }
        match &body.response {
            Some(proto::append_rows_response::Response::Error(status)) => format!(
                "{:?} (code {}): {}",
                tonic::Code::from_i32(status.code),
                status.code,
                status.message
            ),
            Some(proto::append_rows_response::Response::AppendResult(_)) | None => {
                "no error details in response".to_owned()
            }
        }
    }
}

impl DriverResponse for BigqueryResponse {
    fn event_status(&self) -> EventStatus {
        let Some(body) = &self.body else {
            // Transient failure, let acknowledging sources redeliver the events.
            return EventStatus::Errored;
        };
        if !body.row_errors.is_empty() {
            // The AppendRowsResponse reports on specific rows that failed to append,
            // meaning that in theory on failures we can retry the request without the bad events.
            // Unfortunately there's no good mechanism for doing this in the Vector model;
            // it's assumed either the whole thing is successful or it is not.
            return EventStatus::Rejected;
        }
        match &body.response {
            Some(proto::append_rows_response::Response::AppendResult(_)) => EventStatus::Delivered,
            Some(proto::append_rows_response::Response::Error(status)) => {
                // `google.rpc.Code` values are the gRPC status codes.
                match tonic::Code::from_i32(status.code) {
                    // We really shouldn't be able to get here, but just in case
                    tonic::Code::Ok => EventStatus::Delivered,
                    // Permanent errors - retrying won't help
                    tonic::Code::InvalidArgument
                    | tonic::Code::NotFound
                    | tonic::Code::AlreadyExists
                    // PermissionDenied requires human action (IAM changes) to resolve.
                    | tonic::Code::PermissionDenied
                    | tonic::Code::FailedPrecondition
                    | tonic::Code::OutOfRange
                    | tonic::Code::Unimplemented
                    | tonic::Code::DataLoss => EventStatus::Rejected,
                    // Everything else (including Unauthenticated, since a bearer token may have
                    // just expired and a retry will pick up the refreshed token) is transient.
                    _ => EventStatus::Errored,
                }
            }
            // Unexpected response from BigQuery. We should mark the request as Errored.
            None => EventStatus::Errored,
        }
    }
    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.request_byte_size
    }
    fn bytes_sent(&self) -> Option<usize> {
        Some(self.request_uncompressed_size)
    }
}

#[derive(Debug, Snafu)]
pub(super) enum BigqueryServiceError {
    #[snafu(display("BigQuery request failure: {}", status))]
    Request { status: tonic::Status },
    #[snafu(display("BigQuery response stream closed"))]
    ResponseStreamClosed,
}

impl BigqueryServiceError {
    /// Whether this error should be retried.
    pub(super) fn is_retryable(&self) -> bool {
        match self {
            Self::ResponseStreamClosed => true,
            Self::Request { status } if is_transport_failure(status) => true,
            // Allow transient gRPC status codes to be retried.
            Self::Request { status } => matches!(
                status.code(),
                tonic::Code::Unavailable
                | tonic::Code::ResourceExhausted
                | tonic::Code::DeadlineExceeded
                // Internal is documented as a retriable server-side error in `AppendRowsResponse`.
                | tonic::Code::Internal
                // Unauthenticated is included because a bearer token may expire between the token
                // refresher updating it and the request being sent. A retry will pick up a new token.
                | tonic::Code::Unauthenticated
                // Aborted is returned when the server closes the stream (e.g. on restart) and advises
                // clients to reconnect.
                | tonic::Code::Aborted
                // Unknown also covers HTTP errors without a gRPC status.
                | tonic::Code::Unknown
                | tonic::Code::Cancelled
            ),
        }
    }
}

impl From<tonic::Status> for BigqueryServiceError {
    fn from(status: tonic::Status) -> Self {
        Self::Request { status }
    }
}

pub(super) type Transport = GrpcTransport<tonic::body::Body>;

pub(super) type BigQueryWriteClient = proto::big_query_write_client::BigQueryWriteClient<Transport>;

#[derive(Clone)]
pub(super) struct BigqueryService {
    service: BigQueryWriteClient,
    auth: RequestAuth,
}

/// Whether the status was caused by a retriable connection-level failure rather than an error
/// returned by BigQuery.
fn is_transport_failure(status: &tonic::Status) -> bool {
    let mut source = std::error::Error::source(status);
    while let Some(error) = source {
        if let Some(error) = error.downcast_ref::<HttpError>() {
            return error.is_retriable();
        }
        // Failures reading the response body, such as a connection reset mid-stream.
        if error.is::<std::io::Error>() || error.is::<hyper::Error>() {
            return true;
        }
        source = error.source();
    }
    false
}

#[derive(Clone, Debug)]
pub(super) struct BigqueryRetryLogic;

impl RetryLogic for BigqueryRetryLogic {
    type Error = BigqueryServiceError;
    type Request = BigqueryRequest;
    type Response = BigqueryResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_retryable()
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction<Self::Request> {
        match response.event_status() {
            // Dropped means the event was abandoned without finalization.
            // It is never returned by `BigqueryResponse::event_status()` but treat it as a
            // no-op rather than a retriable error.
            EventStatus::Delivered | EventStatus::Recorded | EventStatus::Dropped => {
                RetryAction::Successful
            }
            EventStatus::Errored => RetryAction::Retry(
                format!(
                    "transient error in BigQuery response: {}",
                    response.error_summary()
                )
                .into(),
            ),
            EventStatus::Rejected => RetryAction::DontRetry(
                format!(
                    "permanent error in BigQuery response: {}",
                    response.error_summary()
                )
                .into(),
            ),
        }
    }
}

impl BigqueryService {
    pub(super) fn new(
        transport: Transport,
        origin: http::Uri,
        auth: GcpAuthenticator,
        write_stream: &str,
    ) -> crate::Result<Self> {
        let service = BigQueryWriteClient::with_origin(transport, origin)
            // AppendRowsResponse.row_errors is a repeated field
            // that can grow with batch size, so remove the cap to avoid decoding failures on large
            // error responses.
            .max_decoding_message_size(usize::MAX);
        Ok(Self {
            service,
            auth: RequestAuth::for_append_rows(auth, write_stream)?,
        })
    }
}

impl Service<BigqueryRequest> for BigqueryService {
    type Response = BigqueryResponse;
    type Error = BigqueryServiceError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: BigqueryRequest) -> Self::Future {
        let metadata = std::mem::take(request.metadata_mut());
        let request_byte_size = metadata.into_events_estimated_json_encoded_byte_size();
        let request_uncompressed_size = request.uncompressed_size;

        let mut client = self.service.clone();
        let auth = self.auth.clone();

        Box::pin(async move {
            // Ideally, we would maintain the gRPC stream, detect when auth expired and re-request with new auth.
            // But issuing a new request every time leads to more comprehensible code with reasonable performance.
            let stream = auth.request(tokio_stream::once(request.request))?;
            let response = client.append_rows(stream).await?;
            match response.into_inner().message().await? {
                Some(body) => {
                    trace!(message = "Received response body from BigQuery.", ?body,);
                    Ok(BigqueryResponse {
                        body: Some(body),
                        request_byte_size,
                        request_uncompressed_size,
                    })
                }
                None => Err(BigqueryServiceError::ResponseStreamClosed),
            }
        })
    }
}

/// Tower layer, applied outside the retry layer, that reports transient errors which exhausted
/// their retries as an `Errored` response.
///
/// Without this layer the driver maps every `Err` from `Service::call` to
/// `EventStatus::Rejected`, which would drop transient-but-exhausted failures
/// as if they were permanent.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RetriesExhaustedAsErroredLayer;

impl<S> tower::Layer<S> for RetriesExhaustedAsErroredLayer {
    type Service = RetriesExhaustedAsErrored<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RetriesExhaustedAsErrored { inner }
    }
}

#[derive(Clone, Debug)]
pub(super) struct RetriesExhaustedAsErrored<S> {
    inner: S,
}

impl<S> Service<BigqueryRequest> for RetriesExhaustedAsErrored<S>
where
    S: Service<BigqueryRequest, Response = BigqueryResponse, Error = crate::Error>,
    S::Future: Send + 'static,
{
    type Response = BigqueryResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: BigqueryRequest) -> Self::Future {
        let count = request.get_metadata().event_count();
        let future = self.inner.call(request);
        Box::pin(async move {
            match future.await {
                Ok(response) => {
                    // The retry layer only returns an `Errored` response once a transient error in the
                    // response body has exhausted its retries.
                    if response.event_status() == EventStatus::Errored {
                        emit!(GcpBigqueryRetriesExhaustedError {
                            reason: response.error_summary(),
                            count,
                        });
                    }
                    Ok(response)
                }
                Err(error) => {
                    // Anything that isn't a `BigqueryServiceError` (eg request timeout) is retried, so it's transient.
                    let retriable = error
                        .downcast_ref::<BigqueryServiceError>()
                        .is_none_or(BigqueryServiceError::is_retryable);
                    if retriable {
                        emit!(GcpBigqueryRetriesExhaustedError {
                            reason: error.to_string(),
                            count,
                        });
                        Ok(BigqueryResponse::retries_exhausted())
                    } else {
                        Err(error)
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::FutureExt;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
    use tokio_stream::wrappers::UnboundedReceiverStream;
    use tonic::{Request, Response, Status};
    use tower::Service;
    use vector_lib::request_metadata::RequestMetadata;

    use vector_lib::event::{BatchNotifier, BatchStatus, Event, LogEvent};

    use super::{
        BigqueryRequest, BigqueryService, EventFinalizers, GrpcTransport, Transport, proto,
    };
    use crate::config::SinkConfig;
    use crate::sinks::gcp::bigquery::BigqueryConfig;
    use crate::test_util::generate_events_with_stream;

    /// How the fake server handles `AppendRows` calls.
    enum AppendRows {
        /// Forward each request to the test, which sends the response.
        Forward,
        /// Fail every call with this status.
        Fail(Status),
        /// Panic on the first call, then forward.
        PanicOnFirstCall,
    }

    /// A dumb `BigQueryWrite` server that can be used to test the `BigqueryService`.
    struct BigqueryServer {
        behavior: AppendRows,
        append_rows_calls: Arc<AtomicUsize>,
        append_rows_request_sender: UnboundedSender<(
            proto::AppendRowsRequest,
            tokio::sync::oneshot::Sender<Result<proto::AppendRowsResponse, Status>>,
        )>,
    }

    #[async_trait::async_trait]
    impl proto::big_query_write_server::BigQueryWrite for BigqueryServer {
        async fn create_write_stream(
            &self,
            _request: Request<proto::CreateWriteStreamRequest>,
        ) -> Result<Response<proto::WriteStream>, Status> {
            unimplemented!()
        }

        type AppendRowsStream =
            UnboundedReceiverStream<std::result::Result<proto::AppendRowsResponse, Status>>;

        async fn append_rows(
            &self,
            request: Request<tonic::Streaming<proto::AppendRowsRequest>>,
        ) -> std::result::Result<Response<Self::AppendRowsStream>, Status> {
            let call = self.append_rows_calls.fetch_add(1, Ordering::SeqCst);
            match &self.behavior {
                AppendRows::Fail(status) => return Err(status.clone()),
                AppendRows::PanicOnFirstCall if call == 0 => {
                    panic!("fake BigQuery server panicked on purpose")
                }
                AppendRows::Forward | AppendRows::PanicOnFirstCall => {}
            }

            let mut streaming = request.into_inner();
            let (sender, receiver) =
                unbounded_channel::<Result<proto::AppendRowsResponse, Status>>();
            let message_sender = self.append_rows_request_sender.clone();
            tokio::spawn(async move {
                loop {
                    match streaming.message().await.unwrap() {
                        Some(message) => {
                            let (stream_sender, stream_receiver) = tokio::sync::oneshot::channel();
                            message_sender.send((message, stream_sender)).unwrap();
                            let response = stream_receiver.await.unwrap();
                            sender.send(response).unwrap();
                        }
                        None => {
                            return;
                        }
                    }
                }
            });
            let receiver_stream = UnboundedReceiverStream::new(receiver);
            Ok(Response::new(receiver_stream))
        }

        async fn get_write_stream(
            &self,
            _request: Request<proto::GetWriteStreamRequest>,
        ) -> std::result::Result<Response<proto::WriteStream>, Status> {
            unimplemented!()
        }

        async fn finalize_write_stream(
            &self,
            _request: Request<proto::FinalizeWriteStreamRequest>,
        ) -> std::result::Result<Response<proto::FinalizeWriteStreamResponse>, Status> {
            unimplemented!()
        }

        async fn batch_commit_write_streams(
            &self,
            _request: Request<proto::BatchCommitWriteStreamsRequest>,
        ) -> std::result::Result<Response<proto::BatchCommitWriteStreamsResponse>, Status> {
            unimplemented!()
        }

        async fn flush_rows(
            &self,
            _request: Request<proto::FlushRowsRequest>,
        ) -> std::result::Result<Response<proto::FlushRowsResponse>, Status> {
            unimplemented!()
        }
    }

    /// Create a `TcpListener` on some arbitrary local address and a plaintext transport and origin for it
    async fn create_tcp_listener() -> (tokio::net::TcpListener, Transport, http::Uri) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let origin = format!("http://{addr}").parse().unwrap();
        let client = vector_http::Http2Client::new(
            crate::tls::MaybeTlsSettings::Raw(()),
            &crate::config::ProxyConfig::default(),
            hyper_util::client::legacy::Builder::new(hyper_util::rt::TokioExecutor::new()),
        )
        .unwrap();
        (listener, GrpcTransport(client), origin)
    }

    type RequestReceiver = UnboundedReceiver<(
        proto::AppendRowsRequest,
        tokio::sync::oneshot::Sender<Result<proto::AppendRowsResponse, Status>>,
    )>;

    /// A running fake `BigqueryServer`.
    struct TestServer {
        transport: Transport,
        origin: http::Uri,
        receiver: RequestReceiver,
        append_rows_calls: Arc<AtomicUsize>,
        shutdown: tokio::sync::oneshot::Sender<()>,
        join_handle: tokio::task::JoinHandle<()>,
    }

    async fn start_server(behavior: AppendRows) -> TestServer {
        let (sender, receiver) = unbounded_channel();
        let append_rows_calls = Arc::new(AtomicUsize::new(0));
        let bigquery_server = BigqueryServer {
            behavior,
            append_rows_calls: Arc::clone(&append_rows_calls),
            append_rows_request_sender: sender,
        };
        let (listener, transport, origin) = create_tcp_listener().await;
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let router = tonic::transport::server::Server::builder().add_service(
            proto::big_query_write_server::BigQueryWriteServer::new(bigquery_server),
        );
        let join_handle = tokio::spawn(async move {
            router
                .serve_with_incoming_shutdown(
                    tokio_stream::wrappers::TcpListenerStream::new(listener),
                    shutdown_rx.map(|x| x.unwrap()),
                )
                .await
                .unwrap();
        });
        TestServer {
            transport,
            origin,
            receiver,
            append_rows_calls,
            shutdown: shutdown_tx,
            join_handle,
        }
    }

    async fn run_server() -> (
        BigqueryService,
        RequestReceiver,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let server = start_server(AppendRows::Forward).await;
        let service = BigqueryService::new(
            server.transport,
            server.origin,
            crate::gcp::GcpAuthenticator::None,
            "test",
        )
        .unwrap();
        (
            service,
            server.receiver,
            server.shutdown,
            server.join_handle,
        )
    }

    async fn run_sink(append_rows: AppendRows, retry_attempts: usize) -> (BatchStatus, usize) {
        let mut server = start_server(append_rows).await;
        let responder = tokio::spawn(async move {
            while let Some((_request, respond)) = server.receiver.recv().await {
                _ = respond.send(Ok(proto::AppendRowsResponse {
                    response: Some(proto::append_rows_response::Response::AppendResult(
                        proto::append_rows_response::AppendResult { offset: None },
                    )),
                    ..Default::default()
                }));
            }
        });
        let desc_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("lib/codecs/tests/data/protobuf/integration.desc");
        let config = format!(
            r#"
            project = "testproject"
            dataset = "testdataset"
            table = "integration"
            endpoint = "{}"
            skip_authentication = true
            request.retry_attempts = {retry_attempts}
            request.retry_initial_backoff_secs = 1
            request.retry_jitter_mode = "None"
            encoding.protobuf.desc_file = {}
            encoding.protobuf.message_type = "test.Integration"
            "#,
            server.origin,
            toml::Value::String(desc_file.to_string_lossy().into_owned()),
        );
        let (config, cx) = crate::sinks::util::test::load_sink::<BigqueryConfig>(&config).unwrap();
        let (sink, _healthcheck) = config.build(cx).await.unwrap();

        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let (_, events) = generate_events_with_stream(
            |index| {
                Event::Log(LogEvent::from(BTreeMap::from([
                    ("time".into(), 0.into()),
                    ("count".into(), i64::try_from(index).unwrap().into()),
                    ("message".into(), "hello".into()),
                ])))
            },
            3,
            Some(batch),
        );
        sink.run(events).await.unwrap();
        let status = receiver.await;

        let calls = server.append_rows_calls.load(Ordering::SeqCst);
        responder.abort();
        server.shutdown.send(()).unwrap();
        server.join_handle.await.unwrap();
        (status, calls)
    }

    #[tokio::test]
    async fn permanent_rpc_error_is_rejected_without_retry() {
        let (status, calls) =
            run_sink(AppendRows::Fail(Status::invalid_argument("bad rows")), 3).await;
        assert_eq!(status, BatchStatus::Rejected);
        assert_eq!(calls, 1);
    }

    #[tokio::test]
    async fn transient_rpc_error_is_errored_after_retries() {
        let (status, calls) = run_sink(AppendRows::Fail(Status::unavailable("try again")), 1).await;
        // Exhausted retries should not be reported as rejected so acknowledging sources can
        // redeliver the events.
        assert_eq!(status, BatchStatus::Errored);
        assert_eq!(calls, 2);
    }

    /// A server-side stream reset is retried like other transport failures.
    #[tokio::test]
    async fn server_stream_reset_then_success_is_delivered() {
        let (status, calls) = run_sink(AppendRows::PanicOnFirstCall, 2).await;
        assert_eq!(status, BatchStatus::Delivered);
        assert_eq!(calls, 2);
    }

    #[tokio::test]
    async fn bigquery_service_stream() {
        let (mut service, mut receiver, shutdown, server_future) = run_server().await;
        // send a request and process the response
        let client_future = tokio::spawn(async move {
            assert!(
                service
                    .poll_ready(&mut std::task::Context::from_waker(
                        futures::task::noop_waker_ref(),
                    ))
                    .is_ready()
            );
            let response = service
                .call(BigqueryRequest {
                    request: proto::AppendRowsRequest {
                        write_stream: "test".to_string(),
                        offset: None,
                        trace_id: String::new(),
                        missing_value_interpretations: BTreeMap::new(),
                        default_missing_value_interpretation: 0,
                        rows: None,
                    },
                    metadata: RequestMetadata::default(),
                    finalizers: EventFinalizers::default(),
                    uncompressed_size: 1,
                })
                .await
                .unwrap();
            assert_eq!("ack", response.body.unwrap().write_stream);
        });
        // validate the request
        let (request, responder) = receiver.recv().await.unwrap();
        assert_eq!("test", request.write_stream);
        // respond to the request
        responder
            .send(Ok(proto::AppendRowsResponse {
                response: Some(proto::append_rows_response::Response::AppendResult(
                    proto::append_rows_response::AppendResult { offset: None },
                )),
                write_stream: "ack".into(),
                updated_schema: None,
                row_errors: Vec::new(),
            }))
            .unwrap();
        // clean everything up
        shutdown.send(()).unwrap();
        client_future.await.unwrap();
        server_future.await.unwrap();
    }
}

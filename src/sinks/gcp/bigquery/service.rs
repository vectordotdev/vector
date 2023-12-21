use futures::future::BoxFuture;
use snafu::Snafu;
use std::task::{Context, Poll};
use tonic::metadata::MetadataValue;
use tonic::service::interceptor::InterceptedService;
use tonic::service::Interceptor;
use tonic::transport::Channel;
use tonic::{Request, Status};
use tower::Service;
use vector_common::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_core::event::EventStatus;
use vector_core::stream::DriverResponse;

use super::proto::google::cloud::bigquery::storage::v1 as proto;
use crate::event::{EventFinalizers, Finalizable};
use crate::gcp::GcpAuthenticator;

#[derive(Clone)]
pub struct AuthInterceptor {
    pub auth: GcpAuthenticator,
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(token) = self.auth.make_token() {
            let value: MetadataValue<_> = token
                .parse()
                .map_err(|e| tonic::Status::unauthenticated(format!("{e:?}")))?;
            request.metadata_mut().insert("authorization", value);
        }
        Ok(request)
    }
}

pub struct BigqueryRequest {
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
pub struct BigqueryResponse {
    body: proto::AppendRowsResponse,
    request_byte_size: GroupedCountByteSize,
    request_uncompressed_size: usize,
}

impl DriverResponse for BigqueryResponse {
    fn event_status(&self) -> EventStatus {
        if !self.body.row_errors.is_empty() {
            // The AppendRowsResponse reports on specific rows that failed to append,
            // meaning that in theory on failures we can retry the request without the bad events.
            // Unfortunately there's no good mechanism for doing this in the Vector model;
            // it's assumed either the whole thing is successful or it is not.
            return EventStatus::Rejected;
        }
        match &self.body.response {
            None => EventStatus::Dropped,
            Some(proto::append_rows_response::Response::AppendResult(_)) => EventStatus::Delivered,
            Some(proto::append_rows_response::Response::Error(status)) => {
                match super::proto::google::rpc::Code::try_from(status.code) {
                    // we really shouldn't be able to get here, but just in case
                    Ok(super::proto::google::rpc::Code::Ok) => EventStatus::Delivered,
                    // these errors can't be retried because the event payload is almost definitely bad
                    Ok(super::proto::google::rpc::Code::InvalidArgument)
                    | Ok(super::proto::google::rpc::Code::NotFound)
                    | Ok(super::proto::google::rpc::Code::AlreadyExists) => EventStatus::Rejected,
                    // everything else can probably be retried
                    _ => EventStatus::Errored,
                }
            }
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
pub enum BigqueryServiceError {
    #[snafu(display("Error communicating with BigQuery: {}", error))]
    Transport { error: tonic::transport::Error },
    #[snafu(display("BigQuery request failure: {}", status))]
    Request { status: tonic::Status },
    #[snafu(display("BigQuery row write failures: {:?}", row_errors))]
    RowWrite { row_errors: Vec<proto::RowError> },
}

impl From<tonic::transport::Error> for BigqueryServiceError {
    fn from(error: tonic::transport::Error) -> Self {
        Self::Transport { error }
    }
}

impl From<tonic::Status> for BigqueryServiceError {
    fn from(status: tonic::Status) -> Self {
        Self::Request { status }
    }
}

impl From<Vec<proto::RowError>> for BigqueryServiceError {
    fn from(row_errors: Vec<proto::RowError>) -> Self {
        Self::RowWrite { row_errors }
    }
}

type BigQueryWriteClient = proto::big_query_write_client::BigQueryWriteClient<
    InterceptedService<tonic::transport::Channel, AuthInterceptor>,
>;

pub struct BigqueryService {
    service: BigQueryWriteClient,
}

impl BigqueryService {
    pub async fn with_auth(channel: Channel, auth: GcpAuthenticator) -> crate::Result<Self> {
        let service = proto::big_query_write_client::BigQueryWriteClient::with_interceptor(
            channel,
            AuthInterceptor { auth },
        );
        Ok(Self { service })
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

        Box::pin(async move {
            // Ideally, we would maintain the gRPC stream, detect when auth expired and re-request with new auth.
            // But issuing a new request every time leads to more comprehensible code with reasonable performance.
            trace!(
                message = "Sending request to BigQuery",
                request = format!("{:?}", request.request),
            );
            let stream = tokio_stream::once(request.request);
            let response = client.append_rows(stream).await?;
            match response.into_inner().message().await? {
                Some(body) => {
                    trace!(
                        message = "Received response body from BigQuery",
                        body = format!("{:?}", body),
                    );
                    if body.row_errors.is_empty() {
                        Ok(BigqueryResponse {
                            body,
                            request_byte_size,
                            request_uncompressed_size,
                        })
                    } else {
                        Err(body.row_errors.into())
                    }
                }
                None => Err(tonic::Status::unknown("response stream closed").into()),
            }
        })
    }
}

#[cfg(test)]
mod test {
    use futures::FutureExt;
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
    use tokio_stream::wrappers::UnboundedReceiverStream;
    use tonic::{Request, Response, Status};
    use tower::Service;

    use super::{proto, BigqueryRequest, BigqueryService};

    /// A dumb BigQueryWrite server that can be used to test the BigqueryService.
    struct BigqueryServer {
        error: Option<Status>,
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
            if let Some(error) = &self.error {
                return Err(error.clone());
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

    /// Create a TcpListener on some arbitrary local address and an HTTP Channel that's connected to it
    async fn create_tcp_listener() -> (tokio::net::TcpListener, tonic::transport::Channel) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let uri = tonic::transport::Uri::builder()
            .scheme("http")
            .authority(format!("{}:{}", addr.ip(), addr.port()))
            .path_and_query("/")
            .build()
            .unwrap();
        let channel = tonic::transport::Channel::builder(uri)
            .connect()
            .await
            .unwrap();
        (listener, channel)
    }

    /// Run a fake BigqueryServer, providing a client, a request handler, and a handle to shut it down.
    async fn run_server() -> (
        BigqueryService,
        UnboundedReceiver<(
            proto::AppendRowsRequest,
            tokio::sync::oneshot::Sender<Result<proto::AppendRowsResponse, Status>>,
        )>,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let (sender, receiver) = unbounded_channel();
        let bigquery_server = BigqueryServer {
            error: None,
            append_rows_request_sender: sender,
        };
        let (listener, channel) = create_tcp_listener().await;
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
        let service = BigqueryService::with_auth(channel, crate::gcp::GcpAuthenticator::None)
            .await
            .unwrap();
        (service, receiver, shutdown_tx, join_handle)
    }

    #[tokio::test]
    async fn bigquery_service_stream() {
        let (mut service, mut receiver, shutdown, server_future) = run_server().await;
        // send a request and process the response
        let client_future = tokio::spawn(async move {
            assert!(service
                .poll_ready(&mut std::task::Context::from_waker(
                    futures::task::noop_waker_ref(),
                ))
                .is_ready());
            let response = service
                .call(BigqueryRequest {
                    request: proto::AppendRowsRequest {
                        write_stream: "test".to_string(),
                        offset: None,
                        trace_id: "".to_string(),
                        missing_value_interpretations: Default::default(),
                        default_missing_value_interpretation: 0,
                        rows: None,
                    },
                    metadata: Default::default(),
                    finalizers: Default::default(),
                    uncompressed_size: 1,
                })
                .await
                .unwrap();
            assert_eq!("ack", response.body.write_stream);
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
                row_errors: Default::default(),
            }))
            .unwrap();
        // clean everything up
        shutdown.send(()).unwrap();
        client_future.await.unwrap();
        server_future.await.unwrap();
    }
}

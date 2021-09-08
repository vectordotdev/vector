use crate::{
    config::{DataType, GenerateConfig, Resource, SinkContext, SinkHealthcheckOptions},
    event::{proto::EventWrapper, Event},
    proto::vector as proto,
    sinks::util::{
        retries::RetryLogic, sink, BatchConfig, BatchSettings, BatchSink, EncodedEvent,
        EncodedLength, ServiceBuilderExt, TowerRequestConfig, VecBuffer,
    },
    sinks::{Healthcheck, VectorSink},
    tls::{tls_connector_builder, MaybeTlsSettings, TlsConfig},
};
use futures::{future::BoxFuture, stream, SinkExt, StreamExt, TryFutureExt};
use http::uri::Uri;
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use prost::Message;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::task::{Context, Poll};
use tonic::{body::BoxBody, IntoRequest};
use tower::ServiceBuilder;

type Client = proto::Client<HyperSvc>;
type Response = Result<tonic::Response<proto::PushEventsResponse>, tonic::Status>;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    address: String,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(default)]
    tls: Option<TlsConfig>,
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(default_config("127.0.0.1:6000")).unwrap()
    }
}

fn default_config(address: &str) -> VectorConfig {
    VectorConfig {
        address: address.to_owned(),
        batch: BatchConfig::default(),
        request: TowerRequestConfig::default(),
        tls: None,
    }
}

/// grpc doesn't like an address without a scheme, so we default to http or https if one isn't
/// specified in the address.
fn with_default_scheme(address: &str, tls: bool) -> crate::Result<Uri> {
    let uri: Uri = address.parse()?;
    if uri.scheme().is_none() {
        // Default the scheme to http or https.
        let mut parts = uri.into_parts();

        parts.scheme = if tls {
            Some(
                "https"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("https should be valid")),
            )
        } else {
            Some(
                "http"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("http should be valid")),
            )
        };

        if parts.path_and_query.is_none() {
            parts.path_and_query = Some(
                "/".parse()
                    .unwrap_or_else(|_| unreachable!("root should be valid")),
            );
        }
        Ok(Uri::from_parts(parts)?)
    } else {
        Ok(uri)
    }
}

fn new_client(
    tls_settings: &MaybeTlsSettings,
) -> crate::Result<hyper::Client<HttpsConnector<HttpConnector>, BoxBody>> {
    let mut http = HttpConnector::new();
    http.enforce_http(false);

    let tls = tls_connector_builder(tls_settings)?;
    let mut https = HttpsConnector::with_connector(http, tls)?;

    let settings = tls_settings.tls().cloned();
    https.set_callback(move |c, _uri| {
        if let Some(settings) = &settings {
            settings.apply_connect_configuration(c);
        }

        Ok(())
    });

    Ok(hyper::Client::builder().http2_only(true).build(https))
}

#[derive(Clone)]
struct HyperSvc {
    uri: Uri,
    client: hyper::Client<HttpsConnector<HttpConnector>, BoxBody>,
}

impl tower::Service<hyper::Request<BoxBody>> for HyperSvc {
    type Response = hyper::Response<hyper::Body>;
    type Error = hyper::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: hyper::Request<BoxBody>) -> Self::Future {
        let uri = Uri::builder()
            .scheme(self.uri.scheme().unwrap().clone())
            .authority(self.uri.authority().unwrap().clone())
            .path_and_query(req.uri().path_and_query().unwrap().clone())
            .build()
            .unwrap();

        *req.uri_mut() = uri;

        Box::pin(self.client.request(req))
    }
}

impl VectorConfig {
    pub(crate) async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let tls = MaybeTlsSettings::from_config(&self.tls, false)?;
        let uri = with_default_scheme(&self.address, tls.is_tls())?;

        let client = new_client(&tls)?;

        let healthcheck_uri = cx
            .healthcheck
            .uri
            .clone()
            .map(|uri| uri.uri)
            .unwrap_or_else(|| uri.clone());
        let healthcheck_client = proto::Client::new(HyperSvc {
            uri: healthcheck_uri,
            client: client.clone(),
        });

        let healthcheck = healthcheck(healthcheck_client, cx.healthcheck.clone());
        let client = proto::Client::new(HyperSvc { uri, client });
        let request = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch = BatchSettings::default()
            .events(1000)
            .timeout(1)
            .parse_config(self.batch)?;

        let svc = ServiceBuilder::new()
            .settings(request, VectorGrpcRetryLogic)
            .service(client);

        let buffer = VecBuffer::new(batch.size);
        let sink = BatchSink::new(svc, buffer, batch.timeout, cx.acker())
            .sink_map_err(|error| error!(message = "Fatal Vector GRPC sink error.", %error))
            .with_flat_map(move |event| stream::iter(Some(encode_event(event))).map(Ok));

        Ok((VectorSink::Sink(Box::new(sink)), Box::pin(healthcheck)))
    }

    pub(super) const fn input_type(&self) -> DataType {
        DataType::Any
    }

    pub(super) const fn sink_type(&self) -> &'static str {
        "vector"
    }

    pub(super) const fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }
}

/// Check to see if the remote service accepts new events.
async fn healthcheck(mut client: Client, options: SinkHealthcheckOptions) -> crate::Result<()> {
    if !options.enabled {
        return Ok(());
    }

    let request = client.health_check(proto::HealthCheckRequest {});

    if let Ok(response) = request.await {
        let status = proto::ServingStatus::from_i32(response.into_inner().status);

        if let Some(proto::ServingStatus::Serving) = status {
            return Ok(());
        }
    }

    Err(Box::new(Error::Health))
}

impl tower::Service<Vec<EventWrapper>> for Client {
    type Response = ();
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Readiness check of the client is done through the `push_events()`
        // call happening inside `call()`. That check blocks until the client is
        // ready to perform another request.
        //
        // See: <https://docs.rs/tonic/0.4.2/tonic/client/struct.Grpc.html#method.ready>
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, events: Vec<EventWrapper>) -> Self::Future {
        let mut client = self.clone();

        let request = proto::PushEventsRequest { events };
        let future = async move {
            client
                .push_events(request.into_request())
                .map_ok(|_| ())
                .map_err(|source| Error::Request { source })
                .await
        };

        Box::pin(future)
    }
}

fn encode_event(mut event: Event) -> EncodedEvent<EventWrapper> {
    let finalizers = event.metadata_mut().take_finalizers();
    let item = event.into();

    EncodedEvent { item, finalizers }
}

impl EncodedLength for EventWrapper {
    fn encoded_length(&self) -> usize {
        self.encoded_len()
    }
}

impl sink::Response for Response {
    fn is_successful(&self) -> bool {
        self.is_ok()
    }
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Request failed: {}", source))]
    Request { source: tonic::Status },

    #[snafu(display("Vector source unhealthy"))]
    Health,

    #[snafu(display("URL has no host."))]
    NoHost,
}

#[derive(Debug, Clone)]
struct VectorGrpcRetryLogic;

impl RetryLogic for VectorGrpcRetryLogic {
    type Error = Error;
    type Response = ();

    fn is_retriable_error(&self, err: &Self::Error) -> bool {
        use tonic::Code::*;

        match err {
            Error::Request { source } => !matches!(
                source.code(),
                // List taken from
                //
                // <https://github.com/grpc/grpc/blob/ed1b20777c69bd47e730a63271eafc1b299f6ca0/doc/statuscodes.md>
                NotFound
                    | InvalidArgument
                    | AlreadyExists
                    | PermissionDenied
                    | OutOfRange
                    | Unimplemented
                    | Unauthenticated
            ),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::SinkContext,
        sinks::util::test::build_test_server_generic,
        test_util::{next_addr, random_lines_with_stream},
    };
    use bytes::{BufMut, Bytes, BytesMut};
    use futures::{channel::mpsc, StreamExt};
    use http::request::Parts;
    use hyper::Method;
    use vector_core::event::{BatchNotifier, BatchStatus};

    // one byte for the compression flag plus four bytes for the length
    const GRPC_HEADER_SIZE: usize = 5;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<VectorConfig>();
    }

    #[tokio::test]
    async fn deliver_message() {
        let num_lines = 10;

        let in_addr = next_addr();

        let config = format!(r#"address = "http://{}/""#, in_addr);
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let cx = SinkContext::new_test();

        let (sink, _) = config.build(cx).await.unwrap();
        let (rx, trigger, server) = build_test_server_generic(in_addr, move || {
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });

        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        sink.run(events).await.unwrap();
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = get_received(rx, |parts| {
            assert_eq!(Method::POST, parts.method);
            assert_eq!("/vector.Vector/PushEvents", parts.uri.path());
            assert_eq!(
                "application/grpc",
                parts.headers.get("content-type").unwrap().to_str().unwrap()
            );
        })
        .await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[tokio::test]
    async fn acknowledges_error() {
        let num_lines = 10;

        let in_addr = next_addr();

        let config = format!(r#"address = "http://{}/""#, in_addr);
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let cx = SinkContext::new_test();

        let (sink, _) = config.build(cx).await.unwrap();
        let (_rx, trigger, server) = build_test_server_generic(in_addr, move || {
            hyper::Response::builder()
                .header("grpc-status", "7") // permission denied
                .header("content-type", "application/grpc")
                .body(tonic::body::empty_body())
                .unwrap()
        });

        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_, events) = random_lines_with_stream(8, num_lines, Some(batch));

        sink.run(events).await.unwrap();
        drop(trigger);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Errored));
    }

    #[test]
    fn test_with_default_scheme() {
        assert_eq!(
            with_default_scheme("0.0.0.0", false).unwrap().to_string(),
            "http://0.0.0.0/"
        );
        assert_eq!(
            with_default_scheme("0.0.0.0", true).unwrap().to_string(),
            "https://0.0.0.0/"
        );
    }

    async fn get_received(
        rx: mpsc::Receiver<(Parts, Bytes)>,
        assert_parts: impl Fn(Parts),
    ) -> Vec<String> {
        rx.map(|(parts, body)| {
            assert_parts(parts);

            let proto_body = body.slice(GRPC_HEADER_SIZE..);

            let req = proto::PushEventsRequest::decode(proto_body).unwrap();

            let mut events = Vec::with_capacity(req.events.len());
            for event in req.events {
                let event: Event = event.into();
                let string = event.as_log().get("message").unwrap().to_string_lossy();
                events.push(string)
            }

            events
        })
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect()
    }

    // taken from <https://github.com/hyperium/tonic/blob/5aa8ae1fec27377cd4c2a41d309945d7e38087d0/examples/src/grpc-web/client.rs#L45-L75>
    fn encode_body<T>(msg: T) -> Bytes
    where
        T: prost::Message,
    {
        let mut buf = BytesMut::with_capacity(1024);

        // first skip past the header
        // cannot write it yet since we don't know the size of the
        // encoded message
        buf.reserve(GRPC_HEADER_SIZE);
        unsafe {
            buf.advance_mut(GRPC_HEADER_SIZE);
        }

        // write the message
        msg.encode(&mut buf).unwrap();

        // now we know the size of encoded message and can write the
        // header
        let len = buf.len() - GRPC_HEADER_SIZE;
        {
            let mut buf = &mut buf[..GRPC_HEADER_SIZE];

            // compression flag, 0 means "no compression"
            buf.put_u8(0);

            buf.put_u32(len as u32);
        }

        buf.split_to(len + GRPC_HEADER_SIZE).freeze()
    }
}

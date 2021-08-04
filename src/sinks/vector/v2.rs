use crate::{
    config::{DataType, GenerateConfig, Resource, SinkContext, SinkHealthcheckOptions},
    event::{proto::EventWrapper, Event},
    proto::vector as proto,
    sinks::util::{
        retries::RetryLogic, sink, BatchConfig, BatchSettings, BatchSink, EncodedEvent,
        EncodedLength, ServiceBuilderExt, TowerRequestConfig, VecBuffer,
    },
    sinks::{Healthcheck, VectorSink},
};
use futures::{future::BoxFuture, stream, SinkExt, StreamExt, TryFutureExt};
use http::uri::Uri;
use prost::Message;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::path::PathBuf;
use std::task::{Context, Poll};
use tonic::{
    transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity},
    IntoRequest,
};
use tower::ServiceBuilder;

type Client = proto::Client<Channel>;
type Response = Result<tonic::Response<proto::PushEventsResponse>, tonic::Status>;

// TODO: rename
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    address: String,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(default)]
    pub tls: Option<GrpcTlsConfig>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct GrpcTlsConfig {
    ca_file: PathBuf,
    crt_file: PathBuf,
    key_file: PathBuf,
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

/// grpc doesn't like an address without a scheme, so we default to http if one isn't specified in
/// the address.
fn default_http(address: &str) -> crate::Result<Uri> {
    let uri: Uri = address.parse()?;
    if uri.scheme().is_none() {
        // Default the scheme to http.
        let mut parts = uri.into_parts();
        parts.scheme = Some(
            "http"
                .parse()
                .unwrap_or_else(|_| unreachable!("http should be valid")),
        );
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

impl VectorConfig {
    pub(crate) async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint = Endpoint::from(default_http(&self.address)?);
        let endpoint = match &self.tls {
            Some(tls) => {
                let host = get_authority(&self.address)?;
                let ca = Certificate::from_pem(tokio::fs::read(&tls.ca_file).await?);
                let crt = tokio::fs::read(&tls.crt_file).await?;
                let key = tokio::fs::read(&tls.key_file).await?;
                let identity = Identity::from_pem(crt, key);

                let tls_config = ClientTlsConfig::new()
                    .identity(identity)
                    .ca_certificate(ca)
                    .domain_name(host);

                endpoint.tls_config(tls_config)?
            }
            None => endpoint,
        };

        let client = proto::Client::new(endpoint.connect_lazy()?);

        let healthcheck_client = if let Some(uri) = cx.healthcheck.uri.clone() {
            let endpoint = Endpoint::from(uri.uri);
            proto::Client::new(endpoint.connect_lazy()?)
        } else {
            client.clone()
        };

        let healthcheck = healthcheck(healthcheck_client, cx.healthcheck.clone());

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

    pub(super) fn input_type(&self) -> DataType {
        DataType::Any
    }

    pub(super) fn sink_type(&self) -> &'static str {
        "vector"
    }

    pub(super) fn resources(&self) -> Vec<Resource> {
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

fn get_authority(url: &str) -> Result<String, Error> {
    url.parse::<Uri>()
        .ok()
        .and_then(|uri| uri.authority().map(ToString::to_string))
        .ok_or(Error::NoHost)
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
        match err {
            Error::Request { source } => !matches!(source.code(), tonic::Code::Unknown),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::SinkContext,
        sinks::util::test::build_test_server_status,
        test_util::{next_addr, random_lines_with_stream},
    };
    use bytes::Bytes;
    use futures::{channel::mpsc, StreamExt};
    use http::{request::Parts, StatusCode};
    use hyper::Method;
    use vector_core::event::{BatchNotifier, BatchStatus};

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
        let (rx, trigger, server) = build_test_server_status(in_addr, StatusCode::OK);
        tokio::spawn(server);

        let (batch, _receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        sink.run(events).await.unwrap();
        drop(trigger);
        // This check fails, ref https://github.com/timberio/vector/issues/7624
        // assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

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
    #[ignore] // This test hangs, possibly an infinite retry loop
    async fn acknowledges_error() {
        let num_lines = 10;

        let in_addr = next_addr();

        let config = format!(r#"address = "http://{}/""#, in_addr);
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let cx = SinkContext::new_test();

        let (sink, _) = config.build(cx).await.unwrap();
        let (rx, trigger, server) = build_test_server_status(in_addr, StatusCode::FORBIDDEN);
        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        sink.run(events).await.unwrap();
        drop(trigger);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Errored));

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

    async fn get_received(
        rx: mpsc::Receiver<(Parts, Bytes)>,
        assert_parts: impl Fn(Parts),
    ) -> Vec<String> {
        rx.map(|(parts, body)| {
            assert_parts(parts);

            // Remove the grpc header, which is:
            // 1 bytes for compressed/not compressed
            // 4 bytes for the message len
            // https://github.com/grpc/grpc/blob/master/doc/PROTOCOL-HTTP2.md#requests
            let proto_body = body.slice(5..);

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
}

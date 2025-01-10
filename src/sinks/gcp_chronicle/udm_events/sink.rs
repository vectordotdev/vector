//! This sink sends data to Google Chronicles UDM Events log entries endpoint.
//! See <https://cloud.google.com/chronicle/docs/reference/ingestion-api#udmevents>
//! for more information.
use bytes::BytesMut;
use http::header::{self, HeaderValue};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::io;
use std::fmt;
use tokio_util::codec::Encoder as _;
use tower::ServiceBuilder;
use vector_lib::{
    config::telemetry,
    event::{Event, EventFinalizers, Finalizable},
    sink::VectorSink,
};
use vector_lib::{
    request_metadata::{GroupedCountByteSize, RequestMetadata},
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    codecs,
    gcp::GcpAuthenticator,
    http::HttpClient,

    sinks::{
        gcp_chronicle::{
            service::ChronicleService, ChronicleRequest,
            ChronicleRequestPayload
        },
        gcs_common::config::GcsRetryLogic,
        prelude::*,
        util::{
            encoding::{as_tracked_write, Encoder},
            metadata::RequestMetadataBuilder,
            request_builder::EncodeResult,
            Compression, RequestBuilder,
        },
    },
};

use super::config::ChronicleUDMEventsConfig;

#[derive(Clone, Debug, Serialize)]
struct ChronicleUDMEventsRequestBody {
    customer_id: String,
    events: Vec<serde_json::Value>,
}

#[derive(Clone, Debug)]
struct ChronicleUDMEventsEncoder {
    customer_id: String,
    encoder: codecs::Encoder<()>,
    transformer: codecs::Transformer,
}

impl Encoder<Vec<Event>> for ChronicleUDMEventsEncoder {
    fn encode_input(
        &self,
        input: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut encoder = self.encoder.clone();
        let mut byte_size = telemetry().create_request_count_byte_size();
        let events = input
            .into_iter()
            .filter_map(|mut event| {
                let mut bytes = BytesMut::new();
                self.transformer.transform(&mut event);

                byte_size.add_event(&event, event.estimated_json_encoded_size_of());

                encoder.encode(event, &mut bytes).ok()?;

                let value = json!(String::from_utf8_lossy(&bytes));

                Some(value)
            })
            .collect::<Vec<_>>();

        let json = json!(ChronicleUDMEventsRequestBody {
            customer_id: self.customer_id.clone(),
            events: events,
        });

        let size = as_tracked_write::<_, _, io::Error>(writer, &json, |writer, json| {
            serde_json::to_writer(writer, json)?;
            Ok(())
        })?;

        Ok((size, byte_size))
    }
}

#[derive(Clone, Debug)]
struct ChronicleUDMEventsRequestBuilder {
    encoder: ChronicleUDMEventsEncoder,
    compression: Compression,
}

impl ChronicleUDMEventsRequestBuilder {
    fn new(config: &ChronicleUDMEventsConfig) -> crate::Result<Self> {
        let transformer = config.chronicle_common.encoding.transformer();
        let serializer = config.chronicle_common.encoding.config().build()?;
        let compression = Compression::from(config.chronicle_common.compression);
        let encoder = crate::codecs::Encoder::<()>::new(serializer);
        let encoder = ChronicleUDMEventsEncoder {
            customer_id: config.chronicle_common.customer_id.clone(),
            encoder,
            transformer,
        };
        Ok(Self {
            encoder,
            compression,
        })
    }
}

impl RequestBuilder<Vec<Event>> for ChronicleUDMEventsRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = ChronicleUDMEventsEncoder;
    type Payload = ChronicleRequestPayload;
    type Request = ChronicleRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: Vec<Event>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let mut events = input;
        let finalizers = events.take_finalizers();

        let builder = RequestMetadataBuilder::from_events(&events);
        (finalizers, builder, events)
    }

    fn build_request(
        &self,
        finalizers: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let mut headers = HashMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        match payload.compressed_byte_size {
            Some(compressed_byte_size) => {
                headers.insert(
                    header::CONTENT_LENGTH,
                    HeaderValue::from_str(&compressed_byte_size.to_string()).unwrap(),
                );
                headers.insert(
                    header::CONTENT_ENCODING,
                    HeaderValue::from_str(&self.compression.content_encoding().unwrap()).unwrap(),
                );
            }
            None => {
                headers.insert(
                    header::CONTENT_LENGTH,
                    HeaderValue::from_str(&payload.uncompressed_byte_size.to_string()).unwrap(),
                );
            }
        }

        return ChronicleRequest {
            headers,
            body: payload.into_payload().bytes,
            finalizers,
            metadata,
        };
    }
}

impl ChronicleUDMEventsConfig {
    pub fn build_sink(
        &self,
        client: HttpClient,
        base_url: String,
        creds: GcpAuthenticator,
    ) -> crate::Result<VectorSink> {
        use crate::sinks::util::service::ServiceBuilderExt;

        let request = self.chronicle_common.request.into_settings();
        let batch_settings = self.chronicle_common.batch.into_batcher_settings()?;

        let svc = ServiceBuilder::new()
            .settings(request, GcsRetryLogic)
            .service(ChronicleService::new(client, base_url, creds));

        let request_settings = ChronicleUDMEventsRequestBuilder::new(self)?;

        let sink = ChronicleUDMEventsSink::new(svc, request_settings, batch_settings, "http");

        Ok(VectorSink::from_event_streamsink(sink))
    }
}

pub struct ChronicleUDMEventsSink<Svc, RB> {
    service: Svc,
    request_builder: RB,
    batcher_settings: BatcherSettings,
    protocol: &'static str,
}

impl<Svc, RB> ChronicleUDMEventsSink<Svc, RB> {
    pub const fn new(
        service: Svc,
        request_builder: RB,
        batcher_settings: BatcherSettings,
        protocol: &'static str,
    ) -> Self {
        Self {
            service,
            request_builder,
            batcher_settings,
            protocol,
        }
    }
}

impl<Svc, RB> ChronicleUDMEventsSink<Svc, RB>
where
    Svc: Service<RB::Request> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
    RB: RequestBuilder<Vec<Event>> + Send + Sync + 'static,
    RB::Error: fmt::Display + Send,
    RB::Request: Finalizable + MetaDescriptive + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let settings = self.batcher_settings;
        let request_builder = self.request_builder;

        input
            .batched(settings.as_byte_size_config())
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .protocol(self.protocol)
            .run()
            .await
    }
}

#[async_trait]
impl<Svc, RB> StreamSink<Event> for ChronicleUDMEventsSink<Svc, RB>
where
    Svc: Service<RB::Request> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
    RB: RequestBuilder<Vec<Event>> + Send + Sync + 'static,
    RB::Error: fmt::Display + Send,
    RB::Request: Finalizable + MetaDescriptive + Send,
{
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[cfg(all(test, feature = "chronicle-udm-events-integration-tests"))]
mod integration_tests {
    use indoc::indoc;
    use reqwest::{Client, Method, Response};
    use serde::{Deserialize, Serialize};
    use vector_lib::event::{BatchNotifier, BatchStatus};

    use super::*;
    use crate::test_util::{
        components::{
            run_and_assert_sink_compliance, run_and_assert_sink_error, COMPONENT_ERROR_TAGS,
            SINK_TAGS,
        },
        random_events_with_stream, random_string, trace_init,
    };

    const ADDRESS_ENV_VAR: &str = "CHRONICLE_ADDRESS";

    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct UdmMetadata {
        event_timestamp: String,
        log_type: String
    }

    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct Log {
        metadata: UdmMetadata
    }

    fn config(auth_path: &str) -> ChronicleUDMEventsConfig {
        let address = std::env::var(ADDRESS_ENV_VAR).unwrap();
        let config = format!(
            indoc! { r#"
                endpoint = "{}"
                customer_id = "customer id"
                credentials_path = "{}"
                encoding.codec = "json"
            "# },
            address, auth_path
        );

        let config: ChronicleUDMEventsConfig = toml::from_str(&config).unwrap();
        config
    }

    async fn config_build(
        auth_path: &str,
    ) -> crate::Result<(VectorSink, crate::sinks::Healthcheck)> {
        let cx = SinkContext::default();
        config(auth_path).build(cx).await
    }

    #[tokio::test]
    async fn publish_events() {
        trace_init();

        let log_type = random_string(10);
        let (sink, healthcheck) =
            config_build("/home/vector/scripts/integration/gcp/auth.json")
                .await
                .expect("Building sink failed");

        healthcheck.await.expect("Health check failed");

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input, events) = random_events_with_stream(100, 100, Some(batch));
        run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let response = pull_messages(&log_type).await;
        assert_eq!(input.len(), response.len());
    }

    #[tokio::test]
    async fn invalid_credentials() {
        trace_init();
        // Test with an auth file that doesnt match the public key sent to the dummy chronicle server.
        let sink = config_build(
            "/home/vector/scripts/integration/gcp/invalidauth.json",
        )
        .await;

        assert!(sink.is_err())
    }

    #[tokio::test]
    async fn publish_invalid_events() {
        trace_init();

        let (sink, healthcheck) =
            config_build("/home/vector/scripts/integration/gcp/auth.json")
                .await
                .expect("Building sink failed");

        healthcheck.await.expect("Health check failed");

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_input, events) = random_events_with_stream(100, 100, Some(batch));
        run_and_assert_sink_error(sink, events, &COMPONENT_ERROR_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
    }

    async fn request(method: Method, path: &str, log_type: &str) -> Response {
        let address = std::env::var(ADDRESS_ENV_VAR).unwrap();
        let url = format!("{}/{}", address, path);
        Client::new()
            .request(method.clone(), &url)
            .query(&[("log_type", log_type)])
            .send()
            .await
            .unwrap_or_else(|_| panic!("Sending {} request to {} failed", method, url))
    }

    async fn pull_messages(log_type: &str) -> Vec<Log> {
        request(Method::GET, "logs", log_type)
            .await
            .json::<Vec<Log>>()
            .await
            .expect("Extracting pull data failed")
    }
}

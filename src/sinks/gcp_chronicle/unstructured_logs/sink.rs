//! This sink sends data to Google Chronicles unstructured log entries endpoint.
//! See <https://cloud.google.com/chronicle/docs/reference/ingestion-api#unstructuredlogentries>
//! for more information.
use bytes::BytesMut;
use std::fmt;

use http::header::{self, HeaderValue};
use serde::Serialize;
use serde_json::json;
use tower::ServiceBuilder;
use std::collections::HashMap;
use std::io;
use tokio_util::codec::Encoder as _;
use vector_lib::{
    request_metadata::{GroupedCountByteSize, RequestMetadata},
    sink::VectorSink,
    config::telemetry,
    event::{Event, EventFinalizers, Finalizable},
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    codecs,
    gcp::GcpAuthenticator,
    sinks::prelude::*,
    http::HttpClient, sinks::{
        gcp_chronicle::{
            service::ChronicleService, ChronicleRequest, ChronicleRequestPayload
        },
        gcs_common::config::GcsRetryLogic,
        util::{
            encoding::{as_tracked_write, Encoder},
            metadata::RequestMetadataBuilder,
            request_builder::EncodeResult,
            Compression, RequestBuilder,
        },
    }
};

use super::{
    partitioner::{ChroniclePartitionKey, ChroniclePartitioner},
    config::ChronicleUnstructuredConfig
};

#[derive(Clone, Debug, Serialize)]
struct ChronicleUnstructuredRequestBody {
    customer_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<Vec<Label>>,
    log_type: String,
    entries: Vec<serde_json::Value>,
}

#[derive(Clone, Debug)]
struct ChronicleUnstructuredEncoder {
    customer_id: String,
    labels: Option<Vec<Label>>,
    encoder: codecs::Encoder<()>,
    transformer: codecs::Transformer,
}

impl Encoder<(ChroniclePartitionKey, Vec<Event>)> for ChronicleUnstructuredEncoder {
    fn encode_input(
        &self,
        input: (ChroniclePartitionKey, Vec<Event>),
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let (key, events) = input;
        let mut encoder = self.encoder.clone();
        let mut byte_size = telemetry().create_request_count_byte_size();
        let events = events
            .into_iter()
            .filter_map(|mut event| {
                let timestamp = event
                    .as_log()
                    .get_timestamp()
                    .and_then(|ts| ts.as_timestamp())
                    .cloned();
                let mut bytes = BytesMut::new();
                self.transformer.transform(&mut event);

                byte_size.add_event(&event, event.estimated_json_encoded_size_of());

                encoder.encode(event, &mut bytes).ok()?;

                let mut value = json!({
                    "log_text": String::from_utf8_lossy(&bytes),
                });

                if let Some(ts) = timestamp {
                    value.as_object_mut().unwrap().insert(
                        "ts_rfc3339".to_string(),
                        ts.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
                            .into(),
                    );
                }

                Some(value)
            })
            .collect::<Vec<_>>();

        let json = json!(ChronicleUnstructuredRequestBody {
            customer_id: self.customer_id.clone(),
            namespace: key.namespace,
            labels: self.labels.clone(),
            log_type: key.log_type,
            entries: events,
        });

        let size = as_tracked_write::<_, _, io::Error>(writer, &json, |writer, json| {
            serde_json::to_writer(writer, json)?;
            Ok(())
        })?;

        Ok((size, byte_size))
    }
}

// Settings required to produce a request that do not change per
// request. All possible values are pre-computed for direct use in
// producing a request.
#[derive(Clone, Debug)]
struct ChronicleUnstructuredRequestBuilder {
    encoder: ChronicleUnstructuredEncoder,
    compression: Compression,
}

impl RequestBuilder<(ChroniclePartitionKey, Vec<Event>)> for ChronicleUnstructuredRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = (ChroniclePartitionKey, Vec<Event>);
    type Encoder = ChronicleUnstructuredEncoder;
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
        input: (ChroniclePartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (partition_key, mut events) = input;
        let finalizers = events.take_finalizers();

        let builder = RequestMetadataBuilder::from_events(&events);
        (finalizers, builder, (partition_key, events))
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
            metadata
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct Label {
    key: String,
    value: String,
}

impl ChronicleUnstructuredRequestBuilder {
    fn new(config: &ChronicleUnstructuredConfig) -> crate::Result<Self> {
        let transformer = config.chronicle_common.encoding.transformer();
        let serializer = config.chronicle_common.encoding.config().build()?;
        let compression = Compression::from(config.chronicle_common.compression);
        let encoder = crate::codecs::Encoder::<()>::new(serializer);
        let encoder = ChronicleUnstructuredEncoder {
            customer_id: config.chronicle_common.customer_id.clone(),
            labels: config.labels.as_ref().map(|labs| {
                labs.iter()
                    .map(|(k, v)| Label {
                        key: k.to_string(),
                        value: v.to_string(),
                    })
                    .collect::<Vec<_>>()
            }),
            encoder,
            transformer,
        };
        Ok(Self {
            encoder,
            compression,
        })
    }
}

impl ChronicleUnstructuredConfig {
    pub fn build_sink(
        &self,
        client: HttpClient,
        base_url: String,
        creds: GcpAuthenticator,
    ) -> crate::Result<VectorSink> {
        use crate::sinks::util::service::ServiceBuilderExt;

        let request = self.chronicle_common.request.into_settings();
        let batch_settings = self.chronicle_common.batch.into_batcher_settings()?;
        let partitioner = self.partitioner()?;

        let svc = ServiceBuilder::new()
            .settings(request, GcsRetryLogic)
            .service(ChronicleService::new(client, base_url, creds));

        let request_settings = ChronicleUnstructuredRequestBuilder::new(self)?;

        let sink = ChronicleUnstructuredSink::new(svc, request_settings, partitioner, batch_settings, "http");

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn partitioner(&self) -> crate::Result<ChroniclePartitioner> {
        Ok(ChroniclePartitioner::new(
            self.log_type.clone(),
            self.namespace.clone(),
        ))
    }

}

pub struct ChronicleUnstructuredSink<Svc, RB> {
    service: Svc,
    request_builder: RB,
    partitioner: ChroniclePartitioner,
    batcher_settings: BatcherSettings,
    protocol: &'static str,
}

impl<Svc, RB> ChronicleUnstructuredSink<Svc, RB> {
    pub const fn new(
        service: Svc,
        request_builder: RB,
        partitioner: ChroniclePartitioner,
        batcher_settings: BatcherSettings,
        protocol: &'static str,
    ) -> Self {
        Self {
            service,
            request_builder,
            partitioner,
            batcher_settings,
            protocol,
        }
    }
}

impl<Svc, RB> ChronicleUnstructuredSink<Svc, RB>
where
    Svc: Service<RB::Request> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
    RB: RequestBuilder<(ChroniclePartitionKey, Vec<Event>)> + Send + Sync + 'static,
    RB::Error: fmt::Display + Send,
    RB::Request: Finalizable + MetaDescriptive + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let partitioner = self.partitioner;
        let settings = self.batcher_settings;

        let request_builder = self.request_builder;

        input
            .batched_partitioned(partitioner, || settings.as_byte_size_config())
            .filter_map(|(key, batch)| async move {
                // A `TemplateRenderingError` will have been emitted by `KeyPartitioner` if the key here is `None`,
                // thus no further `EventsDropped` event needs emitting at this stage.
                key.map(move |k| (k, batch))
            })
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
impl<Svc, RB> StreamSink<Event> for ChronicleUnstructuredSink<Svc, RB>
where
    Svc: Service<RB::Request> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
    RB: RequestBuilder<(ChroniclePartitionKey, Vec<Event>)> + Send + Sync + 'static,
    RB::Error: fmt::Display + Send,
    RB::Request: Finalizable + MetaDescriptive + Send,
{
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[cfg(all(test, feature = "chronicle-unstructured-integration-tests"))]
mod integration_tests {
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

    fn config(log_type: &str, auth_path: &str) -> ChronicleUnstructuredConfig {
        let address = std::env::var(ADDRESS_ENV_VAR).unwrap();
        let config = format!(
            indoc! { r#"
             endpoint = "{}"
             customer_id = "customer id"
             namespace = "namespace"
             credentials_path = "{}"
             log_type = "{}"
             encoding.codec = "text"
        "# },
            address, auth_path, log_type
        );

        let config: ChronicleUnstructuredConfig = toml::from_str(&config).unwrap();
        config
    }

    async fn config_build(
        log_type: &str,
        auth_path: &str,
    ) -> crate::Result<(VectorSink, crate::sinks::Healthcheck)> {
        let cx = SinkContext::default();
        config(log_type, auth_path).build(cx).await
    }

    #[tokio::test]
    async fn publish_events() {
        trace_init();

        let log_type = random_string(10);
        let (sink, healthcheck) =
            config_build(&log_type, "/home/vector/scripts/integration/gcp/auth.json")
                .await
                .expect("Building sink failed");

        healthcheck.await.expect("Health check failed");

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input, events) = random_events_with_stream(100, 100, Some(batch));
        run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let response = pull_messages(&log_type).await;
        let messages = response
            .into_iter()
            .map(|message| message.log_text)
            .collect::<Vec<_>>();
        assert_eq!(input.len(), messages.len());
        for i in 0..input.len() {
            let data = serde_json::to_value(&messages[i]).unwrap();
            let expected = serde_json::to_value(input[i].as_log().get("message").unwrap()).unwrap();
            assert_eq!(data, expected);
        }
    }

    #[tokio::test]
    async fn invalid_credentials() {
        trace_init();

        let log_type = random_string(10);
        // Test with an auth file that doesnt match the public key sent to the dummy chronicle server.
        let sink = config_build(
            &log_type,
            "/home/vector/scripts/integration/gcp/invalidauth.json",
        )
        .await;

        assert!(sink.is_err())
    }

    #[tokio::test]
    async fn publish_invalid_events() {
        trace_init();

        // The chronicle-emulator we are testing against is setup so a `log_type` of "INVALID"
        // will return a `400 BAD_REQUEST`.
        let log_type = "INVALID";
        let (sink, healthcheck) =
            config_build(log_type, "/home/vector/scripts/integration/gcp/auth.json")
                .await
                .expect("Building sink failed");

        healthcheck.await.expect("Health check failed");

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_input, events) = random_events_with_stream(100, 100, Some(batch));
        run_and_assert_sink_error(sink, events, &COMPONENT_ERROR_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
    }

    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct Log {
        customer_id: String,
        namespace: String,
        log_type: String,
        log_text: String,
        ts_rfc3339: String,
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

//! The `vector` source. See [VectorConfig].
use std::net::SocketAddr;

use chrono::Utc;
use futures::TryFutureExt;
use tonic::{Request, Response, Status};
use vector_lib::codecs::NativeDeserializerConfig;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{CountByteSize, InternalEventHandle as _};
use vector_lib::{
    config::LogNamespace,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    config::{
        DataType, GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig,
        SourceContext, SourceOutput,
    },
    internal_events::{EventsReceived, StreamClosedError},
    proto::vector as proto,
    serde::bool_or_struct,
    sources::{util::grpc::run_grpc_server, Source},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
    SourceSender,
};

/// Marker type for version two of the configuration for the `vector` source.
#[configurable_component]
#[derive(Clone, Debug)]
enum VectorConfigVersion {
    /// Marker value for version two.
    #[serde(rename = "2")]
    V2,
}

#[derive(Debug, Clone)]
struct Service {
    pipeline: SourceSender,
    acknowledgements: bool,
    log_namespace: LogNamespace,
}

#[tonic::async_trait]
impl proto::Service for Service {
    async fn push_events(
        &self,
        request: Request<proto::PushEventsRequest>,
    ) -> Result<Response<proto::PushEventsResponse>, Status> {
        let mut events: Vec<Event> = request
            .into_inner()
            .events
            .into_iter()
            .map(Event::from)
            .collect();

        let now = Utc::now();
        for event in &mut events {
            if let Event::Log(ref mut log) = event {
                self.log_namespace.insert_standard_vector_source_metadata(
                    log,
                    VectorConfig::NAME,
                    now,
                );
            }
        }

        let count = events.len();
        let byte_size = events.estimated_json_encoded_size_of();
        let events_received = register!(EventsReceived);
        events_received.emit(CountByteSize(count, byte_size));

        let receiver = BatchNotifier::maybe_apply_to(self.acknowledgements, &mut events);

        self.pipeline
            .clone()
            .send_batch(events)
            .map_err(|error| {
                let message = error.to_string();
                emit!(StreamClosedError { count });
                Status::unavailable(message)
            })
            .and_then(|_| handle_batch_status(receiver))
            .await?;

        Ok(Response::new(proto::PushEventsResponse {}))
    }

    // TODO: figure out a way to determine if the current Vector instance is "healthy".
    async fn health_check(
        &self,
        _: Request<proto::HealthCheckRequest>,
    ) -> Result<Response<proto::HealthCheckResponse>, Status> {
        let message = proto::HealthCheckResponse {
            status: proto::ServingStatus::Serving.into(),
        };

        Ok(Response::new(message))
    }
}

async fn handle_batch_status(receiver: Option<BatchStatusReceiver>) -> Result<(), Status> {
    let status = match receiver {
        Some(receiver) => receiver.await,
        None => BatchStatus::Delivered,
    };

    match status {
        BatchStatus::Errored => Err(Status::internal("Delivery error")),
        BatchStatus::Rejected => Err(Status::data_loss("Delivery failed")),
        BatchStatus::Delivered => Ok(()),
    }
}

/// Configuration for the `vector` source.
#[configurable_component(source("vector", "Collect observability data from a Vector instance."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    /// Version of the configuration.
    version: Option<VectorConfigVersion>,

    /// The socket address to listen for connections on.
    ///
    /// It _must_ include a port.
    pub address: SocketAddr,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    log_namespace: Option<bool>,
}

impl VectorConfig {
    /// Creates a `VectorConfig` with the given address.
    pub fn from_address(addr: SocketAddr) -> Self {
        Self {
            address: addr,
            ..Default::default()
        }
    }
}

impl Default for VectorConfig {
    fn default() -> Self {
        Self {
            version: None,
            address: "0.0.0.0:6000".parse().unwrap(),
            tls: None,
            acknowledgements: Default::default(),
            log_namespace: None,
        }
    }
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(VectorConfig::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "vector")]
impl SourceConfig for VectorConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let tls_settings = MaybeTlsSettings::from_config(&self.tls, true)?;
        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);
        let log_namespace = cx.log_namespace(self.log_namespace);

        let service = proto::Server::new(Service {
            pipeline: cx.out,
            acknowledgements,
            log_namespace,
        })
        .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
        // Tonic added a default of 4MB in 0.9. This replaces the old behavior.
        .max_decoding_message_size(usize::MAX);

        let source =
            run_grpc_server(self.address, tls_settings, service, cx.shutdown).map_err(|error| {
                error!(message = "Source future failed.", %error);
            });

        Ok(Box::pin(source))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let schema_definition = NativeDeserializerConfig
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        vec![SourceOutput::new_logs(DataType::all(), schema_definition)]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod test {
    use vector_lib::lookup::owned_value_path;
    use vector_lib::{config::LogNamespace, schema::Definition};
    use vrl::value::{kind::Collection, Kind};

    use crate::config::SourceConfig;

    use super::VectorConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = VectorConfig::default();

        let definitions = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        let expected_definition =
            Definition::new_with_default_metadata(Kind::any(), [LogNamespace::Vector])
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None,
                );

        assert_eq!(definitions, Some(expected_definition))
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = VectorConfig::default();

        let definitions = config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        let expected_definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        )
        .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None);

        assert_eq!(definitions, Some(expected_definition))
    }
}

#[cfg(feature = "sinks-vector")]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{SinkConfig as _, SinkContext},
        sinks::vector::VectorConfig as SinkConfig,
        test_util, SourceSender,
    };
    use vector_lib::assert_event_data_eq;
    use vector_lib::config::log_schema;

    async fn run_test(vector_source_config_str: &str, addr: SocketAddr) {
        let config = format!(r#"address = "{}""#, addr);
        let source: VectorConfig = toml::from_str(&config).unwrap();

        let (tx, rx) = SourceSender::new_test();
        let server = source
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(server);
        test_util::wait_for_tcp(addr).await;

        // Ideally, this would be a fully custom agent to send the data,
        // but the sink side already does such a test and this is good
        // to ensure interoperability.
        let sink: SinkConfig = toml::from_str(vector_source_config_str).unwrap();
        let cx = SinkContext::default();
        let (sink, _) = sink.build(cx).await.unwrap();

        let (mut events, stream) = test_util::random_events_with_stream(100, 100, None);
        sink.run(stream).await.unwrap();

        for event in &mut events {
            event.as_mut_log().insert(
                log_schema().source_type_key_target_path().unwrap(),
                "vector",
            );
        }

        let output = test_util::collect_ready(rx).await;
        assert_event_data_eq!(events, output);
    }

    #[tokio::test]
    async fn receive_message() {
        let addr = test_util::next_addr();

        let config = format!(r#"address = "{}""#, addr);
        run_test(&config, addr).await;
    }

    #[tokio::test]
    async fn receive_compressed_message() {
        let addr = test_util::next_addr();

        let config = format!(
            r#"address = "{}"
            compression=true"#,
            addr
        );
        run_test(&config, addr).await;
    }
}

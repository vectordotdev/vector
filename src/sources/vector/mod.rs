//! The `vector` source. See [VectorConfig].
use std::net::SocketAddr;

use vector_lib::{
    codecs::NativeDeserializerConfig, config::LogNamespace, configurable::configurable_component,
};
mod fetch;
mod receive;

use crate::{
    config::{
        DataType, GenerateConfig, Resource, SinkHealthcheckOptions, SourceAcknowledgementsConfig,
        SourceConfig, SourceContext, SourceOutput,
    },
    serde::bool_or_struct,
    sinks::util::TowerRequestConfig,
    sources::{Source, util::grpc::GrpcKeepaliveConfig},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

/// Marker type for version two of the configuration for the `vector` source.
#[configurable_component]
#[derive(Clone, Debug)]
enum VectorConfigVersion {
    /// Marker value for version two.
    #[serde(rename = "2")]
    V2,
}

#[configurable_component()]
#[configurable(description = "vector mode")]
#[derive(Clone, Debug)]
pub enum VectorMode {
    #[serde(rename = "fetch")]
    #[configurable(description = "fetch mode")]
    Fetch,
    #[serde(rename = "receive")]
    #[configurable(description = "receive mode")]
    Receive,
}

impl Default for VectorMode {
    fn default() -> Self {
        VectorMode::Receive
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

    /// The mode of vector source
    ///
    /// push or pull
    #[serde(default)]
    pub mode: VectorMode,

    /// Vector compression mode in pull mode
    #[serde(default)]
    pub compression: fetch::compression::VectorCompression,

    /// Vector request config in pull mode
    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    keepalive: GrpcKeepaliveConfig,

    /// Somrething something
    #[serde(default)]
    pub healthcheck: SinkHealthcheckOptions,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    pub log_namespace: Option<bool>,
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
            mode: VectorMode::Receive,
            compression: fetch::compression::VectorCompression::default(),
            request: TowerRequestConfig::default(),
            acknowledgements: Default::default(),
            keepalive: Default::default(),
            log_namespace: None,
            healthcheck: Default::default(),
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
        let tls_settings = MaybeTlsSettings::from_config(self.tls.as_ref(), true)?;

        match self.mode {
            VectorMode::Receive => receive::config_to_receive_source(self, tls_settings, cx).await,
            VectorMode::Fetch => fetch::config_to_fetch_source(self, &tls_settings, cx, 5),
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let schema_definition = NativeDeserializerConfig
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        vec![SourceOutput::new_maybe_logs(
            DataType::all_bits(),
            schema_definition,
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        match self.mode {
            VectorMode::Receive => {
                vec![Resource::tcp(self.address)]
            }
            VectorMode::Fetch => vec![],
        }
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod test {
    use vector_lib::{config::LogNamespace, lookup::owned_value_path, schema::Definition};
    use vrl::value::{Kind, kind::Collection};

    use super::VectorConfig;
    use crate::{
        SourceSender,
        config::{SourceConfig, SourceContext},
        test_util,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }

    #[test]
    fn config_keepalive() {
        let config: VectorConfig = toml::from_str(
            r#"
                address = "0.0.0.0:6000"

                [keepalive]
                max_connection_age_secs = 300
                max_connection_age_grace_secs = 30
            "#,
        )
        .unwrap();

        assert_eq!(config.keepalive.max_connection_age_secs, Some(300));
        assert_eq!(config.keepalive.max_connection_age_grace_secs, Some(30));
    }

    #[tokio::test]
    async fn max_connection_age_closes_idle_connection() {
        use tokio::{
            io::AsyncReadExt,
            net::TcpStream,
            time::{Duration, sleep, timeout},
        };

        let (_guard, addr) = test_util::addr::next_addr();
        let source_config = format!(
            r#"
                address = "{addr}"

                [keepalive]
                max_connection_age_secs = 1
            "#
        );
        let source: VectorConfig = toml::from_str(&source_config).unwrap();

        let (tx, _rx) = SourceSender::new_test();
        let server = source
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(server);
        test_util::wait_for_tcp(addr).await;

        let mut stream = TcpStream::connect(addr).await.unwrap();
        sleep(Duration::from_millis(1500)).await;

        let mut buf = [0; 32];
        let read = timeout(Duration::from_secs(2), async {
            loop {
                if stream.read(&mut buf).await.unwrap() == 0 {
                    break 0;
                }
            }
        })
        .await
        .unwrap();

        assert_eq!(read, 0);
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
    use vector_lib::{assert_event_data_eq, config::log_schema};

    use super::*;
    use crate::{
        SourceSender,
        config::{SinkConfig as _, SinkContext},
        sinks::vector::VectorConfig as SinkConfig,
        test_util,
    };

    async fn run_test(compression: Option<&str>) {
        let (_guard, addr) = test_util::addr::next_addr();

        let source_config = format!("address: \"{addr}\"");
        let source: VectorConfig = serde_yaml::from_str(&source_config).unwrap();

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
        let sink_config = match compression {
            Some(c) => indoc::formatdoc! {r#"
                address: "{addr}"
                compression: "{c}"
            "#},
            None => format!("address: \"{addr}\"\n"),
        };
        let sink: SinkConfig = serde_yaml::from_str(&sink_config).unwrap();
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
        run_test(None).await;
    }

    #[tokio::test]
    async fn receive_gzip_compressed_message() {
        run_test(Some("gzip")).await;
    }

    #[tokio::test]
    async fn receive_zstd_compressed_message() {
        run_test(Some("zstd")).await;
    }

    #[tokio::test]
    async fn custom_health_check_works() {
        use crate::proto::vector as proto;
        use tonic::transport::Channel;

        let (_guard, addr) = test_util::addr::next_addr();

        let config = format!("address: \"{addr}\"");
        let source: VectorConfig = serde_yaml::from_str(&config).unwrap();

        let (tx, _rx) = SourceSender::new_test();
        let server = source
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(server);
        test_util::wait_for_tcp(addr).await;

        // Test the custom Vector health check endpoint
        let endpoint = format!("http://{addr}");
        let channel = Channel::from_shared(endpoint)
            .unwrap()
            .connect()
            .await
            .unwrap();

        let mut client = proto::Client::new(channel);
        let response = client
            .health_check(proto::HealthCheckRequest {})
            .await
            .unwrap();

        assert_eq!(
            response.into_inner().status,
            proto::ServingStatus::Serving as i32
        );
    }

    #[tokio::test]
    async fn max_connection_age_allows_client_reconnect() {
        use crate::proto::vector as proto;
        use tokio::time::{Duration, sleep};
        use tonic::transport::Channel;

        use crate::sources::util::grpc::test_support::{
            max_connection_age_connection_observations,
            reset_max_connection_age_connection_observations,
        };

        let (_guard, addr) = test_util::addr::next_addr();

        let config = format!(
            r#"
                address = "{addr}"

                [keepalive]
                max_connection_age_secs = 1
            "#
        );
        let source: VectorConfig = toml::from_str(&config).unwrap();

        reset_max_connection_age_connection_observations();

        let (tx, _rx) = SourceSender::new_test();
        let server = source
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(server);
        test_util::wait_for_tcp(addr).await;

        let endpoint = format!("http://{addr}");
        let channel = Channel::from_shared(endpoint)
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = proto::Client::new(channel);

        let response = client
            .health_check(proto::HealthCheckRequest {})
            .await
            .unwrap();
        assert_eq!(
            response.into_inner().status,
            proto::ServingStatus::Serving as i32
        );
        let observations_before_expiry = max_connection_age_connection_observations();
        assert!(!observations_before_expiry.is_empty());

        sleep(Duration::from_millis(1500)).await;

        let response = client
            .health_check(proto::HealthCheckRequest {})
            .await
            .unwrap();
        assert_eq!(
            response.into_inner().status,
            proto::ServingStatus::Serving as i32
        );
        let observations = max_connection_age_connection_observations();
        assert!(
            observations.len() > observations_before_expiry.len(),
            "expected second RPC to reconnect after max connection age elapsed, got observations: {observations:?}",
        );
        assert!(observations.iter().any(|peer_addr| {
            !observations_before_expiry
                .iter()
                .any(|observed| observed == peer_addr)
        }));
    }

    #[tokio::test]
    async fn standard_grpc_health_check_works() {
        use tonic::transport::Channel;
        use tonic_health::pb::{HealthCheckRequest, health_client::HealthClient};

        let (_guard, addr) = test_util::addr::next_addr();

        let config = format!("address: \"{addr}\"");
        let source: VectorConfig = serde_yaml::from_str(&config).unwrap();

        let (tx, _rx) = SourceSender::new_test();
        let server = source
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(server);
        test_util::wait_for_tcp(addr).await;

        // Test the standard gRPC health check protocol
        let endpoint = format!("http://{addr}");
        let channel = Channel::from_shared(endpoint)
            .unwrap()
            .connect()
            .await
            .unwrap();

        let mut client = HealthClient::new(channel);

        // Check aggregate server health (empty service string)
        let response = client
            .check(HealthCheckRequest {
                service: String::new(),
            })
            .await
            .unwrap();

        use tonic_health::pb::health_check_response::ServingStatus;
        assert_eq!(response.into_inner().status, ServingStatus::Serving as i32);

        // Check the named Vector service health
        let response = client
            .check(HealthCheckRequest {
                service: "vector.Vector".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(response.into_inner().status, ServingStatus::Serving as i32);
    }
}

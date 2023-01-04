mod grpc;
#[cfg(all(test, feature = "envoy-als-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

use self::grpc::Service;
use crate::{
    config::{DataType, GenerateConfig, Output, Resource, SourceConfig, SourceContext},
    sources::{util::grpc::run_grpc_server, Source},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};
use envoy_proto::envoy::service::accesslog::v3::access_log_service_server::AccessLogServiceServer;
use futures::{FutureExt, TryFutureExt};
use std::net::SocketAddr;
use vector_common::internal_event::EventsReceived;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

/// Configuration for the `envoy_als` source.
#[configurable_component(source("envoy_als"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct EnvoyAlsConfig {
    #[configurable(derived)]
    grpc: GrpcConfig,
}

/// Configuration for the `envoy_als` gRPC server.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
struct GrpcConfig {
    /// The address to listen for connections on.
    ///
    /// It _must_ include a port.
    address: SocketAddr,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,
}

impl GenerateConfig for EnvoyAlsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            grpc: GrpcConfig {
                address: "0.0.0.0:9999".parse().unwrap(),
                tls: Default::default(),
            },
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SourceConfig for EnvoyAlsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let events_received = register!(EventsReceived);

        let grpc_tls_settings = MaybeTlsSettings::from_config(&self.grpc.tls, true)?;
        let grpc_service = AccessLogServiceServer::new(Service {
            events_received: events_received.clone(),
            pipeline: cx.out.clone(),
            shutdown: cx.shutdown.clone(),
        })
        .accept_compressed(tonic::codec::CompressionEncoding::Gzip);
        let grpc_source = run_grpc_server(
            self.grpc.address,
            grpc_tls_settings,
            grpc_service,
            cx.shutdown.clone(),
        )
        .map_err(|error| {
            error!(message = "Source future failed.", %error);
        });

        Ok(grpc_source.boxed())
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.grpc.address)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

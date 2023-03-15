//! This mod implements the `envoy_als` source.
//! The Envoy Access Log Service is a gRPC API that Envoy (An L7 load balancer) can
//! be configured to stream logs to. Thus the `envoy_als` source's scope is to accept
//! log streams from Envoy instances, and generate log events for each log.
//! For more information on the details of the ALS service, see
//! https://www.envoyproxy.io/docs/envoy/v1.24.1/api-v3/service/accesslog/v3/als.proto

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
use vector_common::internal_event::{BytesReceived, EventsReceived, Protocol};
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

/// Configuration for the `envoy_als` source.
#[configurable_component(source("envoy_als"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct EnvoyAlsConfig {
    #[configurable(derived)]
    grpc: GrpcConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

/// Configuration for the `envoy_als` gRPC server.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
struct GrpcConfig {
    /// The socket address to listen for connections on.
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
            log_namespace: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SourceConfig for EnvoyAlsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let events_received = register!(EventsReceived);
        let bytes_received = register!(BytesReceived::from(Protocol::from("protobuf")));

        let grpc_tls_settings = MaybeTlsSettings::from_config(&self.grpc.tls, true)?;
        let grpc_service = AccessLogServiceServer::new(Service {
            events_received,
            bytes_received,
            pipeline: cx.out,
            shutdown: cx.shutdown.clone(),
            log_namespace,
        })
        .accept_compressed(tonic::codec::CompressionEncoding::Gzip);
        let grpc_source = run_grpc_server(
            self.grpc.address,
            grpc_tls_settings,
            grpc_service,
            cx.shutdown,
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

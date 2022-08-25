#[cfg(all(test, feature = "opentelemetry-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

mod grpc;
mod http;
mod reply;
mod status;

use std::net::SocketAddr;

use futures::{future::join, FutureExt, TryFutureExt};

use opentelemetry_proto::proto::collector::logs::v1::logs_service_server::LogsServiceServer;
use vector_common::internal_event::{BytesReceived, Protocol};
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

use crate::{
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Output, Resource, SourceConfig,
        SourceContext,
    },
    serde::bool_or_struct,
    sources::{util::grpc::run_grpc_server, Source},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

use self::{
    grpc::Service,
    http::{build_warp_filter, run_http_server},
};

pub const LOGS: &str = "logs";

/// Configuration for the `opentelemetry` source.
#[configurable_component(source("opentelemetry"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryConfig {
    #[configurable(derived)]
    grpc: GrpcConfig,

    #[configurable(derived)]
    http: HttpConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
}

/// Configuration for the `opentelemetry` gRPC server.
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

/// Configuration for the `opentelemetry` HTTP server.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
struct HttpConfig {
    /// The address to listen for connections on.
    ///
    /// It _must_ include a port.
    address: SocketAddr,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,
}

impl GenerateConfig for OpentelemetryConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            grpc: GrpcConfig {
                address: "0.0.0.0:4317".parse().unwrap(),
                tls: Default::default(),
            },
            http: HttpConfig {
                address: "0.0.0.0:4318".parse().unwrap(),
                tls: Default::default(),
            },
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SourceConfig for OpentelemetryConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let acknowledgements = cx.do_acknowledgements(&self.acknowledgements);

        let grpc_tls_settings = MaybeTlsSettings::from_config(&self.grpc.tls, true)?;
        let grpc_service = LogsServiceServer::new(Service {
            pipeline: cx.out.clone(),
            acknowledgements,
        })
        .accept_gzip();
        let grpc_source = run_grpc_server(
            self.grpc.address,
            grpc_tls_settings,
            grpc_service,
            cx.shutdown.clone(),
        )
        .map_err(|error| {
            error!(message = "Source future failed.", %error);
        });

        let http_tls_settings = MaybeTlsSettings::from_config(&self.http.tls, true)?;
        let protocol = http_tls_settings.http_protocol_name();
        let bytes_received = register!(BytesReceived::from(Protocol::from(protocol)));
        let filters = build_warp_filter(acknowledgements, cx.out, bytes_received);
        let http_source =
            run_http_server(self.http.address, http_tls_settings, filters, cx.shutdown);

        Ok(join(grpc_source, http_source).map(|_| Ok(())).boxed())
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(DataType::Log).with_port(LOGS)]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![
            Resource::tcp(self.grpc.address),
            Resource::tcp(self.http.address),
        ]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

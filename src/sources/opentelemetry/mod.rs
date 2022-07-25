#[cfg(test)]
mod tests;

mod log;

use std::{collections::HashMap, net::SocketAddr};

use bytes::Bytes;
use futures::{future::join, FutureExt, TryFutureExt};

use http::HeaderMap;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

use crate::{
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Output, Resource, SourceConfig,
        SourceContext, SourceDescription,
    },
    event::Event,
    opentelemetry::LogService::logs_service_server::LogsServiceServer,
    serde::bool_or_struct,
    sources::{
        http::HttpMethod,
        util::{grpc::run_grpc_server, ErrorMessage, HttpSource},
        Source,
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

use self::log::Service;

pub const LOGS: &str = "logs";

/// Configuration for the `opentelemetry` source.
#[configurable_component(source)]
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

inventory::submit! {
    SourceDescription::new::<OpentelemetryConfig>("opentelemetry")
}

#[async_trait::async_trait]
#[typetag::serde(name = "opentelemetry")]
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

        let http_source = HttpSource::run(
            OpentelemetryHttpServer {},
            self.http.address,
            "/v1/logs",
            HttpMethod::Post,
            true,
            &self.http.tls,
            &None,
            cx,
            self.acknowledgements,
        )?;

        Ok(join(grpc_source, http_source).map(|_| Ok(())).boxed())
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(DataType::Log).with_port(LOGS)]
    }

    fn source_type(&self) -> &'static str {
        "opentelemetry"
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

#[derive(Clone)]
struct OpentelemetryHttpServer;

impl OpentelemetryHttpServer {
    fn decode_body(&self, _body: Bytes) -> Result<Vec<Event>, ErrorMessage> {
        todo!()
    }
}

impl HttpSource for OpentelemetryHttpServer {
    fn build_events(
        &self,
        body: Bytes,
        _header_map: HeaderMap,
        _query_parameters: HashMap<String, String>,
        _path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        let events = self.decode_body(body)?;
        Ok(events)
    }
}

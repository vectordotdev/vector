#[cfg(test)]
mod tests;

mod log;

use std::net::SocketAddr;

use futures::TryFutureExt;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

use crate::{
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Output, Resource, SourceConfig,
        SourceContext, SourceDescription,
    },
    opentelemetry::LogService::logs_service_server::LogsServiceServer,
    serde::bool_or_struct,
    sources::{util::grpc::run_grpc_server, Source},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

use self::log::Service;

pub const LOGS: &str = "logs";

/// Configuration for the `opentelemetry` source.
#[configurable_component(source)]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryConfig {
    /// The address to listen for connections on.
    ///
    /// It _must_ include a port.
    address: SocketAddr,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for OpentelemetryConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:4317".parse().unwrap(),
            tls: Default::default(),
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
        let tls_settings = MaybeTlsSettings::from_config(&self.tls, true)?;
        let acknowledgements = cx.do_acknowledgements(&self.acknowledgements);
        let service = LogsServiceServer::new(Service {
            pipeline: cx.out,
            acknowledgements,
        })
        .accept_gzip();
        let source =
            run_grpc_server(self.address, tls_settings, service, cx.shutdown).map_err(|error| {
                error!(message = "Source future failed.", %error);
            });

        Ok(Box::pin(source))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(DataType::Log).with_port(LOGS)]
    }

    fn source_type(&self) -> &'static str {
        "opentelemetry"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

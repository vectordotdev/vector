use crate::{
    config::SourceContext,
    config::{DataType, GenerateConfig, Resource},
    proto::vector as proto,
    shutdown::ShutdownSignalToken,
    sources::Source,
    Pipeline,
};

use futures::{FutureExt, SinkExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use tonic::{
    transport::{Certificate, Identity, Server, ServerTlsConfig},
    Request, Response, Status,
};
use vector_core::event::{BatchNotifier, BatchStatus, Event};

#[derive(Debug, Clone)]
pub struct Service {
    pipeline: Pipeline,
    acknowledgements: bool,
}

#[tonic::async_trait]
impl proto::Service for Service {
    async fn push_events(
        &self,
        request: Request<proto::EventRequest>,
    ) -> Result<Response<proto::EventResponse>, Status> {
        let (event, receiver) = request
            .into_inner()
            .message
            .map(|data| {
                let event = Event::from(data);
                if self.acknowledgements {
                    let (batch, receiver) = BatchNotifier::new_with_receiver();
                    let event = event.with_batch_notifier(&batch);
                    (event, Some(receiver))
                } else {
                    (event, None)
                }
            })
            .ok_or_else(|| Status::invalid_argument("missing event"))?;

        self.pipeline
            .clone()
            .send(event)
            .await
            .map_err(|err| Status::unavailable(err.to_string()))?;

        let status = if let Some(receiver) = receiver {
            receiver.await
        } else {
            BatchStatus::Delivered
        };
        match status {
            BatchStatus::Delivered => Ok(Response::new(proto::EventResponse {})),
            BatchStatus::Errored => Err(Status::internal("Delivery error")),
            BatchStatus::Failed => Err(Status::data_loss("Delivery failed")),
        }
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

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    pub address: SocketAddr,
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
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

fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:6000".parse().unwrap(),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            tls: None,
        })
        .unwrap()
    }
}

impl VectorConfig {
    pub(super) async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let source = run(self.address, self.tls.clone(), cx).map_err(|error| {
            error!(message = "Source future failed.", %error);
        });

        Ok(Box::pin(source))
    }

    pub(super) fn output_type(&self) -> DataType {
        DataType::Any
    }

    pub(super) fn source_type(&self) -> &'static str {
        "vector"
    }

    pub(super) fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }
}

async fn run(
    address: SocketAddr,
    tls: Option<GrpcTlsConfig>,
    cx: SourceContext,
) -> crate::Result<()> {
    let _span = crate::trace::current_span();

    let service = proto::Server::new(Service {
        pipeline: cx.out,
        acknowledgements: cx.acknowledgements,
    });
    let (tx, rx) = tokio::sync::oneshot::channel::<ShutdownSignalToken>();

    let mut server = match tls {
        Some(tls) => {
            let ca = Certificate::from_pem(tokio::fs::read(&tls.ca_file).await?);
            let crt = tokio::fs::read(&tls.crt_file).await?;
            let key = tokio::fs::read(&tls.key_file).await?;
            let identity = Identity::from_pem(crt, key);

            let tls_config = ServerTlsConfig::new().identity(identity).client_ca_root(ca);

            Server::builder().tls_config(tls_config)?
        }
        None => Server::builder(),
    };

    server
        .add_service(service)
        .serve_with_shutdown(address, cx.shutdown.map(|token| tx.send(token).unwrap()))
        .await?;

    drop(rx.await);

    Ok(())
}

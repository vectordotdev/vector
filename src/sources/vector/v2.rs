use crate::{
    config::SourceContext,
    config::{DataType, GenerateConfig, Resource},
    proto::vector as proto,
    shutdown::ShutdownSignalToken,
    sources::Source,
    Pipeline,
};

use futures::{FutureExt, SinkExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tonic::{
    transport::{Certificate, Identity, Server, ServerTlsConfig},
    Request, Response, Status,
};
use vector_core::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event};

#[derive(Debug, Clone)]
pub struct Service {
    pipeline: Pipeline,
    acknowledgements: bool,
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

        let receiver = self.acknowledgements.then(|| {
            let (batch, receiver) = BatchNotifier::new_with_receiver();
            for event in &mut events {
                event.add_batch_notifier(Arc::clone(&batch));
            }

            receiver
        });

        self.pipeline
            .clone()
            .send_all(&mut futures::stream::iter(events).map(Ok))
            .map_err(|err| Status::unavailable(err.to_string()))
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
        BatchStatus::Failed => Err(Status::data_loss("Delivery failed")),
        BatchStatus::Delivered => Ok(()),
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

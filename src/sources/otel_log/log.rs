use std::{
    net::SocketAddr,
};
use futures::{FutureExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tonic::{
    transport::{server::Connected, Server},
    Request, Response, Status,
};
use tracing::{Instrument, Span};
use vector_core::{
    event::{
        BatchNotifier, 
        BatchStatus, 
        BatchStatusReceiver, 
        Event, 
    },
    ByteSizeOf,
};
use opentelemetry::{
    collector::logs::v1::{
        ExportLogsServiceRequest,
        ExportLogsServiceResponse,
        logs_service_server::{
            LogsService,
            LogsServiceServer,
        }
    },
    logs::v1::{ResourceLogs,LogRecord,},
    resource::v1::Resource as OtelResource,
};
use crate::{
    config::{
        GenerateConfig, 
        AcknowledgementsConfig,
        SourceContext,
        Output,
        DataType,
        Resource,
        SourceDescription,
    },
    internal_events::{EventsReceived, StreamClosedError, TcpBytesReceived},
    serde::bool_or_struct,
    shutdown::ShutdownSignalToken,
    sources::{util::AfterReadExt as _, Source},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
    SourceSender,
};

pub mod opentelemetry {
    pub mod collector {
        pub mod logs {
            pub mod v1 {
                tonic::include_proto!("opentelemetry.proto.collector.logs.v1");
            }
        }
    }
    pub mod common {
        pub mod v1 {
            tonic::include_proto!("opentelemetry.proto.common.v1");
        }
    }
    pub mod resource {
        pub mod v1 {
            tonic::include_proto!("opentelemetry.proto.resource.v1");
        }
    }
    pub mod logs {
        pub mod v1 {
            tonic::include_proto!("opentelemetry.proto.logs.v1");
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryLogConfig {
    address: SocketAddr,
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for OpentelemetryLogConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:6000".parse().unwrap(),
            tls: None,
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

impl OpentelemetryLogConfig {
    pub(super) async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let tls_settings = MaybeTlsSettings::from_config(&self.tls, true)?;
        let acknowledgements = cx.do_acknowledgements(&self.acknowledgements);

        let source = run(self.address, tls_settings, cx, acknowledgements).map_err(|error| {
            error!(message = "Source future failed.", %error);
        });

        Ok(Box::pin(source))
    }

    pub(super) fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    pub(super) const fn source_type(&self) -> &'static str {
        "otel_log"
    }

    pub(super) fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }
}

inventory::submit! {
    SourceDescription::new::<OpentelemetryLogConfig>("otel_log")
}


#[derive(Debug, Clone)]
pub struct Service {
    pipeline: SourceSender,
    acknowledgements: bool,
}

#[tonic::async_trait]
impl LogsService for Service {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        let mut events: Vec<Event> = request
            .into_inner()
            .resource_logs
            .into_iter()
            .flat_map(|v| v.into_iter())
            .collect();

        let count = events.len();
        let byte_size = events.size_of();

        emit!(EventsReceived { count, byte_size });

        let receiver = BatchNotifier::maybe_apply_to_events(self.acknowledgements, &mut events);

        self.pipeline
            .clone()
            .send_batch(events)
            .map_err(|error| {
                let message = error.to_string();
                emit!(StreamClosedError { error, count });
                Status::unavailable(message)
            })
            .and_then(|_| handle_batch_status(receiver))
            .await?;
        Ok(Response::new(ExportLogsServiceResponse{}))
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

#[derive(Serialize, Deserialize)]
struct ScopeLog<> {
    resource: Option<OtelResource>,
    log_record: LogRecord,
}

impl IntoIterator for ResourceLogs {
    type Item = Event;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let resource = self.resource.clone();
        let mut logs: Vec<Event> = vec![];
        for scope_log in self.scope_logs{
            for log_record in scope_log.log_records{
                let sl = ScopeLog{
                    resource: resource.clone(),
                    log_record,
                };
                logs.push(serde_json::to_value(sl).unwrap().try_into().unwrap());
            }
        }
        for library_log in self.instrumentation_library_logs{
            for log_record in library_log.log_records{
                let sl = ScopeLog{
                    resource: resource.clone(),
                    log_record,
                };
                logs.push(serde_json::to_value(sl).unwrap().try_into().unwrap());
            }
        }
        logs.into_iter()
    }
}

async fn run(
    address: SocketAddr,
    tls_settings: MaybeTlsSettings,
    cx: SourceContext,
    acknowledgements: bool,
) -> crate::Result<()> {
    let span = Span::current();

    let service = LogsServiceServer::new(Service {
        pipeline: cx.out,
        acknowledgements,
    })
    .accept_gzip();

    let (tx, rx) = tokio::sync::oneshot::channel::<ShutdownSignalToken>();

    let listener = tls_settings.bind(&address).await?;
    let stream = listener.accept_stream().map(|result| {
        result.map(|socket| {
            let peer_addr = socket.connect_info().remote_addr;
            socket.after_read(move |byte_size| {
                emit!(TcpBytesReceived {
                    byte_size,
                    peer_addr,
                })
            })
        })
    });

    Server::builder()
        .trace_fn(move |_| span.clone())
        .add_service(service)
        .serve_with_incoming_shutdown(stream, cx.shutdown.map(|token| tx.send(token).unwrap()))
        .in_current_span()
        .await?;

    drop(rx.await);

    Ok(())
}
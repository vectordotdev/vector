use std::{
    net::SocketAddr,
};
use std::collections::BTreeMap;
use futures::{FutureExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use tonic::{
    transport::{server::Connected, Server},
    Request, Response, Status,
};
use bytes::Bytes;
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
    logs::v1::{ResourceLogs, LogRecord, ScopeLogs, InstrumentationLibraryLogs,},
    resource::v1::Resource as OtelResource,
    common::v1::{
        InstrumentationLibrary,
        InstrumentationScope,
        any_value::Value as PBValue,
        KeyValue,
    },
};
use value::Value;
use vector_core::config::log_schema;
use vector_core::event::LogEvent;
use crate::{
    config::{
        GenerateConfig,
        SourceConfig,
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
            address: "0.0.0.0:6788".parse().unwrap(),
            tls: None,
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "otel_log")]
impl SourceConfig for OpentelemetryLogConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let tls_settings = MaybeTlsSettings::from_config(&self.tls, true)?;
        let acknowledgements = cx.do_acknowledgements(&self.acknowledgements);

        let source = run(self.address, tls_settings, cx, acknowledgements).map_err(|error| {
            error!(message = "Source future failed.", %error);
        });

        Ok(Box::pin(source))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "otel_log"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
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
struct LogEntry<> {
    resource: Option<OtelResource>,
    log_record: LogRecord,
}

impl From<InstrumentationLibraryLogs> for ScopeLogs {
    fn from(v: InstrumentationLibraryLogs) -> Self {
        Self {
            scope: v.instrumentation_library.map(|v| InstrumentationScope{
                name: v.name,
                version: v.version,
            }),
            log_records: v.log_records,
            schema_url: v.schema_url,
        }
    }
}

impl IntoIterator for ResourceLogs {
    type Item = Event;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        let resource = self.resource;
        // convert instrumentation_library_logs(deprecated) into scope_logs
        let scope_logs: Vec<ScopeLogs> = if !self.scope_logs.is_empty() {
            self.scope_logs
        } else {
            self.instrumentation_library_logs
                .into_iter().map(ScopeLogs::from)
                .collect()
        };

        scope_logs.into_iter()
            .map(|scope_log| scope_log.log_records).flatten()
            .map(|log_record| ResourceLog{
                resource: resource.clone(),
                log_record,
            }.into())
            .collect::<Vec<Event>>().into_iter()
    }
}

impl From<PBValue> for Value {
    fn from(av: PBValue) -> Self {
        match av {
            PBValue::StringValue(v) => Value::Bytes(Bytes::from(v)),
            PBValue::BoolValue(v) => Value::Boolean(v),
            PBValue::IntValue(v) => Value::Integer(v),
            PBValue::DoubleValue(v) => Value::Float(v.into()),
            PBValue::BytesValue(v) => Value::Bytes(Bytes::from(v)),
            PBValue::ArrayValue(arr) => {
                Value::Array(arr.values.into_iter()
                    .filter(|o| o.is_some())
                    .map(|v| v.into())
                    .collect::<Vec<Value>>())
            },
            PBValue::KvlistValue(arr) => {
                Value::Object(
                    arr.values.into_iter()
                    .filter_map(|kv| kv.value.map(|v| (kv.key, v)))
                    .fold(BTreeMap::default(), |mut acc, (k, v)| {
                        acc.insert(k, v.into::<Value>());
                        acc
                    })
                )
            }
        }
    }
}

struct ResourceLog {
    resource: OtelResource,
    log_record: LogRecord,
}

fn kvlist_2_value(arr: Vec<KeyValue>) -> Value {
    arr.into_iter()
        .filter_map(|kv| kv.value.map(|v| (kv.key, v)))
        .fold(BTreeMap::default(), |mut acc, (k, v)| {
            acc.insert(k, v.into::<Value>());
            acc
        })
        .into()
}

impl From<ResourceLog> for Event {
    fn from(rl: ResourceLog) -> Self {
        let mut le = LogEvent::default();
        // resource
        if rl.resource.is_some() {
            le.insert("resources",kvlist_2_value(rl.resource.attributes));
        }
        le.insert("attributes", kvlist_2_value(rl.log_record.attributes));
        rl.log_record.body.map(|v| {
            le.insert(log_schema().message_key(), v);
        });
        le.insert(log_schema().timestamp_key(), rl.log_record.time_unix_nano as i64);
        le.insert("trace_id", Value::Bytes(Bytes::from(rl.log_record.trace_id)));
        le.insert("span_id", Value::Bytes(Bytes::from(rl.log_record.span_id)));
        le.insert("severity_text", rl.log_record.severity_text);
        le.insert("severity_number", rl.log_record.severity_number as i64);
        le.into()
    }
}

fn log_entry_into_event(resource: Option<OtelResource>, log_record: LogRecord) -> Event {
    let entry = LogEntry{
        resource,
        log_record,
    };
    serde_json::to_value(entry).unwrap().try_into().unwrap()
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
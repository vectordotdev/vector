use std::net::SocketAddr;
use std::collections::BTreeMap;
use futures::TryFutureExt;
use serde::{Deserialize, Serialize};
use tonic::{Request, Response, Status};
use bytes::Bytes;
use ordered_float::NotNan;
use vector_core::{
    event::{
        BatchNotifier, 
        BatchStatus, 
        BatchStatusReceiver, 
        Event, 
    },
    ByteSizeOf,
};
use otel_proto::{
    Resource as OtelResource,
    Common::{
        InstrumentationScope,
        any_value::Value as PBValue,
        KeyValue,
    },
    Logs::{
        ResourceLogs,
        LogRecord,
        ScopeLogs,
        InstrumentationLibraryLogs,
    },
    LogService::{
        ExportLogsServiceRequest,
        ExportLogsServiceResponse,
        logs_service_server::{
            LogsService,
            LogsServiceServer,
        },
    },
};
use value::Value;
use vector_core::{
    config::log_schema,
    event::LogEvent,
};
use crate::{
    config::{
        GenerateConfig,
        SourceConfig,
        AcknowledgementsConfig,
        SourceContext,
        Output,
        DataType,
        Resource,
    },
    internal_events::{EventsReceived, StreamClosedError},
    serde::bool_or_struct,
    sources::{
        util::grpc::run_grpc_server,
        Source},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
    SourceSender,
};

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
        let service = LogsServiceServer::new(Service {
            pipeline: cx.out,
            acknowledgements,
        }).accept_gzip();
        let source = run_grpc_server(self.address, tls_settings, service, cx.shutdown).map_err(|error| {
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
            PBValue::DoubleValue(v) => Value::Float(NotNan::new(v).unwrap()),
            PBValue::BytesValue(v) => Value::Bytes(Bytes::from(v)),
            PBValue::ArrayValue(arr) => {
                Value::Array(arr.values.into_iter()
                    .filter_map(|av| av.value)
                    .map(|v| v.into())
                    .collect::<Vec<Value>>())
            },
            PBValue::KvlistValue(arr) => {
                kvlist_2_value(arr.values)
            }
        }
    }
}

struct ResourceLog {
    resource: Option<OtelResource>,
    log_record: LogRecord,
}

fn kvlist_2_value(arr: Vec<KeyValue>) -> Value {
    Value::Object(arr.into_iter()
        .filter_map(|kv| kv.value.map(|av| (kv.key, av)))
        .fold(BTreeMap::default(), |mut acc, (k, av)| {
            av.value.map(|v| {
                acc.insert(k, v.into());
            });
            acc
        }))
}

impl From<ResourceLog> for Event {
    fn from(rl: ResourceLog) -> Self {
        let mut le = LogEvent::default();
        // resource
        rl.resource.map(|resource| {
            le.insert("resources",kvlist_2_value(resource.attributes));
        });
        le.insert("attributes", kvlist_2_value(rl.log_record.attributes));
        rl.log_record.body.and_then(|av| av.value).map(|v| {
            le.insert(log_schema().message_key(), v);
        });
        le.insert(log_schema().timestamp_key(), rl.log_record.time_unix_nano as i64);
        le.insert("trace_id", Value::Bytes(Bytes::from(hex::encode(rl.log_record.trace_id))));
        le.insert("span_id", Value::Bytes(Bytes::from(hex::encode(rl.log_record.span_id))));
        le.insert("severity_text", rl.log_record.severity_text);
        le.insert("severity_number", rl.log_record.severity_number as i64);
        le.into()
    }
}
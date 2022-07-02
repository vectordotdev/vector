use crate::{
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Output, Resource, SourceConfig,
        SourceContext,
    },
    internal_events::{EventsReceived, StreamClosedError},
    serde::bool_or_struct,
    sources::{util::grpc::run_grpc_server, Source},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
    SourceSender,
};
use futures::TryFutureExt;
use otel_proto::LogService::{
    logs_service_server::{LogsService, LogsServiceServer},
    ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tonic::{Request, Response, Status};
use vector_core::{
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    ByteSizeOf,
};

pub const LOGS: &str = "logs";

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryConfig {
    address: SocketAddr,
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
    /// If this setting is set to `true` logs, metrics and traces will be sent to different outputs.
    ///
    /// For a source component named `otel` the received logs, metrics, and traces can then be accessed by specifying
    /// `otel.logs`, `otel.metrics`, and `otel.traces`, respectively, as the input to another component.
    #[serde(default = "crate::serde::default_false")]
    multiple_outputs: bool,
}

impl GenerateConfig for OpentelemetryConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:6788".parse().unwrap(),
            tls: None,
            acknowledgements: Default::default(),
            multiple_outputs: false,
        })
        .unwrap()
    }
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

    fn outputs(&self) -> Vec<Output> {
        if self.multiple_outputs {
            vec![Output::default(DataType::Log).with_port(LOGS)]
        } else {
            vec![Output::default(DataType::Log)]
        }
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

        let receiver = BatchNotifier::maybe_apply_to(self.acknowledgements, &mut events);

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
        Ok(Response::new(ExportLogsServiceResponse {}))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        test_util::{
            self,
            components::{assert_source_compliance, SOURCE_TAGS},
        },
        SourceSender,
    };
    use otel_proto::{
        Common::{any_value, AnyValue, KeyValue},
        LogService::logs_service_client::LogsServiceClient,
        Logs::{LogRecord, ResourceLogs, ScopeLogs},
        Resource as OtelResource,
    };

    #[tokio::test]
    #[allow(deprecated)]
    async fn receive_message() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let addr = test_util::next_addr();
            let config = format!(r#"address = "{}""#, addr);
            let source: OpentelemetryConfig = toml::from_str(&config).unwrap();

            let (tx, rx) = SourceSender::new_test();
            let server = source
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap();
            tokio::spawn(server);
            test_util::wait_for_tcp(addr).await;

            // send request via grpc client
            let mut client = LogsServiceClient::connect(format!("http://{}", addr))
                .await
                .unwrap();
            let req = Request::new(ExportLogsServiceRequest {
                resource_logs: vec![ResourceLogs {
                    resource: Some(OtelResource {
                        attributes: vec![KeyValue {
                            key: "res_key".into(),
                            value: Some(AnyValue {
                                value: Some(any_value::Value::StringValue("res_val".into())),
                            }),
                        }],
                        dropped_attributes_count: 0,
                    }),
                    scope_logs: vec![ScopeLogs {
                        scope: None,
                        log_records: vec![LogRecord {
                            time_unix_nano: 1,
                            observed_time_unix_nano: 2,
                            severity_number: 9,
                            severity_text: "info".into(),
                            body: Some(AnyValue {
                                value: Some(any_value::Value::StringValue("log body".into())),
                            }),
                            attributes: vec![KeyValue {
                                key: "attr_key".into(),
                                value: Some(AnyValue {
                                    value: Some(any_value::Value::StringValue("attr_val".into())),
                                }),
                            }],
                            dropped_attributes_count: 3,
                            flags: 4,
                            // opentelemetry sdk will hex::decode the given trace_id and span_id
                            trace_id: str_into_hex_bytes("4ac52aadf321c2e531db005df08792f5"),
                            span_id: str_into_hex_bytes("0b9e4bda2a55530d"),
                        }],
                        schema_url: "v1".into(),
                    }],
                    instrumentation_library_logs: vec![],
                    schema_url: "v1".into(),
                }],
            });
            let _ = client.export(req).await;
            let mut output = test_util::collect_ready(rx).await;
            // we just send one, so only one output
            assert_eq!(output.len(), 1);
            let actual_event = output.pop().unwrap();
            let expect = r#"
            {
            "attributes": {"attr_key": "attr_val"},
            "resources": {"res_key": "res_val"},
            "message": "log body",
            "trace_id": "4ac52aadf321c2e531db005df08792f5",
            "span_id": "0b9e4bda2a55530d",
            "severity_number": 9,
            "severity_text": "info",
            "flags": 4,
            "timestamp": 1,
            "observed_time_unix_nano": 2,
            "dropped_attributes_count": 3
            }
            "#;
            let expect_json: serde_json::Value = serde_json::from_str(expect).unwrap();
            let expect_event = Event::try_from(expect_json).unwrap();
            assert_eq!(actual_event, expect_event);
        })
        .await;
    }
    fn str_into_hex_bytes(s: &str) -> Vec<u8> {
        // unwrap is okay in test
        hex::decode(s).unwrap()
    }
}

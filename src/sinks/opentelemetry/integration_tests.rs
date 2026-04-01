use futures::stream;
use prost::Message as _;
use vector_lib::{
    codecs::decoding::format::{Deserializer as _, OtlpDeserializer},
    config::LogNamespace,
    opentelemetry::proto::{
        collector::logs::v1::ExportLogsServiceRequest,
        logs::v1::{LogRecord, ResourceLogs, ScopeLogs},
        resource::v1::Resource,
    },
};

use super::OpenTelemetryConfig;
use crate::{
    config::{SinkConfig as _, SinkContext},
    test_util::{
        components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        wait_for_tcp,
    },
};

fn sink_grpc_address() -> String {
    std::env::var("OTEL_GRPC_SINK_ADDRESS")
        .unwrap_or_else(|_| "opentelemetry-collector:4317".to_owned())
}

fn otlp_log_event_with_host(host: &str) -> vector_lib::event::Event {
    let mut event = otlp_log_event();
    if let vector_lib::event::Event::Log(ref mut log) = event {
        log.insert("host", host.to_owned());
    }
    event
}

fn otlp_log_event() -> vector_lib::event::Event {
    let req = ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![],
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: None,
                log_records: vec![LogRecord {
                    severity_text: "INFO".to_string(),
                    body: Some(vector_lib::opentelemetry::proto::common::v1::AnyValue {
                        value: Some(
                            vector_lib::opentelemetry::proto::common::v1::any_value::Value::StringValue(
                                "integration test log message".to_string(),
                            ),
                        ),
                    }),
                    ..Default::default()
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    };

    let bytes = bytes::Bytes::from(req.encode_to_vec());
    let mut events = OtlpDeserializer::default()
        .parse(bytes, LogNamespace::Legacy)
        .expect("failed to deserialize OTLP log event");
    events.remove(0)
}

#[tokio::test]
async fn delivers_logs_via_grpc() {
    let address = sink_grpc_address();
    wait_for_tcp(address.clone()).await;

    let config: OpenTelemetryConfig = toml::from_str(&format!(
        r#"
            protocol = "grpc"
            uri = "http://{address}"
        "#
    ))
    .unwrap();

    let (sink, healthcheck) = config.build(SinkContext::default()).await.unwrap();
    healthcheck.await.expect("gRPC healthcheck failed");

    let events = vec![otlp_log_event()];
    // The gRPC sink emits EndpointBytesSent with the same "endpoint" and "protocol" tags as
    // HTTP sinks, so HTTP_SINK_TAGS is the correct compliance set here.
    run_and_assert_sink_compliance(sink, stream::iter(events), &HTTP_SINK_TAGS).await;
}

#[tokio::test]
async fn delivers_logs_via_grpc_template_uri() {
    let host = sink_grpc_address();
    wait_for_tcp(host.clone()).await;

    let config: OpenTelemetryConfig = toml::from_str(
        r#"
        protocol = "grpc"
        uri = "http://{{ host }}:4317"
    "#,
    )
    .unwrap();

    let (sink, healthcheck) = config.build(SinkContext::default()).await.unwrap();
    // The URI is a dynamic template so there is no static healthcheck URI; the healthcheck
    // skips gracefully and returns Ok(()) rather than failing.
    healthcheck.await.expect("gRPC healthcheck failed unexpectedly for dynamic URI");

    // The event carries `host` so the template renders to the collector address.
    let events = vec![otlp_log_event_with_host(host.split(':').next().unwrap())];
    run_and_assert_sink_compliance(sink, stream::iter(events), &HTTP_SINK_TAGS).await;
}

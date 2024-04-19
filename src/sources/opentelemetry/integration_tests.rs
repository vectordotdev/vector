use std::time::Duration;

use itertools::Itertools;
use serde_json::json;

use super::{LOGS, TRACES};
use crate::{
    config::{log_schema, SourceConfig, SourceContext},
    event::EventStatus,
    test_util::{
        collect_n,
        components::{assert_source_compliance, SOURCE_TAGS},
        retry_until, wait_for_tcp,
    },
};
use prost::Message;

use vector_lib::opentelemetry::proto::{
    collector::trace::v1::ExportTraceServiceRequest,
    trace::v1::{ResourceSpans, ScopeSpans, Span},
};

use super::{tests::new_source, GrpcConfig, HttpConfig, OpentelemetryConfig};

fn otel_health_url() -> String {
    std::env::var("OTEL_HEALTH_URL").unwrap_or_else(|_| "http://0.0.0.0:13133".to_owned())
}

fn otel_otlp_url() -> String {
    std::env::var("OTEL_OTLPHTTP_URL").unwrap_or_else(|_| "http://0.0.0.0:9876".to_owned())
}

fn source_grpc_address() -> String {
    std::env::var("SOURCE_GRPC_ADDRESS").unwrap_or_else(|_| "0.0.0.0:4317".to_owned())
}

fn source_http_address() -> String {
    std::env::var("SOURCE_HTTP_ADDRESS").unwrap_or_else(|_| "0.0.0.0:4318".to_owned())
}

#[tokio::test]
async fn receive_logs_legacy_namespace() {
    assert_source_compliance(&SOURCE_TAGS, async {
        wait_ready(otel_health_url()).await;

        let config = OpentelemetryConfig {
            grpc: GrpcConfig {
                address: source_grpc_address().parse().unwrap(),
                tls: Default::default(),
            },
            http: HttpConfig {
                address: source_http_address().parse().unwrap(),
                tls: Default::default(),
                keepalive: Default::default(),
            },
            acknowledgements: Default::default(),
            log_namespace: Default::default(),
        };

        let (sender, logs_output, _) = new_source(EventStatus::Delivered, LOGS.to_string());
        let server = config
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap();
        tokio::spawn(server);
        wait_for_tcp(source_grpc_address()).await;
        wait_for_tcp(source_http_address()).await;

        let client = reqwest::Client::new();
        let _res = client
            .post(format!("{}/v1/logs", otel_otlp_url()))
            .json(&json!(
                {
                  "resource_logs": [
                    {
                      "scope_logs": [
                        {
                          "log_records": [
                            {
                              "severity_text": "INFO",
                              "body": {
                                "string_value": "foobar"
                              }
                            }
                          ]
                        }
                      ]
                    }
                  ]
                }
            ))
            .send()
            .await
            .expect("Failed to send log to Opentelemetry Collector.");

        // The Opentelemetry Collector is configured to send to both the gRPC and HTTP endpoints
        // so we should expect to collect two events from the single log sent.
        let events = collect_n(logs_output, 2).await;
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0].as_log()[log_schema().message_key().unwrap().to_string()],
            events[1].as_log()[log_schema().message_key().unwrap().to_string()]
        );
    })
    .await;
}

#[tokio::test]
async fn receive_trace() {
    // generate a trace request
    let req = ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: None,
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![Span {
                    trace_id: (1..17).collect_vec(),      //trace_id [u8;16]
                    span_id: (1..9).collect_vec(),        // span_id [u8;8]
                    parent_span_id: (1..9).collect_vec(), // parent_span_id [u8;8]
                    name: "span".to_string(),
                    kind: 1,
                    start_time_unix_nano: 1713525203000000000,
                    end_time_unix_nano: 1713525205000000000,
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    events: vec![],
                    dropped_events_count: 0,
                    links: vec![],
                    dropped_links_count: 0,
                    status: None,
                    trace_state: "".to_string(),
                }],
                schema_url: "".to_string(),
            }],
            schema_url: "".to_string(),
        }],
    };
    let body = req.encode_to_vec();

    assert_source_compliance(&SOURCE_TAGS, async {
        wait_ready(otel_health_url()).await;

        let config = OpentelemetryConfig {
            grpc: GrpcConfig {
                address: source_grpc_address().parse().unwrap(),
                tls: Default::default(),
            },
            http: HttpConfig {
                address: source_http_address().parse().unwrap(),
                tls: Default::default(),
                keepalive: Default::default(),
            },
            acknowledgements: Default::default(),
            log_namespace: Default::default(),
        };

        let (sender, trace_output, _) = new_source(EventStatus::Delivered, TRACES.to_string());
        let server = config
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap();
        tokio::spawn(server);
        wait_for_tcp(source_grpc_address()).await;
        wait_for_tcp(source_http_address()).await;

        let client = reqwest::Client::new();
        let _res = client
            .post(format!("{}/v1/traces", otel_otlp_url()))
            .header(reqwest::header::CONTENT_TYPE, "application/x-protobuf")
            .body(body)
            .send()
            .await
            .expect("Failed to send traces to Opentelemetry Collector.");

        // The Opentelemetry Collector is configured to send to both the gRPC and HTTP endpoints
        // so we should expect to collect two events from the single log sent.
        let events = collect_n(trace_output, 2).await;
        assert_eq!(events.len(), 2);
    })
    .await;
}

async fn wait_ready(address: String) {
    retry_until(
        || async {
            reqwest::get(address.clone())
                .await
                .map_err(|err| err.to_string())
                .and_then(|res| {
                    if res.status().is_success() {
                        Ok(())
                    } else {
                        Err("Not ready yet...".into())
                    }
                })
        },
        Duration::from_secs(1),
        Duration::from_secs(30),
    )
    .await;
}

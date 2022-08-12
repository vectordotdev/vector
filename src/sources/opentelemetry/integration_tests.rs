use std::time::Duration;

use futures::Stream;
use futures_util::StreamExt;
use serde_json::json;

use crate::{
    config::{log_schema, SourceConfig, SourceContext},
    event::{into_event_stream, Event, EventStatus},
    test_util::{
        collect_n,
        components::{assert_source_compliance, SOURCE_TAGS},
        retry_until, wait_for_tcp,
    },
    SourceSender,
};

use super::{GrpcConfig, HttpConfig, OpentelemetryConfig, LOGS};

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
async fn receive_logs() {
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
            },
            acknowledgements: Default::default(),
        };

        let (sender, logs_output, _) = new_source(EventStatus::Delivered);
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
            events[0].as_log()[log_schema().message_key()],
            events[1].as_log()[log_schema().message_key()]
        );
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

fn new_source(
    status: EventStatus,
) -> (
    SourceSender,
    impl Stream<Item = Event>,
    impl Stream<Item = Event>,
) {
    let (mut sender, recv) = SourceSender::new_test_finalize(status);
    let logs_output = sender
        .add_outputs(status, LOGS.to_string())
        .flat_map(into_event_stream);
    (sender, logs_output, recv)
}

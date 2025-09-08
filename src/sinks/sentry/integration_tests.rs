#![cfg(test)]

use std::time::Duration;

use futures::stream;
use sentry::{Envelope, protocol::EnvelopeItem};
use tokio::time::timeout;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path_regex},
};

use super::config::SentryConfig;
use vector_lib::codecs::encoding::format::JsonSerializerOptions;
use vector_lib::codecs::{JsonSerializerConfig, MetricTagValues};

use crate::{
    codecs::{EncodingConfig, Transformer},
    config::{AcknowledgementsConfig, SinkConfig, SinkContext},
    event::{Event, LogEvent},
    sinks::util::BatchConfig,
    sinks::util::http::RequestConfig,
    test_util::{
        components::{SINK_TAGS, run_and_assert_sink_compliance},
        trace_init,
    },
};

async fn sentry_mock_server() -> MockServer {
    let mock_server = MockServer::start().await;

    // Mock the Sentry envelope endpoint
    Mock::given(method("POST"))
        .and(path_regex(r"/api/[0-9]+/envelope/"))
        .and(header("Content-Type", "application/x-sentry-envelope"))
        .and(header("User-Agent", "sentry.vector/0.1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "test-event-id"
        })))
        .mount(&mock_server)
        .await;

    mock_server
}

#[tokio::test]
async fn sentry_sink_handles_log_events() {
    trace_init();

    let mock_server = sentry_mock_server().await;
    let dsn = format!(
        "http://test-key@{}/123",
        mock_server.uri().replace("http://", "")
    );

    let config = SentryConfig {
        dsn: dsn.clone(),
        batch: BatchConfig::default(),
        request: RequestConfig::default(),
        tls: None,
        encoding: EncodingConfig::new(
            JsonSerializerConfig::new(MetricTagValues::Full, JsonSerializerOptions::default())
                .into(),
            Transformer::default(),
        ),
        acknowledgements: AcknowledgementsConfig::default(),
    };

    let cx = SinkContext::default();
    let (sink, _) = config.build(cx).await.unwrap();

    let mut log_event = LogEvent::from("test message");
    log_event.insert("level", "info");
    log_event.insert("logger", "vector.test");

    let events = vec![Event::Log(log_event)];
    let events = stream::iter(events);

    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;

    // Verify the mock server received the request
    // Give some time for the request to be processed
    timeout(Duration::from_secs(5), async {
        loop {
            let requests = mock_server.received_requests().await.unwrap();
            if !requests.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Should have received at least one request");

    let requests = mock_server.received_requests().await.unwrap();
    assert!(
        !requests.is_empty(),
        "Should have received at least one request"
    );

    // Verify the request contains a properly formatted Sentry envelope
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    // Check that the path ends with /envelope/ for the project ID (Sentry API format)
    let path = request.url.path();
    assert!(
        path.contains("/envelope") || path.matches(r"/api/[0-9]+/envelope/").any(|_| true),
        "Expected path to contain '/envelope' but got: {}",
        path
    );

    // Verify headers
    assert_eq!(
        request
            .headers
            .get("Content-Type")
            .unwrap()
            .to_str()
            .unwrap(),
        "application/x-sentry-envelope"
    );
    assert_eq!(
        request.headers.get("User-Agent").unwrap().to_str().unwrap(),
        "sentry.vector/0.1.0"
    );

    // Parse and verify the envelope structure
    let body = &request.body;
    assert!(!body.is_empty(), "Request body should not be empty");

    // The envelope should be parseable by the Sentry library
    let envelope = Envelope::from_slice(body);
    assert!(envelope.is_ok(), "Should be able to parse Sentry envelope");

    let envelope = envelope.unwrap();
    let mut items_count = 0;
    for item in envelope.items() {
        items_count += 1;
        // Log events can be converted to different Sentry item types
        match item {
            EnvelopeItem::Event(_) => {
                // This is a Sentry event (expected for errors/exceptions)
            }
            EnvelopeItem::Transaction(_) => {
                // This might be a transaction if the log has performance data
            }
            _ => {
                // Accept other types as well, since log events might be encoded differently
            }
        }
    }
    assert!(items_count > 0, "Envelope should contain items");
}

#[tokio::test]
async fn sentry_sink_multiple_events() {
    trace_init();

    let mock_server = sentry_mock_server().await;
    let dsn = format!(
        "http://test-key@{}/123",
        mock_server.uri().replace("http://", "")
    );

    let config = SentryConfig {
        dsn: dsn.clone(),
        batch: BatchConfig::default(),
        request: RequestConfig::default(),
        tls: None,
        encoding: EncodingConfig::new(
            JsonSerializerConfig::new(MetricTagValues::Full, JsonSerializerOptions::default())
                .into(),
            Transformer::default(),
        ),
        acknowledgements: AcknowledgementsConfig::default(),
    };

    let cx = SinkContext::default();
    let (sink, _) = config.build(cx).await.unwrap();

    // Create multiple log events
    let mut events = Vec::new();
    for i in 0..3 {
        let mut log_event = LogEvent::from(format!("test message {}", i));
        log_event.insert("level", "error");
        log_event.insert("logger", "vector.test");
        log_event.insert("custom_field", format!("value_{}", i));
        events.push(Event::Log(log_event));
    }

    let events = stream::iter(events);
    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;

    // Wait for requests to be processed
    timeout(Duration::from_secs(5), async {
        loop {
            let requests = mock_server.received_requests().await.unwrap();
            if !requests.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Should have received at least one request");

    let requests = mock_server.received_requests().await.unwrap();
    assert!(
        !requests.is_empty(),
        "Should have received at least one request"
    );

    // Since we're batching, we might get multiple requests or a single request with multiple events
    // Let's verify that we can parse all the envelopes
    for request in &requests {
        let envelope = Envelope::from_slice(&request.body);
        assert!(envelope.is_ok(), "Should be able to parse Sentry envelope");
        let envelope = envelope.unwrap();
        let mut items_count = 0;
        for _item in envelope.items() {
            items_count += 1;
        }
        assert!(items_count > 0, "Envelope should contain items");
    }
}

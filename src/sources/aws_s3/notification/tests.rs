use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::Notify;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{header, method, path},
};

use crate::{
    SourceSender,
    config::{SourceConfig, SourceContext},
    event::{Event, EventStatus},
    shutdown::ShutdownSignal,
    sources::aws_s3::{
        AwsS3Config, NotificationBatch, NotificationBatches, NotificationParser, S3ObjectReference,
        sqs::{DeferredConfig, check_notification_age},
    },
};

mod compatibility;

// A test format different from standard S3 notifications.
struct TestParser;

#[derive(Deserialize)]
struct TestNotification {
    objects: Vec<TestObject>,
    published_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct TestObject {
    bucket: String,
    key: String,
    region: String,
}

impl NotificationParser for TestParser {
    fn parse(&self, body: &str) -> NotificationBatches {
        let batch = serde_json::from_str::<TestNotification>(body)
            .map(|notification| {
                NotificationBatch::new(
                    notification
                        .objects
                        .into_iter()
                        .map(|object| {
                            S3ObjectReference::new(object.bucket, object.key, object.region)
                        })
                        .collect(),
                    notification.published_at,
                )
            })
            .map_err(Into::into);
        vec![batch]
    }
}

fn custom_body(keys: &[&str], published_at: Option<DateTime<Utc>>) -> String {
    let objects = keys
        .iter()
        .map(|key| serde_json::json!({"bucket":"synthetic", "key":key, "region":"us-east-1"}))
        .collect::<Vec<_>>();
    serde_json::json!({"objects":objects, "published_at":published_at}).to_string()
}

fn standard_body() -> String {
    let records = ["one.gz", "two.gz"].map(|key| {
        serde_json::json!({
            "eventVersion":"2.1", "eventSource":"aws:s3", "awsRegion":"us-east-1",
            "eventName":"ObjectCreated:Put", "eventTime":"2026-01-01T00:00:00Z",
            "s3":{"s3SchemaVersion":"1.0", "configurationId":"synthetic",
                "bucket":{"name":"synthetic"}, "object":{"key":key}}
        })
    });
    serde_json::json!({"Records":records}).to_string()
}

fn gzip(body: &str) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(body.as_bytes()).unwrap();
    encoder.finish().unwrap()
}

fn has_target(request: &Request, target: &str) -> bool {
    request
        .headers
        .get("x-amz-target")
        .is_some_and(|value| value == target)
}

struct Fixture {
    server: MockServer,
    processed: Arc<Notify>,
}

impl Fixture {
    async fn new(bodies: Vec<String>) -> Self {
        let server = MockServer::start().await;
        let processed = Arc::new(Notify::new());
        let receives = AtomicUsize::new(0);
        let messages = bodies.iter().enumerate().map(|(index, body)| serde_json::json!({
            "MessageId":format!("message-{index}"), "ReceiptHandle":format!("receipt-{index}"), "Body":body,
        })).collect::<Vec<_>>();
        let signal = Arc::clone(&processed);
        Mock::given(header("x-amz-target", "AmazonSQS.ReceiveMessage"))
            .respond_with(move |_: &Request| {
                let response = if receives.fetch_add(1, Ordering::SeqCst) == 0 {
                    serde_json::json!({"Messages":messages})
                } else {
                    // A second receive starts only after processing and receipt handling finish.
                    signal.notify_one();
                    serde_json::json!({"Messages":[]})
                };
                ResponseTemplate::new(200).set_body_json(response)
            })
            .mount(&server)
            .await;
        Mock::given(header("x-amz-target", "AmazonSQS.DeleteMessageBatch"))
            .respond_with(|request: &Request| {
                let payload: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                let successful = payload["Entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|entry| serde_json::json!({"Id":entry["Id"]}))
                    .collect::<Vec<_>>();
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"Successful":successful}))
            })
            .mount(&server)
            .await;
        Mock::given(header("x-amz-target", "AmazonSQS.SendMessageBatch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"Successful":[{"Id":"message-0", "MessageId":"forwarded"}]}),
            ))
            .mount(&server)
            .await;
        Self { server, processed }
    }

    fn config(&self, deferred: bool) -> AwsS3Config {
        let mut config = serde_json::json!({
            "region":"us-east-1", "endpoint":self.server.uri(), "force_path_style":true,
            "auth":{"access_key_id":"synthetic", "secret_access_key":"synthetic"},
            "sqs":{"queue_url":format!("{}/queue", self.server.uri()), "client_concurrency":1, "poll_secs":0},
            "decoding":{"codec":"json"}
        });
        if deferred {
            config["sqs"]["deferred"] = serde_json::json!({"queue_url":format!("{}/deferred", self.server.uri()), "max_age_secs":60});
        }
        serde_json::from_value(config).unwrap()
    }

    async fn object(&self, key: &str, value: i64) {
        Mock::given(method("GET"))
            .and(path(format!("/synthetic/{key}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(gzip(&format!("{{\"value\":{value}}}\n"))),
            )
            .mount(&self.server)
            .await;
    }

    async fn process(
        &self,
        config: &AwsS3Config,
        parser: Option<Arc<dyn NotificationParser>>,
        status: EventStatus,
    ) -> Vec<Event> {
        let (out, rx) = SourceSender::new_test_finalize(status);
        let (stop, shutdown, _done) = ShutdownSignal::new_wired();
        let mut cx = SourceContext::new_test(out, None);
        cx.shutdown = shutdown;
        cx.acknowledgements = true;
        let source = match parser {
            Some(parser) => config.build_with_notification_parser(cx, parser).await,
            None => config.build(cx).await,
        }
        .unwrap();
        let stop_after_attempt = async {
            self.processed.notified().await;
            stop.cancel();
        };
        let (result, (), events) = tokio::time::timeout(Duration::from_secs(15), async {
            tokio::join!(source, stop_after_attempt, rx.collect())
        })
        .await
        .unwrap();
        result.unwrap();
        events
    }

    async fn downloads(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|request| request.method == "GET")
            .map(|request| request.url.path().to_owned())
            .collect()
    }

    async fn calls(&self, target: &str) -> Vec<Request> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|request| has_target(request, target))
            .collect()
    }
}

#[tokio::test]
async fn custom_parser_uses_shared_reader_and_ack_gated_deletion() {
    let fixture = Fixture::new(vec![custom_body(&["literal%2F+one.gz", "two.gz"], None)]).await;
    for (key, body) in [
        ("literal%252F%2Bone.gz", "{\"value\":1}\n{\"value\":2}\n"),
        ("two.gz", "{\"value\":3}\n"),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/synthetic/{key}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(gzip(body)))
            .mount(&fixture.server)
            .await;
    }
    let config = fixture.config(false);
    let (out, mut rx) = SourceSender::new_test();
    let (stop, shutdown, _done) = ShutdownSignal::new_wired();
    let mut cx = SourceContext::new_test(out, None);
    cx.shutdown = shutdown;
    cx.acknowledgements = true;
    let source = config
        .build_with_notification_parser(cx, Arc::new(TestParser))
        .await
        .unwrap();
    let consume = async {
        for value in 1..=3 {
            let mut event = rx.next().await.unwrap();
            assert_eq!(event.as_log()["value"], value.into());
            assert_eq!(event.as_log()["bucket"], "synthetic".into());
            assert_eq!(event.as_log()["region"], "us-east-1".into());
            if value == 3 {
                tokio::time::sleep(Duration::from_millis(50)).await;
                assert!(
                    fixture
                        .calls("AmazonSQS.DeleteMessageBatch")
                        .await
                        .is_empty()
                );
            }
            event.metadata_mut().update_status(EventStatus::Delivered);
            event.metadata_mut().update_sources();
        }
        fixture.processed.notified().await;
        stop.cancel();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(source, consume)
    })
    .await
    .unwrap();
    result.unwrap();
    let requests = fixture.calls("AmazonSQS.DeleteMessageBatch").await;
    assert_eq!(requests.len(), 1);
    let payload: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(payload["Entries"].as_array().unwrap().len(), 1);
    assert_eq!(payload["Entries"][0]["ReceiptHandle"], "receipt-0");
    assert!(fixture.calls("AmazonSQS.SendMessageBatch").await.is_empty());
}

#[tokio::test]
async fn invalid_or_empty_custom_notifications_do_not_download() {
    let mut wrong_region: serde_json::Value =
        serde_json::from_str(&custom_body(&["one.gz", "two.gz"], None)).unwrap();
    wrong_region["objects"][1]["region"] = "us-west-2".into();
    for (body, deferred, deleted) in [
        ("not a notification".to_owned(), false, false),
        (wrong_region.to_string(), false, false),
        (custom_body(&["one.gz"], None), true, false),
        (custom_body(&[], None), true, true),
    ] {
        let fixture = Fixture::new(vec![body]).await;
        let events = fixture
            .process(
                &fixture.config(deferred),
                Some(Arc::new(TestParser)),
                EventStatus::Delivered,
            )
            .await;
        assert!(events.is_empty());
        assert!(fixture.downloads().await.is_empty());
        assert!(fixture.calls("AmazonSQS.SendMessageBatch").await.is_empty());
        assert_eq!(
            fixture.calls("AmazonSQS.DeleteMessageBatch").await.len(),
            usize::from(deleted)
        );
    }
}

struct OrderedParser;

impl NotificationParser for OrderedParser {
    fn parse(&self, _body: &str) -> NotificationBatches {
        let batch = |key: &str| {
            NotificationBatch::new(
                vec![S3ObjectReference::new(
                    "synthetic".into(),
                    key.to_owned(),
                    "us-east-1".into(),
                )],
                None,
            )
        };
        vec![
            Ok(batch("one.gz")),
            Err(std::io::Error::other("invalid second batch").into()),
            Ok(batch("three.gz")),
        ]
    }
}

#[tokio::test]
async fn parser_or_delivery_failure_stops_processing_and_preserves_receipt() {
    for status in [EventStatus::Delivered, EventStatus::Rejected] {
        let fixture = Fixture::new(vec!["application envelope".into()]).await;
        fixture.object("one.gz", 1).await;
        fixture.object("three.gz", 3).await;
        let mut config = fixture.config(false);
        config.sqs.as_mut().unwrap().delete_failed_message = false;
        let events = fixture
            .process(&config, Some(Arc::new(OrderedParser)), status)
            .await;
        assert_eq!(events.len(), 1);
        assert_eq!(fixture.downloads().await, ["/synthetic/one.gz"]);
        assert!(
            fixture
                .calls("AmazonSQS.DeleteMessageBatch")
                .await
                .is_empty()
        );
    }
}

#[tokio::test]
async fn failed_object_stops_remaining_files_and_preserves_receipt() {
    let fixture = Fixture::new(vec![custom_body(
        &["one.gz", "missing.gz", "three.gz"],
        None,
    )])
    .await;
    fixture.object("one.gz", 1).await;
    Mock::given(method("GET"))
        .and(path("/synthetic/missing.gz"))
        .respond_with(
            ResponseTemplate::new(404).set_body_string("<Error><Code>NoSuchKey</Code></Error>"),
        )
        .mount(&fixture.server)
        .await;
    let events = fixture
        .process(
            &fixture.config(false),
            Some(Arc::new(TestParser)),
            EventStatus::Delivered,
        )
        .await;
    assert_eq!(events.len(), 1);
    assert_eq!(
        fixture.downloads().await,
        ["/synthetic/one.gz", "/synthetic/missing.gz"]
    );
    assert!(
        fixture
            .calls("AmazonSQS.DeleteMessageBatch")
            .await
            .is_empty()
    );
}

#[test]
fn batch_age_check_uses_publication_time() {
    let published_at: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
    let batch = TestParser
        .parse(&custom_body(&["one.gz", "two.gz"], Some(published_at)))
        .pop()
        .unwrap()
        .unwrap();
    let deferred = DeferredConfig {
        queue_url: "http://localhost/deferred".into(),
        max_age_secs: 60,
    };
    for (age_millis, too_old) in [
        (-1_000, false),
        (59_000, false),
        (60_000, false),
        (60_999, false),
        (61_000, true),
    ] {
        let now = published_at + chrono::Duration::milliseconds(age_millis);
        assert_eq!(
            check_notification_age(&batch, &deferred, now).is_err(),
            too_old,
            "age: {age_millis} ms"
        );
    }
}

#[tokio::test]
async fn custom_batch_publication_time_controls_deferral() {
    for (published_at, expected_events) in
        [("2000-01-01T00:00:00Z", 0), ("2100-01-01T00:00:00Z", 2)]
    {
        let fixture = Fixture::new(vec![custom_body(
            &["one.gz", "two.gz"],
            Some(published_at.parse().unwrap()),
        )])
        .await;
        fixture.object("one.gz", 1).await;
        fixture.object("two.gz", 2).await;
        let events = fixture
            .process(
                &fixture.config(true),
                Some(Arc::new(TestParser)),
                EventStatus::Delivered,
            )
            .await;
        assert_eq!(events.len(), expected_events);
        assert_eq!(fixture.downloads().await.len(), expected_events);
        assert_eq!(
            fixture.calls("AmazonSQS.SendMessageBatch").await.len(),
            usize::from(expected_events == 0)
        );
        assert_eq!(fixture.calls("AmazonSQS.DeleteMessageBatch").await.len(), 1);
    }
}

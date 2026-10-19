use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use futures::{FutureExt, StreamExt};
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
        Box::new(std::iter::once(batch))
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

fn gzip_members(lines: &[&str]) -> Vec<u8> {
    lines
        .iter()
        .flat_map(|line| {
            let mut member = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            member.write_all(line.as_bytes()).unwrap();
            member.finish().unwrap()
        })
        .collect()
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
                    .set_body_bytes(gzip_members(&[&format!("{{\"value\":{value}}}\n")])),
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
    for (key, lines) in [
        (
            "literal%252F%2Bone.gz",
            vec!["{\"value\":1}\n", "{\"value\":2}\n"],
        ),
        ("two.gz", vec!["{\"value\":3}\n"]),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/synthetic/{key}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(gzip_members(&lines)))
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

struct OrderedParser {
    next_calls: Arc<AtomicUsize>,
}

impl NotificationParser for OrderedParser {
    fn parse(&self, _body: &str) -> NotificationBatches {
        let next_calls = Arc::clone(&self.next_calls);
        Box::new(std::iter::from_fn(move || {
            match next_calls.fetch_add(1, Ordering::SeqCst) {
                0 => Some(Ok(NotificationBatch::new(
                    vec![S3ObjectReference::new(
                        "synthetic".into(),
                        "one.gz".into(),
                        "us-east-1".into(),
                    )],
                    None,
                ))),
                1 => Some(Err(std::io::Error::other("invalid second batch").into())),
                _ => panic!("must not advance after a batch error"),
            }
        }))
    }
}

#[tokio::test]
async fn parser_or_delivery_failure_stops_iteration_and_preserves_receipt() {
    for (status, expected_calls) in [(EventStatus::Delivered, 2), (EventStatus::Rejected, 1)] {
        let fixture = Fixture::new(vec!["application envelope".into()]).await;
        fixture.object("one.gz", 1).await;
        let next_calls = Arc::new(AtomicUsize::new(0));
        let parser = Arc::new(OrderedParser {
            next_calls: Arc::clone(&next_calls),
        });
        let mut config = fixture.config(false);
        config.sqs.as_mut().unwrap().delete_failed_message = false;
        let events = fixture.process(&config, Some(parser), status).await;
        assert_eq!(events.len(), 1);
        assert_eq!(next_calls.load(Ordering::SeqCst), expected_calls);
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

#[tokio::test]
async fn age_is_checked_per_s3_record_but_once_per_application_batch() {
    for custom in [false, true] {
        let fixture = Fixture::new(vec![]).await;
        fixture.object("one.gz", 1).await;
        fixture.object("two.gz", 2).await;
        let mut config = fixture.config(true);
        config
            .sqs
            .as_mut()
            .unwrap()
            .deferred
            .as_mut()
            .unwrap()
            .max_age_secs = 0;
        let (out, mut rx) = SourceSender::new_test();
        let (stop, shutdown, _done) = ShutdownSignal::new_wired();
        let mut cx = SourceContext::new_test(out, None);
        cx.shutdown = shutdown;
        cx.acknowledgements = true;
        let source = if custom {
            config
                .build_with_notification_parser(cx, Arc::new(TestParser))
                .await
        } else {
            config.build(cx).await
        }
        .unwrap();

        // Start the age window after client setup, not before it.
        let published_at = Utc::now() + chrono::Duration::seconds(5);
        let body = if custom {
            custom_body(&["one.gz", "two.gz"], Some(published_at))
        } else {
            let mut body: serde_json::Value = serde_json::from_str(&standard_body()).unwrap();
            for record in body["Records"].as_array_mut().unwrap() {
                record["eventTime"] = published_at.to_rfc3339().into();
            }
            body.to_string()
        };
        Mock::given(header("x-amz-target", "AmazonSQS.ReceiveMessage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Messages":[{"MessageId":"message-0", "ReceiptHandle":"receipt-0", "Body":body}]
            })))
            .with_priority(1)
            .up_to_n_times(1)
            .mount(&fixture.server)
            .await;

        let consume = async {
            let mut first = rx.next().await.unwrap();
            assert_eq!(first.as_log()["value"], 1.into());
            let wait = (published_at + chrono::Duration::seconds(2) - Utc::now())
                .to_std()
                .unwrap_or_default();
            tokio::time::sleep(wait).await;
            assert!(
                fixture
                    .calls("AmazonSQS.DeleteMessageBatch")
                    .await
                    .is_empty()
            );
            first.metadata_mut().update_status(EventStatus::Delivered);
            first.metadata_mut().update_sources();
            if custom {
                let mut second = rx.next().await.unwrap();
                assert_eq!(second.as_log()["value"], 2.into());
                second.metadata_mut().update_status(EventStatus::Delivered);
                second.metadata_mut().update_sources();
            }
            fixture.processed.notified().await;
            assert!(rx.next().now_or_never().is_none());
            stop.cancel();
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(20), async {
            tokio::join!(source, consume)
        })
        .await
        .unwrap();
        result.unwrap();
        assert_eq!(fixture.downloads().await.len(), if custom { 2 } else { 1 });
        assert_eq!(
            fixture.calls("AmazonSQS.SendMessageBatch").await.len(),
            usize::from(!custom)
        );
        assert_eq!(fixture.calls("AmazonSQS.DeleteMessageBatch").await.len(), 1);
    }
}

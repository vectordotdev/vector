//! Regression tests for existing S3/SNS behavior.

use super::*;

fn notification() -> serde_json::Value {
    serde_json::from_str(&standard_body()).unwrap()
}

#[tokio::test]
async fn s3_sns_and_test_notifications_use_the_default_adapter() {
    let sns = serde_json::json!({"Message":standard_body(), "Timestamp":"2026-01-01T00:00:00Z"})
        .to_string();
    let test =
        serde_json::json!({"Service":"Amazon S3", "Event":"s3:TestEvent", "Bucket":"synthetic"})
            .to_string();
    let fixture = Fixture::new(vec![standard_body(), sns, test]).await;
    fixture.object("one.gz", 1).await;
    fixture.object("two.gz", 2).await;
    let events = fixture
        .process(&fixture.config(false), None, EventStatus::Delivered)
        .await;
    assert_eq!(events.len(), 4);
    assert!(
        events
            .iter()
            .all(|event| event.as_log()["source_type"] == AwsS3Config::NAME.into())
    );
    assert_eq!(
        fixture.downloads().await,
        [
            "/synthetic/one.gz",
            "/synthetic/two.gz",
            "/synthetic/one.gz",
            "/synthetic/two.gz"
        ]
    );
    let requests = fixture.calls("AmazonSQS.DeleteMessageBatch").await;
    assert_eq!(requests.len(), 1);
    let payload: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(payload["Entries"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn malformed_later_record_prevents_all_downloads() {
    let mut body = notification();
    body["Records"][1]
        .as_object_mut()
        .unwrap()
        .remove("eventTime");
    let fixture = Fixture::new(vec![body.to_string()]).await;
    let events = fixture
        .process(&fixture.config(false), None, EventStatus::Delivered)
        .await;
    assert!(events.is_empty());
    assert!(fixture.downloads().await.is_empty());
    assert!(
        fixture
            .calls("AmazonSQS.DeleteMessageBatch")
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn unsupported_later_version_does_not_prevent_earlier_delivery() {
    let mut body = notification();
    body["Records"][1]["eventVersion"] = "3.0".into();
    let fixture = Fixture::new(vec![body.to_string()]).await;
    fixture.object("one.gz", 1).await;
    let events = fixture
        .process(&fixture.config(false), None, EventStatus::Delivered)
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

#[tokio::test]
async fn ignored_event_bypasses_region_and_age_checks() {
    let mut body = notification();
    body["Records"][0]["eventName"] = "ObjectRemoved:Delete".into();
    body["Records"][0]["awsRegion"] = "wrong-region".into();
    body["Records"][1]["eventTime"] = Utc::now().to_rfc3339().into();
    let fixture = Fixture::new(vec![body.to_string()]).await;
    fixture.object("two.gz", 2).await;
    let events = fixture
        .process(&fixture.config(true), None, EventStatus::Delivered)
        .await;
    assert_eq!(events.len(), 1);
    assert_eq!(fixture.downloads().await, ["/synthetic/two.gz"]);
    assert!(fixture.calls("AmazonSQS.SendMessageBatch").await.is_empty());
    assert_eq!(fixture.calls("AmazonSQS.DeleteMessageBatch").await.len(), 1);
}

#[tokio::test]
async fn ignored_event_still_requires_supported_version() {
    let mut body = notification();
    body["Records"][0]["eventName"] = "ObjectRemoved:Delete".into();
    body["Records"][0]["eventVersion"] = "3.0".into();
    let fixture = Fixture::new(vec![body.to_string()]).await;
    let events = fixture
        .process(&fixture.config(false), None, EventStatus::Delivered)
        .await;
    assert!(events.is_empty());
    assert!(fixture.downloads().await.is_empty());
    assert!(
        fixture
            .calls("AmazonSQS.DeleteMessageBatch")
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn region_validation_precedes_age_deferral() {
    let mut body = notification();
    body["Records"][0]["awsRegion"] = "wrong-region".into();
    let fixture = Fixture::new(vec![body.to_string()]).await;
    let events = fixture
        .process(&fixture.config(true), None, EventStatus::Delivered)
        .await;
    assert!(events.is_empty());
    assert!(fixture.downloads().await.is_empty());
    assert!(fixture.calls("AmazonSQS.SendMessageBatch").await.is_empty());
    assert!(
        fixture
            .calls("AmazonSQS.DeleteMessageBatch")
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn old_later_record_defers_original_sns_body_after_earlier_delivery() {
    let mut body = notification();
    body["Records"][0]["eventTime"] = Utc::now().to_rfc3339().into();
    // SNS publication time must not replace the individual S3 event times.
    let body = serde_json::json!({"Message":body.to_string(), "Timestamp":Utc::now().to_rfc3339()})
        .to_string();
    let fixture = Fixture::new(vec![body.clone()]).await;
    fixture.object("one.gz", 1).await;
    let events = fixture
        .process(&fixture.config(true), None, EventStatus::Delivered)
        .await;
    assert_eq!(events.len(), 1);
    assert_eq!(fixture.downloads().await, ["/synthetic/one.gz"]);
    let sent = fixture.calls("AmazonSQS.SendMessageBatch").await;
    assert_eq!(sent.len(), 1);
    let payload: serde_json::Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert_eq!(payload["Entries"][0]["MessageBody"], body);
    assert_eq!(fixture.calls("AmazonSQS.DeleteMessageBatch").await.len(), 1);
}

#[tokio::test]
async fn legacy_forward_failure_still_queues_deletion() {
    // Preserve the existing forwarding bug here; fix it separately.
    let fixture = Fixture::new(vec![standard_body()]).await;
    Mock::given(header("x-amz-target", "AmazonSQS.SendMessageBatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "Failed":[{"Id":"message-0", "Code":"AccessDenied", "SenderFault":true}]
        })))
        .with_priority(1)
        .mount(&fixture.server)
        .await;
    let events = fixture
        .process(&fixture.config(true), None, EventStatus::Delivered)
        .await;
    assert!(events.is_empty());
    assert!(fixture.downloads().await.is_empty());
    assert_eq!(fixture.calls("AmazonSQS.SendMessageBatch").await.len(), 1);
    assert_eq!(fixture.calls("AmazonSQS.DeleteMessageBatch").await.len(), 1);
}

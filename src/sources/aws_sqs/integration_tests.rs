#![cfg(feature = "aws-sqs-integration-tests")]
#![cfg(test)]

use std::{collections::HashSet, time::Duration};

use aws_sdk_sqs::operation::create_queue::CreateQueueOutput;
use aws_types::region::Region;
use futures::StreamExt;
use tokio::time::timeout;

use crate::{
    aws::{auth::AwsAuthentication, region::RegionOrEndpoint},
    config::{log_schema, SourceConfig, SourceContext},
    event::Event,
    sources::aws_sqs::config::AwsSqsConfig,
    test_util::{
        components::{assert_source_compliance, HTTP_PULL_SOURCE_TAGS},
        random_string,
    },
    SourceSender,
};

fn sqs_address() -> String {
    std::env::var("SQS_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

fn gen_queue_name() -> String {
    random_string(10).to_lowercase()
}

async fn ensure_queue(queue_name: &str, client: &aws_sdk_sqs::Client) -> CreateQueueOutput {
    client
        .create_queue()
        .queue_name(queue_name)
        .send()
        .await
        .unwrap()
}

async fn send_test_events(count: u32, queue_url: &str, client: &aws_sdk_sqs::Client) {
    for i in 0..count {
        client
            .send_message()
            .message_body(calculate_message(i))
            .queue_url(queue_url)
            .send()
            .await
            .unwrap();
    }
}

async fn get_sqs_client() -> aws_sdk_sqs::Client {
    let config = aws_sdk_sqs::config::Builder::new()
        .credentials_provider(
            AwsAuthentication::test_auth()
                .credentials_provider(Region::new("custom"), &Default::default(), &None)
                .await
                .unwrap()
                .unwrap(),
        )
        .endpoint_url(sqs_address())
        .region(Some(Region::new("us-east-1")))
        .build();

    aws_sdk_sqs::Client::from_conf(config)
}

#[tokio::test]
pub(crate) async fn test() {
    assert_source_compliance(&HTTP_PULL_SOURCE_TAGS, async {
        let sqs_client = get_sqs_client().await;
        let queue_name = gen_queue_name();
        let queue_url = ensure_queue(&queue_name, &sqs_client)
            .await
            .queue_url
            .expect("Create queue should return the url");

        let num_events = 3;
        send_test_events(num_events, &queue_url, &sqs_client).await;

        let config = AwsSqsConfig {
            region: RegionOrEndpoint::with_both("us-east-1", sqs_address().as_str()),
            auth: AwsAuthentication::test_auth(),
            queue_url: queue_url.clone(),
            ..Default::default()
        };

        let (tx, rx) = SourceSender::new_test();
        tokio::spawn(async move {
            config
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap()
                .await
                .unwrap()
        });

        let mut expected_messages = HashSet::new();
        for i in 0..num_events {
            expected_messages.insert(calculate_message(i));
        }

        let events: Vec<Event> = timeout(
            Duration::from_secs(10),
            rx.take(num_events as usize).collect(),
        )
        .await
        .unwrap();

        for event in events {
            let message = event
                .as_log()
                .get(log_schema().message_key_target_path().unwrap())
                .unwrap()
                .to_string_lossy();
            if !expected_messages.remove(message.as_ref()) {
                panic!("Received unexpected message: {:?}", message);
            }
        }
        assert!(expected_messages.is_empty());
    })
    .await;
}

fn calculate_message(index: u32) -> String {
    format!("Test message: {}", index)
}

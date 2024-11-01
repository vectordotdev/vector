use std::collections::HashMap;

use aws_config::Region;
use aws_sdk_sns::Client as SnsClient;
use aws_sdk_sqs::{types::QueueAttributeName, Client as SqsClient};
use tokio::time::{sleep, Duration};
use vector_lib::codecs::TextSerializerConfig;

use super::{
    config::SnsClientBuilder,
    config::{healthcheck, SnsSinkConfig},
    BaseSSSinkConfig,
};
use crate::common::sqs::SqsClientBuilder;
use crate::config::{SinkConfig, SinkContext};
use crate::{
    aws::{create_client, AwsAuthentication, RegionOrEndpoint},
    config::ProxyConfig,
    test_util::{
        components::{run_and_assert_sink_compliance, AWS_SINK_TAGS},
        random_lines_with_stream, random_string,
    },
};

fn sns_address() -> String {
    std::env::var("SNS_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

async fn create_sns_test_client() -> SnsClient {
    let auth = AwsAuthentication::test_auth();

    let endpoint = sns_address();
    let proxy = ProxyConfig::default();
    create_client::<SnsClientBuilder>(
        &auth,
        Some(Region::new("us-east-1")),
        Some(endpoint),
        &proxy,
        &None,
        &None,
    )
    .await
    .unwrap()
}

fn sqs_address() -> String {
    std::env::var("SQS_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

async fn create_sqs_test_client() -> SqsClient {
    let auth = AwsAuthentication::test_auth();

    let endpoint = sqs_address();
    let proxy = ProxyConfig::default();
    create_client::<SqsClientBuilder>(
        &auth,
        Some(Region::new("us-east-1")),
        Some(endpoint),
        &proxy,
        &None,
        &None,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn sns_send_message_batch() {
    let topic_name = gen_topic_name();
    let topic_arn = ensure_topic(topic_name.clone()).await;

    let sqs_client = create_sqs_test_client().await;
    let queue_url = ensure_queue(&sqs_client, gen_queue_name()).await;
    let queue_arn = get_queue_arn(&sqs_client, queue_url.clone()).await;

    let sns_client = create_sns_test_client().await;

    let base_config = BaseSSSinkConfig {
        encoding: TextSerializerConfig::default().into(),
        message_group_id: None,
        message_deduplication_id: None,
        request: Default::default(),
        tls: Default::default(),
        assume_role: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
    };

    let config = SnsSinkConfig {
        region: RegionOrEndpoint::with_both("us-east-1", sns_address().as_str()),
        topic_arn: topic_arn.clone(),
        base_config,
    };

    healthcheck(sns_client.clone(), config.topic_arn.clone())
        .await
        .unwrap();

    subscribe_queue_to_topic(&sns_client, &topic_arn, &queue_arn).await;

    let cx = SinkContext::default();
    let sink = config.build(cx).await.unwrap().0;

    let (mut input_lines, events) = random_lines_with_stream(100, 10, None);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    sleep(Duration::from_secs(1)).await;

    let response = sqs_client
        .receive_message()
        .max_number_of_messages(input_lines.len() as i32)
        .queue_url(queue_url)
        .send()
        .await
        .unwrap();

    let mut output_lines = response
        .clone()
        .messages
        .unwrap()
        .into_iter()
        .map(|e| e.body.unwrap())
        .collect::<Vec<_>>();

    input_lines.sort();
    output_lines.sort();

    assert_eq!(output_lines, input_lines);
    assert_eq!(input_lines.len(), response.messages.unwrap().len());
}

async fn ensure_topic(topic_arn: String) -> String {
    let client = create_sns_test_client().await;

    let attributes: Option<HashMap<String, String>> = None;

    client
        .create_topic()
        .set_attributes(attributes)
        .name(topic_arn)
        .send()
        .await
        .unwrap()
        .topic_arn
        .unwrap()
}

fn gen_topic_name() -> String {
    format!("test-{}", random_string(10).to_lowercase())
}

async fn ensure_queue(client: &SqsClient, queue_name: String) -> String {
    client
        .create_queue()
        .queue_name(queue_name)
        .send()
        .await
        .unwrap()
        .queue_url
        .unwrap()
}

async fn get_queue_arn(client: &SqsClient, queue_url: String) -> String {
    let arn_attribute = QueueAttributeName::QueueArn;
    client
        .get_queue_attributes()
        .queue_url(queue_url)
        .attribute_names(QueueAttributeName::QueueArn)
        .send()
        .await
        .unwrap()
        .attributes
        .unwrap()
        .get(&arn_attribute)
        .unwrap()
        .clone()
}

fn gen_queue_name() -> String {
    format!("test-{}", random_string(10).to_lowercase())
}

async fn subscribe_queue_to_topic(sns_client: &SnsClient, topic_arn: &str, queue_arn: &str) {
    let mut attributes = HashMap::new();
    attributes.insert("RawMessageDelivery".to_string(), "true".to_string());

    sns_client
        .subscribe()
        .protocol("sqs")
        .endpoint(queue_arn)
        .set_attributes(Some(attributes))
        .topic_arn(topic_arn)
        .send()
        .await
        .unwrap();
}

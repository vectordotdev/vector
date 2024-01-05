use std::collections::HashMap;

use aws_config::Region;
use aws_sdk_sqs::{types::QueueAttributeName, Client as SqsClient};
use tokio::time::{sleep, Duration};
use vector_lib::codecs::TextSerializerConfig;

use crate::config::{SinkConfig, SinkContext};
use crate::sinks::aws_s_s::sqs::{
    config::{healthcheck, SqsSinkConfig},
    BaseSSSinkConfig,
};
use crate::{
    aws::{create_client, AwsAuthentication, RegionOrEndpoint},
    common::sqs::SqsClientBuilder,
    config::ProxyConfig,
    test_util::{
        components::{run_and_assert_sink_compliance, AWS_SINK_TAGS},
        random_lines_with_stream, random_string,
    },
};

fn sqs_address() -> String {
    std::env::var("SQS_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

async fn create_test_client() -> SqsClient {
    let auth = AwsAuthentication::test_auth();

    let endpoint = sqs_address();
    let proxy = ProxyConfig::default();
    create_client::<SqsClientBuilder>(
        &auth,
        Some(Region::new("us-east-1")),
        Some(endpoint),
        &proxy,
        &None,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn sqs_send_message_batch() {
    let queue_name = gen_queue_name();
    ensure_queue(queue_name.clone()).await;
    let queue_url = get_queue_url(queue_name.clone()).await;

    let client = create_test_client().await;

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

    let config = SqsSinkConfig {
        region: RegionOrEndpoint::with_both("us-east-1", sqs_address().as_str()),
        queue_url: queue_url.clone(),
        base_config,
    };

    healthcheck(client.clone(), config.queue_url.clone())
        .await
        .unwrap();

    let cx = SinkContext::default();

    let sink = config.build(cx).await.unwrap().0;

    let (mut input_lines, events) = random_lines_with_stream(100, 10, None);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    sleep(Duration::from_secs(1)).await;

    let response = client
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

async fn ensure_queue(queue_name: String) {
    let client = create_test_client().await;

    let attributes: Option<HashMap<QueueAttributeName, String>> = if queue_name.ends_with(".fifo") {
        let mut hash_map = HashMap::new();
        hash_map.insert(QueueAttributeName::FifoQueue, "true".into());
        Some(hash_map)
    } else {
        None
    };

    client
        .create_queue()
        .set_attributes(attributes)
        .queue_name(queue_name)
        .send()
        .await
        .expect("unable to create queue");
}

async fn get_queue_url(queue_name: String) -> String {
    let client = create_test_client().await;

    client
        .get_queue_url()
        .queue_name(queue_name)
        .send()
        .await
        .unwrap()
        .queue_url
        .unwrap()
}

fn gen_queue_name() -> String {
    format!("test-{}", random_string(10).to_lowercase())
}

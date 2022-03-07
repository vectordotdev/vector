use std::{
    convert::{TryFrom, TryInto},
    num::NonZeroU64,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, stream, FutureExt, Sink, SinkExt, StreamExt, TryFutureExt};
use rusoto_core::RusotoError;
use rusoto_sqs::{
    GetQueueAttributesError, GetQueueAttributesRequest, SendMessageError, SendMessageRequest,
    SendMessageResult, Sqs, SqsClient,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tower::Service;
use tracing_futures::Instrument;
use vector_core::ByteSizeOf;

use super::util::SinkBatchSettings;
use crate::{
    aws::rusoto::{self, AwsAuthentication, RegionOrEndpoint},
    config::{
        log_schema, AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig,
        SinkContext, SinkDescription,
    },
    event::Event,
    internal_events::{AwsSqsEventsSent, TemplateRenderingError},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::RetryLogic,
        sink::Response,
        BatchConfig, EncodedEvent, EncodedLength, TowerRequestConfig, VecBuffer,
    },
    template::{Template, TemplateParseError},
    tls::{MaybeTlsSettings, TlsOptions, TlsSettings},
};
use std::collections::HashMap;

use rusoto_core::Region;
use rusoto_sqs::{CreateQueueRequest, GetQueueUrlRequest, ReceiveMessageRequest};
use tokio::time::{sleep, Duration};

use crate::sinks::VectorSink;
use crate::test_util::{random_lines_with_stream, random_string};

fn sqs_address() -> String {
    std::env::var("SQS_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

#[tokio::test]
async fn sqs_send_message_batch() {
    let cx = SinkContext::new_test();

    let region = Region::Custom {
        name: "localstack".into(),
        endpoint: sqs_address(),
    };

    let queue_name = gen_queue_name();
    ensure_queue(region.clone(), queue_name.clone()).await;
    let queue_url = get_queue_url(region.clone(), queue_name.clone()).await;

    let client = SqsClient::new(region);

    let config = SqsSinkConfig {
        queue_url: queue_url.clone(),
        region: RegionOrEndpoint::with_endpoint(sqs_address().as_str()),
        encoding: Encoding::Text.into(),
        message_group_id: None,
        message_deduplication_id: None,
        request: Default::default(),
        tls: Default::default(),
        assume_role: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
    };

    config.clone().healthcheck(client.clone()).await.unwrap();

    let sink = SqsSink::new(config, cx, client.clone()).unwrap();
    let sink = VectorSink::from_event_sink(sink);

    let (mut input_lines, events) = random_lines_with_stream(100, 10, None);
    sink.run(events).await.unwrap();

    sleep(Duration::from_secs(1)).await;

    let response = client
        .receive_message(ReceiveMessageRequest {
            max_number_of_messages: Some(input_lines.len() as i64),
            queue_url,
            ..Default::default()
        })
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

async fn ensure_queue(region: Region, queue_name: String) {
    let client = SqsClient::new(region);

    let attributes: Option<HashMap<String, String>> = if queue_name.ends_with(".fifo") {
        let mut hash_map = HashMap::new();
        hash_map.insert("FifoQueue".into(), "true".into());
        Some(hash_map)
    } else {
        None
    };

    let req = CreateQueueRequest {
        attributes,
        queue_name,
        tags: None,
    };

    if let Err(error) = client.create_queue(req).await {
        println!("Unable to check the queue {:?}", error);
    }
}

async fn get_queue_url(region: Region, queue_name: String) -> String {
    let client = SqsClient::new(region);

    let req = GetQueueUrlRequest {
        queue_name,
        queue_owner_aws_account_id: None,
    };

    client.get_queue_url(req).await.unwrap().queue_url.unwrap()
}

fn gen_queue_name() -> String {
    format!("test-{}", random_string(10).to_lowercase())
}

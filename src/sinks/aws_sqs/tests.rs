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

use crate::sinks::util::SinkBatchSettings;
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
use std::collections::BTreeMap;

use crate::event::LogEvent;

#[test]
fn sqs_encode_event_text() {
    let message = "hello world".to_string();
    let event = encode_event(message.clone().into(), &Encoding::Text.into(), &None, &None).unwrap();

    assert_eq!(&event.item.message_body, &message);
}

#[test]
fn sqs_encode_event_json() {
    let message = "hello world".to_string();
    let mut event = Event::from(message.clone());
    event.as_mut_log().insert("key", "value");
    let event = encode_event(event, &Encoding::Json.into(), &None, &None).unwrap();

    let map: BTreeMap<String, String> = serde_json::from_str(&event.item.message_body).unwrap();

    assert_eq!(map[&log_schema().message_key().to_string()], message);
    assert_eq!(map["key"], "value".to_string());
}

#[test]
fn sqs_encode_event_deduplication_id() {
    let message_deduplication_id = Template::try_from("{{ transaction_id }}").unwrap();
    let mut log = LogEvent::from("hello world".to_string());
    log.insert("transaction_id", "some id");
    let event = encode_event(
        log.into(),
        &Encoding::Json.into(),
        &None,
        &Some(message_deduplication_id),
    )
    .unwrap();

    assert_eq!(
        event.item.message_deduplication_id,
        Some("some id".to_string())
    );
}

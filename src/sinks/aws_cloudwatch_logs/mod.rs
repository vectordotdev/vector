mod config;
mod healthcheck;
mod request;
mod retry;
mod service;

use crate::aws::rusoto::{self, AwsAuthentication, RegionOrEndpoint};
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, ProxyConfig, SinkConfig, SinkContext, SinkDescription,
    },
    event::{Event, LogEvent, Value},
    internal_events::TemplateRenderingFailed,
    sinks::util::{
        batch::BatchConfig,
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::{FixedRetryPolicy, RetryLogic},
        Compression, EncodedEvent, EncodedLength, PartitionBatchSink, PartitionBuffer,
        PartitionInnerBuffer, TowerRequestConfig, TowerRequestSettings, VecBuffer,
    },
    template::Template,
};
use chrono::{Duration, Utc};
use futures::{future::BoxFuture, ready, stream, FutureExt, SinkExt, StreamExt, TryFutureExt};
use rusoto_core::{request::BufferedHttpResponse, RusotoError};
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, CreateLogGroupError, CreateLogStreamError,
    DescribeLogGroupsRequest, DescribeLogStreamsError, InputLogEvent, PutLogEventsError,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    collections::HashMap,
    convert::TryInto,
    fmt,
    num::NonZeroU64,
    task::{Context, Poll},
};
use tokio::sync::oneshot;
use tower::{
    buffer::Buffer,
    limit::{concurrency::ConcurrencyLimit, rate::RateLimit},
    retry::Retry,
    timeout::Timeout,
    Service, ServiceBuilder, ServiceExt,
};
use vector_core::ByteSizeOf;

use super::util::SinkBatchSettings;

// Estimated maximum size of InputLogEvent with an empty message
const EVENT_SIZE_OVERHEAD: usize = 50;
const MAX_EVENT_SIZE: usize = 256 * 1024;
const MAX_MESSAGE_SIZE: usize = MAX_EVENT_SIZE - EVENT_SIZE_OVERHEAD;

#[derive(Debug, Snafu)]
pub(self) enum CloudwatchLogsError {
    #[snafu(display("{}", source))]
    HttpClientError {
        source: rusoto_core::request::TlsError,
    },
    #[snafu(display("{}", source))]
    InvalidCloudwatchCredentials {
        source: rusoto_credential::CredentialsError,
    },
    #[snafu(display("Encoded event is too long, length={}", length))]
    EventTooLong { length: usize },
}

inventory::submit! {
    SinkDescription::new::<CloudwatchLogsSinkConfig>("aws_cloudwatch_logs")
}

#[derive(Debug)]
pub enum CloudwatchError {
    Put(RusotoError<PutLogEventsError>),
    Describe(RusotoError<DescribeLogStreamsError>),
    CreateStream(RusotoError<CreateLogStreamError>),
    CreateGroup(RusotoError<CreateLogGroupError>),
    NoStreamsFound,
    ServiceDropped,
    MakeService,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CloudwatchKey {
    group: String,
    stream: String,
}

fn encode_log(
    mut log: LogEvent,
    encoding: &EncodingConfig<Encoding>,
) -> Result<InputLogEvent, CloudwatchLogsError> {
    let timestamp = match log.remove(log_schema().timestamp_key()) {
        Some(Value::Timestamp(ts)) => ts.timestamp_millis(),
        _ => Utc::now().timestamp_millis(),
    };

    let message = match encoding.codec() {
        Encoding::Json => serde_json::to_string(&log).unwrap(),
        Encoding::Text => log
            .get(log_schema().message_key())
            .map(|v| v.to_string_lossy())
            .unwrap_or_else(|| "".into()),
    };

    match message.len() {
        length if length <= MAX_MESSAGE_SIZE => Ok(InputLogEvent { message, timestamp }),
        length => Err(CloudwatchLogsError::EventTooLong { length }),
    }
}

fn partition_encode(
    mut event: Event,
    encoding: &EncodingConfig<Encoding>,
    group: &Template,
    stream: &Template,
) -> Option<EncodedEvent<PartitionInnerBuffer<InputLogEvent, CloudwatchKey>>> {
    let group = match group.render_string(&event) {
        Ok(b) => b,
        Err(error) => {
            emit!(&TemplateRenderingFailed {
                error,
                field: Some("group"),
                drop_event: true,
            });
            return None;
        }
    };

    let stream = match stream.render_string(&event) {
        Ok(b) => b,
        Err(error) => {
            emit!(&TemplateRenderingFailed {
                error,
                field: Some("stream"),
                drop_event: true,
            });
            return None;
        }
    };

    let key = CloudwatchKey { group, stream };

    let byte_size = event.size_of();
    encoding.apply_rules(&mut event);
    let event = encode_log(event.into_log(), encoding)
        .map_err(
            |error| error!(message = "Could not encode event.", %error, internal_log_rate_secs = 5),
        )
        .ok()?;

    Some(EncodedEvent::new(
        PartitionInnerBuffer::new(event, key),
        byte_size,
    ))
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeLogGroups failed: {}", source))]
    DescribeLogGroupsFailed {
        source: RusotoError<rusoto_logs::DescribeLogGroupsError>,
    },
    #[snafu(display("No log group found"))]
    NoLogGroup,
    #[snafu(display("Unable to extract group name"))]
    GroupNameError,
    #[snafu(display("Group name mismatch: expected {}, found {}", expected, name))]
    GroupNameMismatch { expected: String, name: String },
}

impl fmt::Display for CloudwatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloudwatchError::Put(error) => write!(f, "CloudwatchError::Put: {}", error),
            CloudwatchError::Describe(error) => write!(f, "CloudwatchError::Describe: {}", error),
            CloudwatchError::CreateStream(error) => {
                write!(f, "CloudwatchError::CreateStream: {}", error)
            }
            CloudwatchError::CreateGroup(error) => {
                write!(f, "CloudwatchError::CreateGroup: {}", error)
            }
            CloudwatchError::NoStreamsFound => write!(f, "CloudwatchError: No Streams Found"),
            CloudwatchError::ServiceDropped => write!(
                f,
                "CloudwatchError: The service was dropped while there was a request in flight."
            ),
            CloudwatchError::MakeService => write!(
                f,
                "CloudwatchError: The inner service was unable to be created."
            ),
        }
    }
}

impl std::error::Error for CloudwatchError {}

impl From<RusotoError<PutLogEventsError>> for CloudwatchError {
    fn from(error: RusotoError<PutLogEventsError>) -> Self {
        CloudwatchError::Put(error)
    }
}

impl From<RusotoError<DescribeLogStreamsError>> for CloudwatchError {
    fn from(error: RusotoError<DescribeLogStreamsError>) -> Self {
        CloudwatchError::Describe(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aws::rusoto::RegionOrEndpoint;
    use crate::event::{Event, Value};
    use std::collections::HashMap;
    use std::convert::{TryFrom, TryInto};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<CloudwatchLogsSinkConfig>();
    }

    #[test]
    fn partition_static() {
        let event = Event::from("hello world");
        let stream = Template::try_from("stream").unwrap();
        let group = "group".try_into().unwrap();
        let encoding = Encoding::Text.into();

        let encoded = partition_encode(event, &encoding, &group, &stream).unwrap();
        let (_event, key) = encoded.item.into_parts();

        let expected = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event() {
        let mut event = Event::from("hello world");

        event.as_mut_log().insert("log_stream", "stream");

        let stream = Template::try_from("{{log_stream}}").unwrap();
        let group = "group".try_into().unwrap();
        let encoding = Encoding::Text.into();

        let encoded = partition_encode(event, &encoding, &group, &stream).unwrap();
        let (_event, key) = encoded.item.into_parts();

        let expected = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event_with_prefix() {
        let mut event = Event::from("hello world");

        event.as_mut_log().insert("log_stream", "stream");

        let stream = Template::try_from("abcd-{{log_stream}}").unwrap();
        let group = "group".try_into().unwrap();
        let encoding = Encoding::Text.into();

        let encoded = partition_encode(event, &encoding, &group, &stream).unwrap();
        let (_event, key) = encoded.item.into_parts();

        let expected = CloudwatchKey {
            stream: "abcd-stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event_with_postfix() {
        let mut event = Event::from("hello world");

        event.as_mut_log().insert("log_stream", "stream");

        let stream = Template::try_from("{{log_stream}}-abcd").unwrap();
        let group = "group".try_into().unwrap();
        let encoding = Encoding::Text.into();

        let encoded = partition_encode(event, &encoding, &group, &stream).unwrap();
        let (_event, key) = encoded.item.into_parts();

        let expected = CloudwatchKey {
            stream: "stream-abcd".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_no_key_event() {
        let event = Event::from("hello world");

        let stream = Template::try_from("{{log_stream}}").unwrap();
        let group = "group".try_into().unwrap();
        let encoding = Encoding::Text.into();

        let stream_val = partition_encode(event, &encoding, &group, &stream);

        assert!(stream_val.is_none());
    }

    fn svc(config: CloudwatchLogsSinkConfig) -> CloudwatchLogsSvc {
        let config = CloudwatchLogsSinkConfig {
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000"),
            ..config
        };
        let key = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };
        let client = config.create_client(&ProxyConfig::from_env()).unwrap();
        CloudwatchLogsSvc::new(&config, &key, client)
    }

    #[test]
    fn cloudwatch_encoded_event_retains_timestamp() {
        let mut event = Event::from("hello world").into_log();
        event.insert("key", "value");
        let encoded = encode_log(event.clone(), &Encoding::Json.into()).unwrap();

        let ts = if let Value::Timestamp(ts) = event[log_schema().timestamp_key()] {
            ts.timestamp_millis()
        } else {
            panic!()
        };

        assert_eq!(encoded.timestamp, ts);
    }

    #[test]
    fn cloudwatch_encode_log_as_json() {
        let mut event = Event::from("hello world").into_log();
        event.insert("key", "value");
        let encoded = encode_log(event, &Encoding::Json.into()).unwrap();
        let map: HashMap<String, String> = serde_json::from_str(&encoded.message[..]).unwrap();
        assert!(map.get(log_schema().timestamp_key()).is_none());
    }

    #[test]
    fn cloudwatch_encode_log_as_text() {
        let mut event = Event::from("hello world").into_log();
        event.insert("key", "value");
        let encoded = encode_log(event, &Encoding::Text.into()).unwrap();
        assert_eq!(encoded.message, "hello world");
    }

    #[test]
    fn cloudwatch_24h_split() {
        let now = Utc::now();
        let events = (0..100)
            .map(|i| now - Duration::hours(i))
            .map(|timestamp| {
                let mut event = Event::new_empty_log();
                event
                    .as_mut_log()
                    .insert(log_schema().timestamp_key(), timestamp);
                encode_log(event.into_log(), &Encoding::Text.into()).unwrap()
            })
            .collect();

        let batches = svc(default_config(Encoding::Text)).process_events(events);

        let day = Duration::days(1).num_milliseconds();
        for batch in batches.iter() {
            assert!((batch.last().unwrap().timestamp - batch.first().unwrap().timestamp) <= day);
        }

        assert_eq!(batches.len(), 5);
    }
}

#[cfg(feature = "aws-cloudwatch-logs-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::aws::rusoto::RegionOrEndpoint;
    use crate::{
        config::{ProxyConfig, SinkConfig, SinkContext},
        test_util::{random_lines, random_lines_with_stream, random_string, trace_init},
    };
    use futures::{stream, SinkExt, StreamExt};
    use pretty_assertions::assert_eq;
    use rusoto_core::Region;
    use rusoto_logs::{CloudWatchLogs, CreateLogGroupRequest, GetLogEventsRequest};
    use std::convert::TryFrom;

    const GROUP_NAME: &str = "vector-cw";

    #[tokio::test]
    async fn cloudwatch_insert_log_event() {
        trace_init();

        ensure_group().await;

        let stream_name = gen_name();
        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from(stream_name.as_str()).unwrap(),
            group_name: Template::try_from(GROUP_NAME).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000"),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11, None);
        sink.run(events).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: stream_name,
            log_group_name: GROUP_NAME.into(),
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[tokio::test]
    async fn cloudwatch_insert_log_events_sorted() {
        trace_init();

        ensure_group().await;

        let stream_name = gen_name();
        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from(stream_name.as_str()).unwrap(),
            group_name: Template::try_from(GROUP_NAME).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000"),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let timestamp = chrono::Utc::now() - chrono::Duration::days(1);

        let (mut input_lines, events) = random_lines_with_stream(100, 11, None);

        // add a historical timestamp to all but the first event, to simulate
        // out-of-order timestamps.
        let mut doit = false;
        let events = events.map(move |mut event| {
            if doit {
                let timestamp = chrono::Utc::now() - chrono::Duration::days(1);

                event
                    .as_mut_log()
                    .insert(log_schema().timestamp_key(), Value::Timestamp(timestamp));
            }
            doit = true;

            event
        });
        let _ = sink.run(events).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: stream_name,
            log_group_name: GROUP_NAME.into(),
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        // readjust input_lines in the same way we have readjusted timestamps.
        let first = input_lines.remove(0);
        input_lines.push(first);
        assert_eq!(output_lines, input_lines);
    }

    #[tokio::test]
    async fn cloudwatch_insert_out_of_range_timestamp() {
        trace_init();

        ensure_group().await;

        let stream_name = gen_name();
        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from(stream_name.as_str()).unwrap(),
            group_name: Template::try_from(GROUP_NAME).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000"),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let now = chrono::Utc::now();

        let mut input_lines = random_lines(100);
        let mut events = Vec::new();
        let mut lines = Vec::new();

        let mut add_event = |offset: chrono::Duration| {
            let line = input_lines.next().unwrap();
            let mut event = Event::from(line.clone());
            event
                .as_mut_log()
                .insert(log_schema().timestamp_key(), now + offset);
            events.push(event);
            line
        };

        // Too old event (> 14 days)
        add_event(Duration::days(-15));
        // Too new event (> 2 hours)
        add_event(Duration::minutes(125));
        // Right now and future in +2h limit
        lines.push(add_event(Duration::zero()));
        lines.push(add_event(Duration::hours(1)));
        lines.push(add_event(Duration::minutes(110)));
        // In 14 days limit
        lines.push(add_event(Duration::days(-1)));
        lines.push(add_event(Duration::days(-13)));

        sink.run(stream::iter(events)).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: stream_name,
            log_group_name: GROUP_NAME.into(),
            start_time: Some((now - Duration::days(30)).timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, lines);
    }

    #[tokio::test]
    async fn cloudwatch_dynamic_group_and_stream_creation() {
        trace_init();

        let stream_name = gen_name();
        let group_name = gen_name();

        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from(stream_name.as_str()).unwrap(),
            group_name: Template::try_from(group_name.as_str()).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000"),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11, None);
        sink.run(events).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: stream_name,
            log_group_name: group_name,
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[tokio::test]
    async fn cloudwatch_insert_log_event_batched() {
        trace_init();

        ensure_group().await;

        let stream_name = gen_name();
        let group_name = gen_name();

        let mut batch = BatchConfig::default();
        batch.max_events = Some(2);

        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from(stream_name.as_str()).unwrap(),
            group_name: Template::try_from(group_name.as_str()).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000"),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch,
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11, None);
        let mut events = events.map(Ok);
        let _ = sink.into_sink().send_all(&mut events).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: stream_name,
            log_group_name: group_name,
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[tokio::test]
    async fn cloudwatch_insert_log_event_partitioned() {
        trace_init();

        ensure_group().await;

        let stream_name = gen_name();
        let config = CloudwatchLogsSinkConfig {
            group_name: Template::try_from(GROUP_NAME).unwrap(),
            stream_name: Template::try_from(format!("{}-{{{{key}}}}", stream_name)).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000"),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, _events) = random_lines_with_stream(100, 10, None);

        let events = input_lines
            .clone()
            .into_iter()
            .enumerate()
            .map(|(i, e)| {
                let mut event = Event::from(e);
                let stream = format!("{}", (i % 2));
                event.as_mut_log().insert("key", stream);
                event
            })
            .collect::<Vec<_>>();
        sink.run(stream::iter(events)).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: format!("{}-0", stream_name),
            log_group_name: GROUP_NAME.into(),
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();
        let events = response.events.unwrap();
        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();
        let expected_output = input_lines
            .clone()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 0)
            .map(|(_, e)| e)
            .collect::<Vec<_>>();

        assert_eq!(output_lines, expected_output);

        let request = GetLogEventsRequest {
            log_stream_name: format!("{}-1", stream_name),
            log_group_name: GROUP_NAME.into(),
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();
        let events = response.events.unwrap();
        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();
        let expected_output = input_lines
            .clone()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, e)| e)
            .collect::<Vec<_>>();

        assert_eq!(output_lines, expected_output);
    }

    #[tokio::test]
    async fn cloudwatch_healthcheck() {
        trace_init();

        ensure_group().await;

        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from("test-stream").unwrap(),
            group_name: Template::try_from(GROUP_NAME).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000"),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let client = config.create_client(&ProxyConfig::default()).unwrap();
        healthcheck(config, client).await.unwrap();
    }

    fn create_client_test() -> CloudWatchLogsClient {
        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };

        let proxy = ProxyConfig::default();
        let client = rusoto::client(&proxy).unwrap();
        let creds = rusoto::AwsCredentialsProvider::new(&region, None).unwrap();
        CloudWatchLogsClient::new_with(client, creds, region)
    }

    async fn ensure_group() {
        let client = create_client_test();
        let req = CreateLogGroupRequest {
            log_group_name: GROUP_NAME.into(),
            ..Default::default()
        };
        let _ = client.create_log_group(req).await;
    }

    fn gen_name() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}

use std::convert::TryFrom;

use aws_config::Region;
use aws_sdk_cloudwatchlogs::Client as CloudwatchLogsClient;
use chrono::Duration;
use futures::{stream, StreamExt};
use similar_asserts::assert_eq;
use vector_lib::codecs::TextSerializerConfig;
use vector_lib::lookup;

use super::*;
use crate::aws::create_client;
use crate::aws::{AwsAuthentication, RegionOrEndpoint};
use crate::sinks::aws_cloudwatch_logs::config::CloudwatchLogsClientBuilder;
use crate::{
    config::{log_schema, ProxyConfig, SinkConfig, SinkContext},
    event::{Event, LogEvent, Value},
    sinks::util::BatchConfig,
    template::Template,
    test_util::{
        components::{run_and_assert_sink_compliance, AWS_SINK_TAGS},
        random_lines, random_lines_with_stream, random_string, trace_init,
    },
};

const GROUP_NAME: &str = "vector-cw";

fn watchlogs_address() -> String {
    std::env::var("WATCHLOGS_ADDRESS").unwrap_or_else(|_| "http://localhost:6000".into())
}

#[tokio::test]
async fn cloudwatch_insert_log_event() {
    trace_init();

    ensure_group().await;

    let stream_name = gen_name();
    let config = CloudwatchLogsSinkConfig {
        stream_name: Template::try_from(stream_name.as_str()).unwrap(),
        group_name: Template::try_from(GROUP_NAME).unwrap(),
        region: RegionOrEndpoint::with_both("us-east-1", watchlogs_address().as_str()),
        encoding: TextSerializerConfig::default().into(),
        create_missing_group: true,
        create_missing_stream: true,
        retention: Default::default(),
        compression: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        tls: Default::default(),
        assume_role: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
    };

    let (sink, _) = config.build(SinkContext::default()).await.unwrap();

    let timestamp = chrono::Utc::now();

    let (input_lines, events) = random_lines_with_stream(100, 11, None);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    let response = create_client_test()
        .await
        .get_log_events()
        .log_stream_name(stream_name)
        .log_group_name(GROUP_NAME)
        .start_time(timestamp.timestamp_millis())
        .send()
        .await
        .unwrap();

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
        region: RegionOrEndpoint::with_both("us-east-1", watchlogs_address().as_str()),
        encoding: TextSerializerConfig::default().into(),
        create_missing_group: true,
        create_missing_stream: true,
        retention: Default::default(),
        compression: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        tls: Default::default(),
        assume_role: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
    };

    let (sink, _) = config.build(SinkContext::default()).await.unwrap();

    let timestamp = chrono::Utc::now() - Duration::days(1);

    let (mut input_lines, events) = random_lines_with_stream(100, 11, None);

    // add a historical timestamp to all but the first event, to simulate
    // out-of-order timestamps.
    let mut doit = false;
    let events = events.map(move |mut events| {
        if doit {
            let timestamp = chrono::Utc::now() - Duration::days(1);

            events.iter_logs_mut().for_each(|log| {
                log.insert(
                    (
                        lookup::PathPrefix::Event,
                        log_schema().timestamp_key().unwrap(),
                    ),
                    Value::Timestamp(timestamp),
                );
            });
        }
        doit = true;

        events
    });
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    let response = create_client_test()
        .await
        .get_log_events()
        .log_stream_name(stream_name)
        .log_group_name(GROUP_NAME)
        .start_time(timestamp.timestamp_millis())
        .send()
        .await
        .unwrap();

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
        region: RegionOrEndpoint::with_both("us-east-1", watchlogs_address().as_str()),
        encoding: TextSerializerConfig::default().into(),
        create_missing_group: true,
        create_missing_stream: true,
        retention: Default::default(),
        compression: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        tls: Default::default(),
        assume_role: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
    };

    let (sink, _) = config.build(SinkContext::default()).await.unwrap();

    let now = chrono::Utc::now();

    let mut input_lines = random_lines(100);
    let mut events = Vec::new();
    let mut lines = Vec::new();

    let mut add_event = |offset: Duration| {
        let line = input_lines.next().unwrap();
        let mut event = LogEvent::from(line.clone());
        event.insert(
            log_schema().timestamp_key_target_path().unwrap(),
            now + offset,
        );
        events.push(Event::Log(event));
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

    run_and_assert_sink_compliance(sink, stream::iter(events), &AWS_SINK_TAGS).await;

    let response = create_client_test()
        .await
        .get_log_events()
        .log_stream_name(stream_name)
        .log_group_name(GROUP_NAME)
        .start_time((now - Duration::days(30)).timestamp_millis())
        .send()
        .await
        .unwrap();

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
        region: RegionOrEndpoint::with_both("us-east-1", watchlogs_address().as_str()),
        encoding: TextSerializerConfig::default().into(),
        create_missing_group: true,
        create_missing_stream: true,
        retention: Default::default(),
        compression: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        tls: Default::default(),
        assume_role: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
    };

    let (sink, _) = config.build(SinkContext::default()).await.unwrap();

    let timestamp = chrono::Utc::now();

    let (input_lines, events) = random_lines_with_stream(100, 11, None);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    let response = create_client_test()
        .await
        .get_log_events()
        .log_stream_name(stream_name)
        .log_group_name(group_name)
        .start_time(timestamp.timestamp_millis())
        .send()
        .await
        .unwrap();

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
        region: RegionOrEndpoint::with_both("us-east-1", watchlogs_address().as_str()),
        encoding: TextSerializerConfig::default().into(),
        create_missing_group: true,
        create_missing_stream: true,
        retention: Default::default(),
        compression: Default::default(),
        batch,
        request: Default::default(),
        tls: Default::default(),
        assume_role: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
    };

    let (sink, _) = config.build(SinkContext::default()).await.unwrap();

    let timestamp = chrono::Utc::now();

    let (input_lines, events) = random_lines_with_stream(100, 11, None);
    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    let response = create_client_test()
        .await
        .get_log_events()
        .log_stream_name(stream_name)
        .log_group_name(group_name)
        .start_time(timestamp.timestamp_millis())
        .send()
        .await
        .unwrap();

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
        region: RegionOrEndpoint::with_both("us-east-1", watchlogs_address().as_str()),
        encoding: TextSerializerConfig::default().into(),
        create_missing_group: true,
        create_missing_stream: true,
        retention: Default::default(),
        compression: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        tls: Default::default(),
        assume_role: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
    };

    let (sink, _) = config.build(SinkContext::default()).await.unwrap();

    let timestamp = chrono::Utc::now();

    let (input_lines, _events) = random_lines_with_stream(100, 10, None);

    let events = input_lines
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let mut event = LogEvent::from(e);
            let stream = (i % 2).to_string();
            event.insert("key", stream);
            Event::Log(event)
        })
        .collect::<Vec<_>>();
    run_and_assert_sink_compliance(sink, stream::iter(events), &AWS_SINK_TAGS).await;

    let response = create_client_test()
        .await
        .get_log_events()
        .log_stream_name(format!("{}-0", stream_name))
        .log_group_name(GROUP_NAME)
        .start_time(timestamp.timestamp_millis())
        .send()
        .await
        .unwrap();
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

    let response = create_client_test()
        .await
        .get_log_events()
        .log_stream_name(format!("{}-1", stream_name))
        .log_group_name(GROUP_NAME)
        .start_time(timestamp.timestamp_millis())
        .send()
        .await
        .unwrap();

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
    use super::healthcheck::healthcheck;

    ensure_group().await;

    let config = CloudwatchLogsSinkConfig {
        stream_name: Template::try_from("test-stream").unwrap(),
        group_name: Template::try_from(GROUP_NAME).unwrap(),
        region: RegionOrEndpoint::with_both("us-east-1", watchlogs_address().as_str()),
        encoding: TextSerializerConfig::default().into(),
        create_missing_group: true,
        create_missing_stream: true,
        retention: Default::default(),
        compression: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        tls: Default::default(),
        assume_role: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
    };

    let client = config.create_client(&ProxyConfig::default()).await.unwrap();
    healthcheck(config, client).await.unwrap();
}

async fn create_client_test() -> CloudwatchLogsClient {
    let auth = AwsAuthentication::test_auth();
    let region = Some(Region::new("us-east-1"));
    let endpoint = Some(watchlogs_address());
    let proxy = ProxyConfig::default();

    create_client::<CloudwatchLogsClientBuilder>(&auth, region, endpoint, &proxy, &None)
        .await
        .unwrap()
}

async fn ensure_group() {
    let client = create_client_test().await;
    _ = client
        .create_log_group()
        .log_group_name(GROUP_NAME)
        .send()
        .await;
}

fn gen_name() -> String {
    format!("test-{}", random_string(10).to_lowercase())
}

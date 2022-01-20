#![cfg(feature = "aws-cloudwatch-logs-integration-tests")]
#![cfg(test)]

use std::convert::TryFrom;

use chrono::Duration;
use futures::StreamExt;
use pretty_assertions::assert_eq;
use rusoto_core::Region;
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, CreateLogGroupRequest, GetLogEventsRequest,
};

use super::*;
use crate::{
    aws::{rusoto, rusoto::RegionOrEndpoint},
    config::{log_schema, ProxyConfig, SinkConfig, SinkContext},
    event::{Event, Value},
    sinks::util::{encoding::StandardEncodings, BatchConfig},
    template::Template,
    test_util::{random_lines, random_lines_with_stream, random_string, trace_init},
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
        region: RegionOrEndpoint::with_endpoint(watchlogs_address().as_str()),
        encoding: StandardEncodings::Text.into(),
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
        region: RegionOrEndpoint::with_endpoint(watchlogs_address().as_str()),
        encoding: StandardEncodings::Text.into(),
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
    let events = events.map(move |mut events| {
        if doit {
            let timestamp = chrono::Utc::now() - chrono::Duration::days(1);

            events.for_each_log(|log| {
                log.insert(log_schema().timestamp_key(), Value::Timestamp(timestamp));
            });
        }
        doit = true;

        events
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
        region: RegionOrEndpoint::with_endpoint(watchlogs_address().as_str()),
        encoding: StandardEncodings::Text.into(),
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

    sink.run_events(events).await.unwrap();

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
        region: RegionOrEndpoint::with_endpoint(watchlogs_address().as_str()),
        encoding: StandardEncodings::Text.into(),
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
        region: RegionOrEndpoint::with_endpoint(watchlogs_address().as_str()),
        encoding: StandardEncodings::Text.into(),
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
    let stream = sink.into_stream(); //.send_all(&mut events).await.unwrap();
    stream.run(events.map(Into::into).boxed()).await.unwrap();

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
        region: RegionOrEndpoint::with_endpoint(watchlogs_address().as_str()),
        encoding: StandardEncodings::Text.into(),
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
    sink.run_events(events).await.unwrap();

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
    use super::healthcheck::healthcheck;

    ensure_group().await;

    let config = CloudwatchLogsSinkConfig {
        stream_name: Template::try_from("test-stream").unwrap(),
        group_name: Template::try_from(GROUP_NAME).unwrap(),
        region: RegionOrEndpoint::with_endpoint(watchlogs_address().as_str()),
        encoding: StandardEncodings::Text.into(),
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
        endpoint: watchlogs_address(),
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

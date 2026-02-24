use chrono::{Local, Utc};
use indoc::formatdoc;
use regex::Regex;
use std::collections::BTreeMap;
use vector_common::finalization::BatchStatus;
use vector_core::event::{BatchNotifier, Event, LogEvent, Value};
use vector_core::sink::VectorSink;

use crate::config::SinkConfig;
use crate::sinks::gcp::bigquery::BigqueryConfig;
use crate::test_util::components::{run_and_assert_sink_compliance, SINK_TAGS};
use crate::test_util::{generate_events_with_stream, random_string, trace_init};

/// An event generator that can be used with `generate_events_with_stream`
fn event_generator(index: usize) -> Event {
    let now = Local::now().with_timezone(&Utc);
    let value = Value::Object(BTreeMap::from([
        ("time".into(), Value::Timestamp(now)),
        ("count".into(), Value::Integer(index as i64)),
        ("message".into(), Value::from(random_string(64))),
        ("user".into(), Value::from("Bob".to_string())),
    ]));
    Event::Log(LogEvent::from_parts(value, Default::default()))
}

/// Create a BigquerySink from the local environment
async fn create_sink() -> VectorSink {
    let desc_file = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("lib/codecs/tests/data/protobuf/integration.desc")
        .to_string_lossy()
        .into_owned();
    let message_type = "test.Integration";
    let write_stream = std::env::var("TEST_GCP_BIGQUERY_WRITE_STREAM")
        .expect("couldn't find the BigQuery write stream in environment variables");
    let re =
        Regex::new("^projects/([^/]+)/datasets/([^/]+)/tables/([^/]+)/streams/_default$").unwrap();
    let captures = re
        .captures(&write_stream)
        .expect("malformed BigQuery write stream in environment variables");
    let project = captures.get(1).unwrap().as_str();
    let dataset = captures.get(2).unwrap().as_str();
    let table = captures.get(3).unwrap().as_str();
    let config = formatdoc! {r#"
        project = "{project}"
        dataset = "{dataset}"
        table = "{table}"
        encoding.protobuf.desc_file = "{desc_file}"
        encoding.protobuf.message_type = "{message_type}"
    "#};
    let (bigquery_config, cx) =
        crate::sinks::util::test::load_sink::<BigqueryConfig>(&config).unwrap();
    let (bigquery_sink, bigquery_healthcheck) = bigquery_config.build(cx).await.unwrap();
    bigquery_healthcheck
        .await
        .expect("BigQuery healthcheck failed");
    bigquery_sink
}

#[tokio::test]
async fn gcp_bigquery_sink() {
    trace_init();
    let bigquery_sink = create_sink().await;
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (_, events) = generate_events_with_stream(event_generator, 10, Some(batch));
    run_and_assert_sink_compliance(bigquery_sink, events, &SINK_TAGS).await;
    assert_eq!(Ok(BatchStatus::Delivered), receiver.try_recv());
}

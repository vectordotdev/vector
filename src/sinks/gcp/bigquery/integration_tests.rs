use chrono::Utc;
use indoc::formatdoc;
use regex::Regex;
use std::collections::BTreeMap;
use vector_common::finalization::BatchStatus;
use vector_lib::event::{BatchNotifier, Event, LogEvent, Value};
use vector_lib::sink::VectorSink;

use crate::config::SinkConfig;
use crate::sinks::gcp::bigquery::BigqueryConfig;
use crate::test_util::components::{SINK_TAGS, run_and_assert_sink_compliance};
use crate::test_util::{generate_events_with_stream, random_string, trace_init};

struct SinkParams {
    project: String,
    dataset: String,
    table: String,
    endpoint: String,
    skip_authentication: bool,
}

impl SinkParams {
    fn from_env() -> Self {
        if let Ok(write_stream) = std::env::var("TEST_GCP_BIGQUERY_WRITE_STREAM") {
            // Real BigQuery write stream is specified. We'll need to use real GCP credentials.
            let re =
                Regex::new("^projects/([^/]+)/datasets/([^/]+)/tables/([^/]+)/streams/_default$")
                    .unwrap();
            let captures = re
                .captures(&write_stream)
                .expect("TEST_GCP_BIGQUERY_WRITE_STREAM is not a valid write stream path");
            Self {
                project: captures[1].to_string(),
                dataset: captures[2].to_string(),
                table: captures[3].to_string(),
                endpoint: crate::gcp::BIGQUERY_STORAGE_URL.to_string(),
                skip_authentication: false,
            }
        } else {
            // Otherwise, use the local emulator.
            Self {
                project: "testproject".to_string(),
                dataset: "testdataset".to_string(),
                table: "integration".to_string(),
                endpoint: std::env::var("BIGQUERY_EMULATOR_ADDRESS")
                    .unwrap_or_else(|_| "http://localhost:9060".to_string()),
                skip_authentication: true,
            }
        }
    }
}

/// An event generator that can be used with `generate_events_with_stream`
fn event_generator(index: usize) -> Event {
    let now = Utc::now();
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
    let desc_file_toml = toml::Value::String(desc_file).to_string();
    let SinkParams {
        project,
        dataset,
        table,
        endpoint,
        skip_authentication,
    } = SinkParams::from_env();
    let auth_line = if skip_authentication {
        "skip_authentication = true".to_string()
    } else {
        String::new()
    };
    let config = formatdoc! {r#"
        project = "{project}"
        dataset = "{dataset}"
        table = "{table}"
        endpoint = "{endpoint}"
        {auth_line}
        encoding.protobuf.desc_file = {desc_file_toml}
        encoding.protobuf.message_type = "test.Integration"
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

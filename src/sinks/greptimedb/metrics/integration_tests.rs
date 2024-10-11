use crate::{
    config::{SinkConfig, SinkContext},
    sinks::{greptimedb::metrics::config::GreptimeDBMetricsConfig, util::test::load_sink},
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        trace_init,
    },
};
use chrono::{DateTime, Duration, Utc};
use futures::stream;
use vector_lib::{
    event::{Event, Metric, MetricKind, MetricValue},
    metric_tags,
};

#[tokio::test]
async fn test_greptimedb_sink() {
    trace_init();
    let cfg = format!(
        r#"endpoint= "{}"
"#,
        std::env::var("GREPTIMEDB_ENDPOINT").unwrap_or_else(|_| "localhost:4001".to_owned())
    );

    let (config, _) = load_sink::<GreptimeDBMetricsConfig>(&cfg).unwrap();
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let query_client = query_client();

    // Drop the table and data inside
    let _ = query_client
        .get(&format!(
            "{}/v1/sql",
            std::env::var("GREPTIMEDB_HTTP").unwrap_or_else(|_| "http://localhost:4000".to_owned())
        ))
        .query(&[("sql", "DROP TABLE ns_my_counter")])
        .send()
        .await
        .unwrap();

    let base_time = Utc::now();
    let events: Vec<_> = (0..10)
        .map(|idx| create_event("my_counter", idx, base_time))
        .collect();
    run_and_assert_sink_compliance(sink, stream::iter(events), &SINK_TAGS).await;

    let query_response = query_client
        .get(&format!(
            "{}/v1/sql",
            std::env::var("GREPTIMEDB_HTTP").unwrap_or_else(|_| "http://localhost:4000".to_owned())
        ))
        .query(&[("sql", "SELECT region, val FROM ns_my_counter")])
        .send()
        .await
        .unwrap()
        .text()
        .await
        .expect("Fetch json from greptimedb failed");
    let result: serde_json::Value =
        serde_json::from_str(&query_response).expect("Invalid json returned from greptimedb query");
    assert_eq!(
        result
            .pointer("/output/0/records/rows")
            .and_then(|v| v.as_array())
            .expect("Error getting greptimedb response array")
            .len(),
        10
    )
}

#[tokio::test]
async fn test_greptimedb_sink_new_naming() {
    trace_init();
    let cfg = format!(
        r#"endpoint= "{}"
new_naming = true
"#,
        std::env::var("GREPTIMEDB_ENDPOINT").unwrap_or_else(|_| "localhost:4001".to_owned())
    );

    let (config, _) = load_sink::<GreptimeDBMetricsConfig>(&cfg).unwrap();
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let query_client = query_client();

    // Drop the table and data inside
    let _ = query_client
        .get(&format!(
            "{}/v1/sql",
            std::env::var("GREPTIMEDB_HTTP").unwrap_or_else(|_| "http://localhost:4000".to_owned())
        ))
        .query(&[("sql", "DROP TABLE ns_my_counter")])
        .send()
        .await
        .unwrap();

    let base_time = Utc::now();
    let events: Vec<_> = (0..10)
        .map(|idx| create_event("my_counter_new", idx, base_time))
        .collect();
    run_and_assert_sink_compliance(sink, stream::iter(events), &SINK_TAGS).await;

    let query_response = query_client
        .get(&format!(
            "{}/v1/sql",
            std::env::var("GREPTIMEDB_HTTP").unwrap_or_else(|_| "http://localhost:4000".to_owned())
        ))
        .query(&[(
            "sql",
            "SELECT region, greptime_value, greptime_timestamp FROM ns_my_counter_new",
        )])
        .send()
        .await
        .unwrap()
        .text()
        .await
        .expect("Fetch json from greptimedb failed");
    let result: serde_json::Value =
        serde_json::from_str(&query_response).expect("Invalid json returned from greptimedb query");
    assert_eq!(
        result
            .pointer("/output/0/records/rows")
            .and_then(|v| v.as_array())
            .expect("Error getting greptimedb response array")
            .len(),
        10
    )
}

fn query_client() -> reqwest::Client {
    reqwest::Client::builder().build().unwrap()
}

fn create_event(name: &str, i: i32, base_time: DateTime<Utc>) -> Event {
    Event::Metric(
        Metric::new(
            name.to_owned(),
            MetricKind::Incremental,
            MetricValue::Counter { value: i as f64 },
        )
        .with_namespace(Some("ns"))
        .with_tags(Some(metric_tags!(
            "region" => "us-west-1",
            "production" => "true",
        )))
        .with_timestamp(Some(base_time + Duration::seconds(i as i64))),
    )
}

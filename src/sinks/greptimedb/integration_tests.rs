use vector_core::event::MetricKind;
use vector_core::metric_tags;

use crate::sinks::util::test::load_sink;
use crate::{
    config::{SinkConfig, SinkContext},
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        trace_init,
    },
};

use super::*;

#[tokio::test]
async fn test_greptimedb_sink() {
    trace_init();
    let cfg = format!(
        r#"endpoint= "{}"
"#,
        std::env::var("GREPTIMEDB_ENDPOINT").unwrap_or_else(|_| "localhost:4001".to_owned())
    );

    let (config, _) = load_sink::<GreptimeDBConfig>(&cfg).unwrap();
    let (sink, _hc) = config.build(SinkContext::new_test()).await.unwrap();

    let events: Vec<_> = (0..10).map(create_event).collect();
    run_and_assert_sink_compliance(sink, stream::iter(events), &SINK_TAGS).await;

    let query_client = query_client();
    let query_response = query_client
        .get(&format!(
            "{}/v1/sql",
            std::env::var("GREPTIMEDB_HTTP").unwrap_or_else(|_| "http://localhost:4000".to_owned())
        ))
        .query(&[("sql", "SELECT region, value FROM ns_my_counter")])
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

fn create_event(i: i32) -> Event {
    Event::Metric(
        Metric::new(
            "my_counter".to_owned(),
            MetricKind::Incremental,
            MetricValue::Counter { value: i as f64 },
        )
        .with_namespace(Some("ns"))
        .with_tags(Some(metric_tags!(
            "region" => "us-west-1",
            "production" => "true",
        )))
        .with_timestamp(Some(Utc::now())),
    )
}

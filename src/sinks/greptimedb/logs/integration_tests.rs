use chrono::{DateTime, Utc};
use futures::stream;
use vector_lib::event::{Event, LogEvent};

use crate::{
    config::{SinkConfig, SinkContext},
    sinks::util::test::load_sink,
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        trace_init,
    },
};

use crate::sinks::greptimedb::logs::config::GreptimeDBLogsConfig;

#[tokio::test]
async fn test_greptimedb_logs_sink() {
    trace_init();
    let greptimedb_http_endpoint =
        std::env::var("GREPTIMEDB_HTTP").unwrap_or_else(|_| "http://localhost:4000".to_owned());
    let cfg = format!(
        r#"endpoint= "{}"
table = "logs"
dbname = "public"
pipeline_name = "test"
"#,
        &greptimedb_http_endpoint
    );

    // This is a minimal config that is required to run the sink
    let pipeline = "processors:\n  - date:\n      field: timestamp\n      formats:\n        - \"%Y-%m-%dT%H:%M:%S%.9fZ\"\n      ignore_missing: true\n\ntransform:\n  - fields:\n      - name\n      - namespace\n      - kind\n      - message   \n    type: string\n  - field: timestamp\n    type: time\n    index: timestamp";

    let (config, _) = load_sink::<GreptimeDBLogsConfig>(&cfg).unwrap();
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let client = GreptimeClient::new(greptimedb_http_endpoint.clone());

    // Create a pipeline
    client.create_pipeline("test", pipeline).await;

    // Drop the table and data inside
    client.query("DROP TABLE logs").await;

    let base_time = Utc::now();
    let events: Vec<_> = (0..10).map(|idx| create_event(idx, base_time)).collect();

    run_and_assert_sink_compliance(sink, stream::iter(events), &SINK_TAGS).await;

    let query_response = client.query("SELECT * FROM logs").await;
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

struct GreptimeClient {
    client: reqwest::Client,
    endpoint: String,
}

impl GreptimeClient {
    fn new(endpoint: String) -> Self {
        GreptimeClient {
            client: reqwest::Client::builder().build().unwrap(),
            endpoint,
        }
    }

    async fn create_pipeline(&self, pipeline_name: &str, pipeline_content: &str) {
        self.client
            .post(&format!(
                "{}/v1/events/pipelines/{}",
                self.endpoint, pipeline_name
            ))
            .header("Content-Type", "application/x-yaml")
            .body(String::from(pipeline_content))
            .send()
            .await
            .unwrap();
    }

    async fn query(&self, sql: &str) -> String {
        self.client
            .get(&format!("{}/v1/sql", self.endpoint))
            .query(&[("sql", sql)])
            .send()
            .await
            .unwrap()
            .text()
            .await
            .expect("Fetch json from greptimedb failed")
    }
}

fn create_event(i: i32, base_time: DateTime<Utc>) -> Event {
    let mut event = LogEvent::default();
    event.insert("message", format!("test message {}", i));
    event.insert("timestamp", base_time);
    event.insert("name", "test");
    event.insert("namespace", "default");
    event.insert("kind", "test");
    Event::Log(event)
}

use std::env;

use chrono::{DateTime, Duration, Utc};
use futures::stream;
use serde::{Deserialize, Serialize};
use vector_lib::event::{BatchNotifier, BatchStatus, Event, LogEvent};

use super::*;
use crate::{
    config::{SinkConfig, SinkContext},
    sinks::axiom::config::UrlOrRegion,
    test_util::components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
};

#[tokio::test]
async fn axiom_logs_put_data() {
    let client = reqwest::Client::new();
    let url = env::var("AXIOM_URL").unwrap();
    let token = env::var("AXIOM_TOKEN").expect("AXIOM_TOKEN environment variable to be set");
    assert!(!token.is_empty(), "$AXIOM_TOKEN required");
    let dataset = env::var("AXIOM_DATASET").unwrap();
    let org_id = env::var("AXIOM_ORG_ID").unwrap();

    let cx = SinkContext::default();

    let config = AxiomConfig {
        endpoint: UrlOrRegion {
            url: Some(url.clone()),
            region: None,
        },
        token: token.clone().into(),
        dataset: dataset.clone(),
        org_id: Some(org_id.clone()),
        ..Default::default()
    };

    // create unique test id so tests can run in parallel
    let test_id = uuid::Uuid::new_v4().to_string();

    let (sink, _) = config.build(cx).await.unwrap();

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();

    let mut event1 = LogEvent::from("message_1").with_batch_notifier(&batch);
    event1.insert("host", "aws.cloud.eur");
    event1.insert("source_type", "file");
    event1.insert("test_id", test_id.clone());

    let mut event2 = LogEvent::from("message_2").with_batch_notifier(&batch);
    event2.insert("host", "aws.cloud.eur");
    event2.insert("source_type", "file");
    event2.insert("test_id", test_id.clone());

    drop(batch);

    let events = vec![Event::Log(event1), Event::Log(event2)];

    run_and_assert_sink_compliance(sink, stream::iter(events), &HTTP_SINK_TAGS).await;

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    #[derive(Serialize)]
    struct QueryRequest {
        apl: String,
        #[serde(rename = "endTime")]
        end_time: DateTime<Utc>,
        #[serde(rename = "startTime")]
        start_time: DateTime<Utc>,
        // ...
    }

    #[derive(Deserialize, Debug)]
    struct QueryResponseMatch {
        data: serde_json::Value,
        // ...
    }

    #[derive(Deserialize, Debug)]
    struct QueryResponse {
        matches: Vec<QueryResponseMatch>,
        // ...
    }

    let query_req = QueryRequest {
        apl: format!(
            "['{dataset}'] | where test_id == '{test_id}' | order by _time desc | limit 2"
        ),
        start_time: Utc::now() - Duration::minutes(10),
        end_time: Utc::now() + Duration::minutes(10),
    };
    let query_res: QueryResponse = client
        .post(format!("{url}/v1/datasets/_apl?format=legacy"))
        .header("X-Axiom-Org-Id", org_id)
        .header("Authorization", format!("Bearer {token}"))
        .json(&query_req)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(2, query_res.matches.len());

    let fst = match query_res.matches[0].data {
        serde_json::Value::Object(ref obj) => obj,
        _ => panic!("Unexpected value, expected object"),
    };
    // Note that we order descending, so message_2 comes first
    assert_eq!("message_2", fst.get("message").unwrap().as_str().unwrap());

    let snd = match query_res.matches[1].data {
        serde_json::Value::Object(ref obj) => obj,
        _ => panic!("Unexpected value, expected object"),
    };
    assert_eq!("message_1", snd.get("message").unwrap().as_str().unwrap());
}

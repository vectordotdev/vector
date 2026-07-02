use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use vector::test_util::trace_init;

const DEFAULT_EVENT_COUNT: usize = 50;
const DEFAULT_RETAINED_TAIL: usize = 3;
const MAX_RETRIES: usize = 30;
const WAIT_INTERVAL: Duration = Duration::from_secs(1);

fn vector_endpoint() -> String {
    std::env::var("VECTOR_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
}

fn collector_endpoint() -> String {
    std::env::var("COLLECTOR_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8081".to_string())
}

fn event_count() -> usize {
    std::env::var("EVENT_COUNT")
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .expect("EVENT_COUNT should be an unsigned integer.")
        })
        .unwrap_or(DEFAULT_EVENT_COUNT)
}

fn retained_tail() -> usize {
    std::env::var("RETAINED_TAIL")
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .expect("RETAINED_TAIL should be an unsigned integer.")
        })
        .unwrap_or(DEFAULT_RETAINED_TAIL)
}

#[derive(Debug, Deserialize)]
struct CollectorResponse {
    ids: Vec<usize>,
}

async fn get_collected_ids(client: &Client, collector: &str) -> Vec<usize> {
    client
        .get(format!("{collector}/ids"))
        .send()
        .await
        .expect("getting collected ids should succeed")
        .error_for_status()
        .expect("collector ids response should be successful")
        .json::<CollectorResponse>()
        .await
        .expect("collector ids response should be JSON")
        .ids
}

#[tokio::test]
async fn retains_newest_events_when_memory_buffer_is_full() {
    trace_init();

    let client = Client::new();
    let vector = vector_endpoint();
    let collector = collector_endpoint();
    let event_count = event_count();
    let retained_tail = retained_tail();

    for id in 0..event_count {
        client
            .post(&vector)
            .json(&json!({
                "id": id,
                "message": format!("event-{id}"),
            }))
            .send()
            .await
            .unwrap_or_else(|_| panic!("sending event {id} to Vector should succeed"))
            .error_for_status()
            .unwrap_or_else(|_| panic!("Vector should accept event {id}"));
    }

    let expected_tail = (event_count - retained_tail..event_count).collect::<Vec<_>>();

    let mut ids = Vec::new();
    for _ in 0..MAX_RETRIES {
        ids = get_collected_ids(&client, &collector).await;
        if expected_tail.iter().all(|id| ids.contains(id)) {
            break;
        }
        tokio::time::sleep(WAIT_INTERVAL).await;
    }

    assert!(
        expected_tail.iter().all(|id| ids.contains(id)),
        "collector should receive the newest buffered events; expected tail {expected_tail:?}, got {ids:?}"
    );
    assert!(
        ids.len() < event_count,
        "drop_oldest should shed some events under forced sink backpressure; got all {event_count} events: {ids:?}"
    );
}

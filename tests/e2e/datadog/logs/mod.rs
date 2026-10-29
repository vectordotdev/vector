use std::time::Duration;

use serde_json::Value;
use tokio::time::{Instant, sleep};
use tracing::info;
use vector::test_util::trace_init;

use super::*;

const LOGS_ENDPOINT: &str = "/api/v2/logs";

fn expected_log_events() -> usize {
    std::env::var("EXPECTED_LOG_EVENTS").map_or(1000, |n_expected| {
        n_expected
            .parse::<usize>()
            .expect("EXPECTED_LOG_EVENTS should be an unsigned integer.")
    })
}

// Asserts that each log event has the hostname and timestamp fields, and
// Removes them from the log so that comparison can more easily be made.
// @return the number of log entries in the payload.
fn assert_timestamp_hostname(payloads: &mut [Value]) -> usize {
    let mut n_log_events = 0;

    for payload_array in payloads {
        for log_val in payload_array.as_array_mut().expect("should be array") {
            n_log_events += 1;

            let log = log_val
                .as_object_mut()
                .expect("log entries should be objects");

            // timestamp is available in the flog generated logs as a datetime but
            // there does not appear to be a way to add a custom parser in the Agent
            // to handle it.
            assert!(log.remove("timestamp").is_some());
            assert!(log.remove("hostname").is_some());
        }
    }

    n_log_events
}

// runs assertions that each set of payloads should be true to regardless
// of the pipeline
fn common_assertions(payloads: &mut [Value]) {
    assert!(!payloads.is_empty());

    let n_log_events = assert_timestamp_hostname(payloads);

    info!("log events received: {n_log_events}");

    assert_eq!(n_log_events, expected_log_events());
}

// reduces the payload down to just the log data
fn reduce_to_data(payloads: Vec<FakeIntakePayload<Value>>) -> Vec<Value> {
    payloads
        .into_iter()
        .map(|mut payload| payload.data.take())
        .collect()
}

fn count_log_events(payloads: &[Value]) -> usize {
    payloads
        .iter()
        .map(|payload| payload.as_array().map_or(0, Vec::len))
        .sum()
}

// Polls a fakeintake until it holds the expected number of log events or the deadline passes.
async fn get_log_payloads(address: String, deadline: Instant) -> Vec<Value> {
    loop {
        info!("getting log payloads from {address}");
        let mut payloads =
            get_fakeintake_payloads::<FakeIntakeResponseJson>(&address, LOGS_ENDPOINT)
                .await
                .payloads;

        // The logs endpoint receives an empty healthcheck payload in the beginning
        payloads.retain(|raw_payload| !raw_payload.data.as_array().unwrap().is_empty());
        let payloads = reduce_to_data(payloads);

        // Wait for at least the expected count. `common_assertions` still checks the exact count,
        // so duplicates fail there.
        if count_log_events(&payloads) >= expected_log_events() || Instant::now() >= deadline {
            break payloads;
        }
        sleep(Duration::from_secs(1)).await;
    }
}

#[tokio::test]
async fn validate() {
    trace_init();

    // Wait for workload data, not compilation time, to make both pipelines ready.
    // Leave time to report failures before nextest's two-minute test timeout.
    let deadline = Instant::now() + Duration::from_secs(90);

    info!("getting log payloads from agent-only pipeline");
    let mut agent_payloads = get_log_payloads(fake_intake_agent_address(), deadline).await;
    common_assertions(&mut agent_payloads);

    info!("getting log payloads from agent-vector pipeline");
    let mut vector_payloads = get_log_payloads(fake_intake_vector_address(), deadline).await;
    common_assertions(&mut vector_payloads);

    assert_eq!(agent_payloads, vector_payloads);
}

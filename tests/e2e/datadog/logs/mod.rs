use serde_json::Value;

use vector::test_util::trace_init;

use super::*;

const LOGS_ENDPOINT: &str = "/api/v2/logs";

fn expected_log_events() -> usize {
    std::env::var("EXPECTED_LOG_EVENTS")
        .map(|n_expected| {
            n_expected
                .parse::<usize>()
                .expect("EXPECTED_LOG_EVENTS should be an unsigned integer.")
        })
        .unwrap_or(1000)
}

// Asserts that each log event has the hostname and timestamp fields, and
// Removes them from the log so that comparison can more easily be made.
// @return the number of log entries in the payload.
fn assert_timestamp_hostname(payloads: &mut [Value]) -> usize {
    let mut n_log_events = 0;

    payloads.iter_mut().for_each(|payload_array| {
        payload_array
            .as_array_mut()
            .expect("should be array")
            .iter_mut()
            .for_each(|log_val| {
                n_log_events += 1;

                let log = log_val
                    .as_object_mut()
                    .expect("log entries should be objects");

                assert!(log.remove("timestamp").is_some());
                assert!(log.remove("hostname").is_some());
            })
    });

    n_log_events
}

// runs assertions that each set of payloads should be true to regardless
// of the pipeline
fn common_assertions(payloads: &mut [Value]) {
    assert!(payloads.len() > 0);

    let n_log_events = assert_timestamp_hostname(payloads);

    println!("log events received: {n_log_events}");

    assert!(n_log_events == expected_log_events());
}

#[tokio::test]
async fn validate() {
    trace_init();

    // a small sleep here is kind of hard to avoid. Regardless of dependencies flagged for the
    // containers, we need the events to flow between them.
    std::thread::sleep(std::time::Duration::from_secs(5));

    println!("getting log payloads from agent-only pipeline");
    let mut agent_payloads = get_payloads_agent(LOGS_ENDPOINT).await;

    common_assertions(&mut agent_payloads);

    println!("getting log payloads from agent-vector pipeline");
    let mut vector_payloads = get_payloads_vector(LOGS_ENDPOINT).await;

    common_assertions(&mut vector_payloads);

    assert_eq!(agent_payloads, vector_payloads);
}

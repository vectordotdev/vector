use serde_json::Value;
use std::{thread::sleep, time::Duration};

use vector::test_util::trace_init;

use super::*;

const LOGS_ENDPOIINT: &str = "/api/v2/logs";

fn expected_log_events() -> u32 {
    std::env::var("EXPECTED_LOG_EVENTS")
        .unwrap_or_else(|_| "1000".to_string())
        .parse::<u32>()
        .expect("EXPECTED_LOG_EVENTS should be an unsigned int")
}

// Asserts that each log event has the hostname and timestamp fields, and
// Removes them from the log so that comparison can more easily be made.
// @return the number of log entries in the payload.
fn assert_timestamp_hostname(payloads: &mut Vec<Value>) -> u32 {
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
fn common_assertions(payloads: &mut Vec<Value>) {
    //dbg!(&payloads);

    assert!(payloads.len() > 0);

    let n_log_events = assert_timestamp_hostname(payloads);

    println!("log events received: {n_log_events}");

    assert!(n_log_events == expected_log_events());
}

#[tokio::test]
async fn test_logs() {
    trace_init();

    // As it stands, we do still need a small sleep to allow the events to flow through.
    // There doesn't seem to be a great way to avoid this.
    sleep(Duration::from_secs(5));

    println!("getting log payloads from agent-only pipeline");
    let mut agent_payloads = get_payloads_agent(LOGS_ENDPOIINT).await;

    common_assertions(&mut agent_payloads);

    println!("getting log payloads from agent-vector pipeline");
    let mut vector_payloads = get_payloads_vector(LOGS_ENDPOIINT).await;

    common_assertions(&mut vector_payloads);

    assert_eq!(agent_payloads, vector_payloads);
}

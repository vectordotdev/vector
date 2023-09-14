use serde_json::Value;

use vector::test_util::trace_init;

use super::*;

const AGENT_DEFAULT_ENDPOINT: &str = "/api/v2/series";

// TODO the v1 endpoint is not compatible with fakeintake parsed
// payloads right now. we might need to change to use v2
const VECTOR_DEFAULT_ENDPOINT: &str = "/api/v1/series";

// runs assertions that each set of payloads should be true to regardless
// of the pipeline
fn common_assertions(payloads: &mut Vec<Value>) {
    assert!(payloads.len() > 0);

    println!("metric events received: {}", payloads.len());
}

#[tokio::test]
async fn validate() {
    trace_init();

    // TODO need to see if can configure the agent flush interval
    std::thread::sleep(std::time::Duration::from_secs(5));

    println!("getting log payloads from agent-only pipeline");
    let mut agent_payloads = get_payloads_agent(AGENT_DEFAULT_ENDPOINT).await;

    common_assertions(&mut agent_payloads);

    println!("getting log payloads from agent-vector pipeline");
    let mut vector_payloads = get_payloads_vector(VECTOR_DEFAULT_ENDPOINT).await;

    common_assertions(&mut vector_payloads);

    assert_eq!(agent_payloads, vector_payloads);
}

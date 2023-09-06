use vector::test_util::trace_init;

use super::*;

#[tokio::test]
async fn test_logs() {
    trace_init();

    let logs_endpoint = "/api/v2/logs";
    let _agent_payloads = get_payloads_agent(&logs_endpoint).await;

    dbg!(&_agent_payloads);

    let _vector_payloads = get_payloads_vector(&logs_endpoint).await;

    dbg!(&_vector_payloads);

    assert!(true);
}

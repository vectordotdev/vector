use vector::test_util::trace_init;

use std::{thread::sleep, time::Duration};

use super::*;

fn assert_timestamp_hostname(payloads: &mut Vec<serde_json::Value>) -> usize {
    let mut n_payloads = 0;
    payloads.iter_mut().for_each(|payload_array| {
        payload_array
            .as_array_mut()
            .expect("should be array")
            .iter_mut()
            .for_each(|payload| {
                n_payloads += 1;
                let obj = payload
                    .as_object_mut()
                    .expect("payload entries should be objects");
                assert!(obj.remove("timestamp").is_some());
                assert!(obj.remove("hostname").is_some());
            })
    });
    n_payloads
}

#[tokio::test]
async fn test_logs() {
    trace_init();

    // As it stands, we do still need a small sleep to allow the events to flow through.
    // There doesn't seem to be a great way to avoid this.
    sleep(Duration::from_secs(5));

    let logs_endpoint = "/api/v2/logs";
    let mut agent_payloads = get_payloads_agent(&logs_endpoint).await;

    //dbg!(&agent_payloads);

    let mut vector_payloads = get_payloads_vector(&logs_endpoint).await;

    //dbg!(&vector_payloads);

    assert!(agent_payloads.len() > 0);
    assert!(vector_payloads.len() > 0);

    let n_agent_payloads = assert_timestamp_hostname(&mut agent_payloads);
    let n_vector_payloads = assert_timestamp_hostname(&mut vector_payloads);

    println!("n agent payloads: {n_agent_payloads}");
    println!("n vector payloads: {n_vector_payloads}");

    assert_eq!(agent_payloads, vector_payloads);
}

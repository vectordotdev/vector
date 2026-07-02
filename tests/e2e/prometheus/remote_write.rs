use std::time::Duration;

use serde::Deserialize;
use vector::test_util::trace_init;

#[derive(Debug, Deserialize)]
struct ReceiverStats {
    requests: u64,
    bytes: u64,
}

async fn read_receiver_stats(client: &reqwest::Client) -> Option<ReceiverStats> {
    client
        .get("http://prometheus-remote-write-receiver:8080/stats")
        .send()
        .await
        .ok()?
        .json::<ReceiverStats>()
        .await
        .ok()
}

async fn read_remote_write_buffer_bytes(client: &reqwest::Client) -> Option<f64> {
    let metrics = client
        .get("http://vector:9598/metrics")
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;

    metrics.lines().find_map(|line| {
        if line.starts_with("vector_buffer_byte_size")
            && line.contains(r#"component_id="remote_write""#)
        {
            line.split_whitespace().nth(1)?.parse::<f64>().ok()
        } else {
            None
        }
    })
}

#[tokio::test]
async fn disk_buffer_is_reclaimed_after_successful_delivery() {
    trace_init();

    let client = reqwest::Client::new();
    let mut last_requests = 0;
    let mut stable_samples = 0;
    let mut stats = None;

    for _ in 0..60 {
        match read_receiver_stats(&client).await {
            Some(current) => {
                if current.requests >= 10 && current.requests == last_requests {
                    stable_samples += 1;
                } else {
                    stable_samples = 0;
                }

                last_requests = current.requests;
                stats = Some(current);

                if stable_samples >= 3 {
                    break;
                }
            }
            None => {}
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let stats = stats.expect("remote-write receiver should become reachable");
    assert!(
        stats.requests >= 10,
        "expected at least 10 remote-write requests, got {stats:?}"
    );
    assert!(
        stats.bytes > 0,
        "remote-write receiver should receive non-empty payloads"
    );

    // Give the disk buffer reader a short window to finalize delivered records
    // and reclaim rotated data files. The E2E vector service sets
    // VECTOR_DISK_V2_MAX_DATA_FILE_SIZE low so finalizer retention shows up
    // without generating a large amount of traffic.
    tokio::time::sleep(Duration::from_secs(2)).await;

    let mut buffer_bytes = None;
    for _ in 0..30 {
        buffer_bytes = read_remote_write_buffer_bytes(&client).await;

        if matches!(buffer_bytes, Some(bytes) if bytes < 2.0 * 1024.0 * 1024.0) {
            break;
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let buffer_bytes = buffer_bytes.expect("remote_write buffer metrics should be exposed");
    assert!(
        buffer_bytes < 2.0 * 1024.0 * 1024.0,
        "remote_write disk buffer was not reclaimed; buffer_byte_size={buffer_bytes}"
    );
}

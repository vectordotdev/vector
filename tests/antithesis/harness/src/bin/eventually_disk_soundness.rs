//! Terminal soundness check for `disk_v2`.
//!
//! Antithesis runs an `eventually_` command only after fault injection stops and
//! fault-phase drivers are killed. At that point this command requires Vector to
//! recover, drain all logically occupied buffer space, accept a fresh sequence
//! that crosses data-file boundaries, deliver every fresh record, and drain
//! again. It does not require delivery of fault-phase records.

#![allow(clippy::disallowed_types)] // antithesis assert macros expand to once_cell::Lazy

#[cfg(target_os = "linux")]
extern crate antithesis_instrumentation;

use std::{
    fs,
    path::{Path, PathBuf},
};

use antithesis_harness::{is_progress_probe_payload, payload_field};
use antithesis_sdk::{antithesis_init, assert_always, assert_reachable};
use clap::Parser;
use serde_json::json;
use tokio::time;

const MAX_BUFFER_SIZE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DATA_FILE_SIZE_BYTES: u64 = 2 * 1024 * 1024;
const PROBE_COUNT: usize = 12;
const EMPTY_POLLS_REQUIRED: usize = 3;

#[derive(Parser)]
struct Args {
    #[arg(long, env = "VECTOR_SOURCE_URL", default_value = "http://vector:8080/")]
    source_url: String,
    #[arg(long, env = "ORACLE_URL", default_value = "http://127.0.0.1:8686")]
    oracle_url: String,
    #[arg(
        long,
        env = "VECTOR_METRICS_URL",
        default_value = "http://vector:9598/metrics"
    )]
    metrics_url: String,
    #[arg(
        long,
        env = "VECTOR_BUFFER_DIR",
        default_value = "/sut-data/buffer/v2/out"
    )]
    buffer_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, Default)]
struct BufferMetrics {
    occupancy_events: Option<f64>,
    occupancy_bytes: Option<f64>,
    received_events: Option<f64>,
    sent_events: Option<f64>,
    discarded_events: Option<f64>,
}

#[derive(Debug)]
struct DiskSnapshot {
    file_count: usize,
    total_data_file_bytes: u64,
    largest_data_file_bytes: u64,
}

fn disk_metric_sum(body: &str, metric_name: &str) -> Option<f64> {
    let mut matches = 0usize;
    let sum = body
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let sample = fields.next()?;
            let value = fields.next()?;
            if sample.starts_with(metric_name)
                && sample.contains("buffer_type=\"disk\"")
                && sample.contains("buffer_id=\"out\"")
            {
                matches += 1;
                value.parse::<f64>().ok()
            } else {
                None
            }
        })
        .sum();
    (matches > 0).then_some(sum)
}

async fn fetch_metrics(client: &reqwest::Client, metrics_url: &str) -> Option<BufferMetrics> {
    let body = client
        .get(metrics_url)
        .timeout(time::Duration::from_secs(3))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    parse_buffer_metrics(&body)
}

fn parse_buffer_metrics(body: &str) -> Option<BufferMetrics> {
    // The maximum-size gauge proves the buffer exists, but current-occupancy
    // gauges are registered lazily. Preserve their absence: after a restart,
    // missing usage gauges do not prove that recovered on-disk data has drained.
    disk_metric_sum(body, "vector_buffer_max_size_bytes")?;

    Some(BufferMetrics {
        occupancy_events: disk_metric_sum(body, "vector_buffer_size_events"),
        occupancy_bytes: disk_metric_sum(body, "vector_buffer_size_bytes"),
        received_events: disk_metric_sum(body, "vector_buffer_received_events_total"),
        sent_events: disk_metric_sum(body, "vector_buffer_sent_events_total"),
        discarded_events: disk_metric_sum(body, "vector_buffer_discarded_events_total"),
    })
}

async fn wait_for_metrics(
    client: &reqwest::Client,
    metrics_url: &str,
    timeout: time::Duration,
) -> Option<BufferMetrics> {
    let deadline = time::Instant::now() + timeout;
    while time::Instant::now() < deadline {
        if let Some(metrics) = fetch_metrics(client, metrics_url).await {
            return Some(metrics);
        }
        time::sleep(time::Duration::from_secs(2)).await;
    }
    None
}

async fn open_ingest_gate(
    client: &reqwest::Client,
    oracle_url: &str,
    timeout: time::Duration,
) -> bool {
    let deadline = time::Instant::now() + timeout;
    while time::Instant::now() < deadline {
        if matches!(
            client
                .post(format!("{oracle_url}/ingest/open"))
                .timeout(time::Duration::from_secs(3))
                .send()
                .await,
            Ok(response) if response.status().is_success()
        ) {
            return true;
        }
        time::sleep(time::Duration::from_millis(500)).await;
    }
    false
}

async fn wait_for_empty_buffer(
    client: &reqwest::Client,
    metrics_url: &str,
    timeout: time::Duration,
) -> (bool, Option<BufferMetrics>) {
    let deadline = time::Instant::now() + timeout;
    let mut stable_empty_polls = 0usize;
    let mut latest = None;
    while time::Instant::now() < deadline {
        if let Some(metrics) = fetch_metrics(client, metrics_url).await {
            stable_empty_polls =
                if metrics.occupancy_events == Some(0.0) && metrics.occupancy_bytes == Some(0.0) {
                    stable_empty_polls + 1
                } else {
                    0
                };
            latest = Some(metrics);
            if stable_empty_polls >= EMPTY_POLLS_REQUIRED {
                return (true, latest);
            }
        } else {
            stable_empty_polls = 0;
        }
        time::sleep(time::Duration::from_secs(2)).await;
    }
    (false, latest)
}

fn disk_snapshot(buffer_dir: &Path) -> Result<DiskSnapshot, String> {
    let entries = fs::read_dir(buffer_dir)
        .map_err(|error| format!("could not read {}: {error}", buffer_dir.display()))?;
    let mut snapshot = DiskSnapshot {
        file_count: 0,
        total_data_file_bytes: 0,
        largest_data_file_bytes: 0,
    };
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with("buffer-data-") || !file_name.ends_with(".dat") {
            continue;
        }
        let size = match entry.metadata() {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        snapshot.file_count += 1;
        snapshot.total_data_file_bytes = snapshot.total_data_file_bytes.saturating_add(size);
        snapshot.largest_data_file_bytes = snapshot.largest_data_file_bytes.max(size);
    }
    Ok(snapshot)
}

fn assert_physical_bounds(snapshot: Result<DiskSnapshot, String>, phase: &str) {
    assert_always!(
        snapshot.is_ok(),
        "the terminal checker can inspect the disk-buffer data files",
        &json!({ "phase": phase, "error": snapshot.as_ref().err() })
    );
    let Ok(snapshot) = snapshot else {
        return;
    };
    assert_always!(
        snapshot.largest_data_file_bytes <= MAX_DATA_FILE_SIZE_BYTES,
        "no disk-buffer data file exceeds its configured maximum",
        &json!({
            "phase": phase,
            "file_count": snapshot.file_count,
            "largest_data_file_bytes": snapshot.largest_data_file_bytes,
            "max_data_file_size_bytes": MAX_DATA_FILE_SIZE_BYTES,
        })
    );
    assert_always!(
        snapshot.total_data_file_bytes <= MAX_BUFFER_SIZE_BYTES,
        "settled disk-buffer data files stay within the configured on-disk limit",
        &json!({
            "phase": phase,
            "file_count": snapshot.file_count,
            "total_data_file_bytes": snapshot.total_data_file_bytes,
            "max_buffer_size_bytes": MAX_BUFFER_SIZE_BYTES,
        })
    );
}

async fn claim_progress_probe(client: &reqwest::Client, oracle_url: &str) -> Option<u64> {
    for _ in 0..8 {
        let response = client
            .post(format!("{oracle_url}/claim"))
            .timeout(time::Duration::from_secs(5))
            .send()
            .await
            .ok()?;
        let id = response.text().await.ok()?.trim().parse().ok()?;
        if is_progress_probe_payload(id) {
            return Some(id);
        }
    }
    None
}

async fn post_probe(client: &reqwest::Client, source_url: &str, id: u64) -> bool {
    let event = json!([{ "id": id, "data": payload_field(id) }]);
    matches!(
        client
            .post(source_url)
            .timeout(time::Duration::from_secs(10))
            .json(&event)
            .send()
            .await,
        Ok(response) if response.status().is_success()
    )
}

async fn submit_progress_probes(
    client: &reqwest::Client,
    source_url: &str,
    oracle_url: &str,
    count: usize,
    timeout: time::Duration,
) -> Vec<u64> {
    let mut probe_ids = Vec::with_capacity(count);
    let deadline = time::Instant::now() + timeout;
    while probe_ids.len() < count && time::Instant::now() < deadline {
        let Some(id) = claim_progress_probe(client, oracle_url).await else {
            time::sleep(time::Duration::from_secs(1)).await;
            continue;
        };
        if post_probe(client, source_url, id).await {
            probe_ids.push(id);
        } else {
            time::sleep(time::Duration::from_secs(1)).await;
        }
    }
    probe_ids
}

async fn delivered(client: &reqwest::Client, oracle_url: &str, id: u64) -> bool {
    let Ok(response) = client
        .get(format!("{oracle_url}/delivered?id={id}"))
        .timeout(time::Duration::from_secs(3))
        .send()
        .await
    else {
        return false;
    };
    response
        .text()
        .await
        .map(|body| body.trim() == "1")
        .unwrap_or(false)
}

async fn wait_for_all_delivered(
    client: &reqwest::Client,
    oracle_url: &str,
    ids: &[u64],
    timeout: time::Duration,
) -> Vec<u64> {
    let deadline = time::Instant::now() + timeout;
    let mut missing = ids.to_vec();
    while !missing.is_empty() && time::Instant::now() < deadline {
        let mut still_missing = Vec::new();
        for id in missing {
            if !delivered(client, oracle_url, id).await {
                still_missing.push(id);
            }
        }
        missing = still_missing;
        if !missing.is_empty() {
            time::sleep(time::Duration::from_secs(2)).await;
        }
    }
    missing
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    antithesis_init();
    let args = Args::parse();
    let client = reqwest::Client::new();

    let gate_opened =
        open_ingest_gate(&client, &args.oracle_url, time::Duration::from_secs(60)).await;
    assert_always!(
        gate_opened,
        "the terminal checker reopens the sink retry gate",
        &json!({ "oracle_url": args.oracle_url })
    );
    if !gate_opened {
        return;
    }

    // Faults have stopped, but a node restart can still be initializing. Serving
    // the disk-buffer metrics proves the process and topology came back.
    let recovered =
        wait_for_metrics(&client, &args.metrics_url, time::Duration::from_secs(180)).await;
    assert_always!(
        recovered.is_some(),
        "Vector restarts and exposes disk-buffer metrics after faults stop",
        &json!({ "metrics_url": args.metrics_url })
    );
    let Some(recovered_metrics) = recovered else {
        return;
    };
    assert_always!(
        recovered_metrics
            .occupancy_bytes
            .is_none_or(|bytes| bytes <= MAX_BUFFER_SIZE_BYTES as f64),
        "recovered disk-buffer occupancy is within the configured limit",
        &json!({
            "occupancy_events": recovered_metrics.occupancy_events,
            "occupancy_bytes": recovered_metrics.occupancy_bytes,
            "max_buffer_size_bytes": MAX_BUFFER_SIZE_BYTES,
        })
    );

    // Buffer usage gauges start a fresh process-local accounting window after a
    // restart, so the max-size gauge can exist while byte occupancy is absent.
    // Delivering this fresh FIFO barrier proves all recoverable records ahead of
    // it were processed and guarantees that usage gauges have been registered.
    let barrier_ids = submit_progress_probes(
        &client,
        &args.source_url,
        &args.oracle_url,
        1,
        time::Duration::from_secs(180),
    )
    .await;
    assert_always!(
        barrier_ids.len() == 1,
        "the recovered disk buffer accepts a fresh drain-barrier probe",
        &json!({ "accepted_count": barrier_ids.len(), "barrier_ids": barrier_ids })
    );
    if barrier_ids.len() != 1 {
        return;
    }
    let missing_barrier = wait_for_all_delivered(
        &client,
        &args.oracle_url,
        &barrier_ids,
        time::Duration::from_secs(180),
    )
    .await;
    assert_always!(
        missing_barrier.is_empty(),
        "the recovered disk buffer drains through a fresh FIFO barrier",
        &json!({ "barrier_ids": barrier_ids, "missing_ids": missing_barrier })
    );
    if !missing_barrier.is_empty() {
        return;
    }

    // Loss is permitted, but after the FIFO barrier arrives, a healthy recovery
    // must explicitly report that it has stopped claiming occupied byte space.
    let (initially_drained, pre_probe_metrics) =
        wait_for_empty_buffer(&client, &args.metrics_url, time::Duration::from_secs(180)).await;
    assert_always!(
        initially_drained,
        "the recovered disk buffer drains instead of remaining permanently stuck",
        &json!({
            "occupancy_events": pre_probe_metrics.and_then(|metrics| metrics.occupancy_events),
            "occupancy_bytes": pre_probe_metrics.and_then(|metrics| metrics.occupancy_bytes),
            "received_events": pre_probe_metrics.and_then(|metrics| metrics.received_events),
            "sent_events": pre_probe_metrics.and_then(|metrics| metrics.sent_events),
            "discarded_events": pre_probe_metrics.and_then(|metrics| metrics.discarded_events),
        })
    );
    if !initially_drained {
        return;
    }
    // Three empty metric polls span the one-second stale-file cleanup interval,
    // so the physical snapshot is settled rather than racing normal deletion.
    assert_physical_bounds(disk_snapshot(&args.buffer_dir), "after recovery drain");

    // Each selected payload is just over the 256KiB write-buffer size and is
    // hex encoded in JSON. Twelve records therefore force multiple 2MiB data
    // file boundaries while staying below the record-size limit.
    let probe_ids = submit_progress_probes(
        &client,
        &args.source_url,
        &args.oracle_url,
        PROBE_COUNT,
        time::Duration::from_secs(180),
    )
    .await;
    assert_always!(
        probe_ids.len() == PROBE_COUNT,
        "the recovered disk buffer accepts a full fresh probe sequence",
        &json!({ "accepted_count": probe_ids.len(), "probe_ids": probe_ids })
    );
    if probe_ids.len() != PROBE_COUNT {
        return;
    }

    let missing = wait_for_all_delivered(
        &client,
        &args.oracle_url,
        &probe_ids,
        time::Duration::from_secs(180),
    )
    .await;
    assert_always!(
        missing.is_empty(),
        "every fault-free post-recovery probe traverses the disk buffer",
        &json!({ "probe_ids": probe_ids, "missing_ids": missing })
    );
    if !missing.is_empty() {
        return;
    }
    assert_reachable!(
        "post-recovery progress crosses disk-buffer data-file boundaries",
        &json!({ "probe_count": probe_ids.len() })
    );

    let (finally_drained, post_probe_metrics) =
        wait_for_empty_buffer(&client, &args.metrics_url, time::Duration::from_secs(180)).await;
    assert_always!(
        finally_drained,
        "the disk buffer returns to zero occupancy after fresh progress",
        &json!({
            "occupancy_events": post_probe_metrics.and_then(|metrics| metrics.occupancy_events),
            "occupancy_bytes": post_probe_metrics.and_then(|metrics| metrics.occupancy_bytes),
            "received_events": post_probe_metrics.and_then(|metrics| metrics.received_events),
            "sent_events": post_probe_metrics.and_then(|metrics| metrics.sent_events),
            "discarded_events": post_probe_metrics.and_then(|metrics| metrics.discarded_events),
        })
    );
    if finally_drained {
        assert_physical_bounds(disk_snapshot(&args.buffer_dir), "after fresh progress");
    }
}

#[cfg(test)]
mod tests {
    use super::parse_buffer_metrics;

    #[test]
    fn max_size_metric_does_not_imply_zero_occupancy() {
        let body = r#"
vector_buffer_max_size_bytes{buffer_id="out",buffer_type="disk"} 8388608
"#;

        let metrics = parse_buffer_metrics(body).expect("buffer should be ready");

        assert_eq!(metrics.occupancy_events, None);
        assert_eq!(metrics.occupancy_bytes, None);
        assert_eq!(metrics.received_events, None);
        assert_eq!(metrics.sent_events, None);
        assert_eq!(metrics.discarded_events, None);
    }

    #[test]
    fn parses_registered_buffer_metrics() {
        let body = r#"
vector_buffer_max_size_bytes{buffer_id="out",buffer_type="disk"} 8388608
vector_buffer_size_events{buffer_id="out",buffer_type="disk"} 1
vector_buffer_size_bytes{buffer_id="out",buffer_type="disk"} 524496
vector_buffer_received_events_total{buffer_id="out",buffer_type="disk"} 12
vector_buffer_sent_events_total{buffer_id="out",buffer_type="disk"} 11
vector_buffer_discarded_events_total{buffer_id="out",buffer_type="disk"} 2
"#;

        let metrics = parse_buffer_metrics(body).expect("buffer should be ready");

        assert_eq!(metrics.occupancy_events, Some(1.0));
        assert_eq!(metrics.occupancy_bytes, Some(524_496.0));
        assert_eq!(metrics.received_events, Some(12.0));
        assert_eq!(metrics.sent_events, Some(11.0));
        assert_eq!(metrics.discarded_events, Some(2.0));
    }

    #[test]
    fn metrics_without_the_disk_buffer_are_not_ready() {
        let body = r#"
vector_buffer_max_size_bytes{buffer_id="other",buffer_type="disk"} 8388608
vector_buffer_max_size_bytes{buffer_id="out",buffer_type="memory"} 8388608
vector_buffer_size_events{buffer_id="out",buffer_type="disk"} 1
"#;

        assert!(parse_buffer_metrics(body).is_none());
    }
}

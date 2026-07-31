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

use antithesis_harness::{is_rollover_probe_payload, payload_field};
use antithesis_sdk::{antithesis_init, assert_always, assert_reachable, assert_unreachable};
use clap::Parser;
use serde_json::json;
use tokio::time;

const MAX_BUFFER_SIZE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DATA_FILE_SIZE_BYTES: u64 = 2 * 1024 * 1024;
const PROBE_COUNT: usize = 20;
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
    occupancy_events: f64,
    occupancy_bytes: f64,
    received_events: f64,
    sent_events: f64,
    discarded_events: f64,
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
    Some(BufferMetrics {
        occupancy_events: disk_metric_sum(&body, "vector_buffer_size_events")?,
        occupancy_bytes: disk_metric_sum(&body, "vector_buffer_size_bytes")?,
        received_events: disk_metric_sum(&body, "vector_buffer_received_events_total")
            .unwrap_or_default(),
        sent_events: disk_metric_sum(&body, "vector_buffer_sent_events_total").unwrap_or_default(),
        discarded_events: disk_metric_sum(&body, "vector_buffer_discarded_events_total")
            .unwrap_or_default(),
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
                if metrics.occupancy_events == 0.0 && metrics.occupancy_bytes == 0.0 {
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
    let Ok(snapshot) = snapshot else {
        assert_unreachable!(
            "the terminal checker can inspect the disk-buffer data files",
            &json!({ "phase": phase, "error": snapshot.unwrap_err() })
        );
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

async fn claim_rollover_probe(client: &reqwest::Client, oracle_url: &str) -> Option<u64> {
    for _ in 0..8 {
        let response = client
            .post(format!("{oracle_url}/claim"))
            .timeout(time::Duration::from_secs(5))
            .send()
            .await
            .ok()?;
        let id = response.text().await.ok()?.trim().parse().ok()?;
        if is_rollover_probe_payload(id) {
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
        recovered_metrics.occupancy_bytes <= MAX_BUFFER_SIZE_BYTES as f64,
        "recovered disk-buffer occupancy is within the configured limit",
        &json!({
            "occupancy_events": recovered_metrics.occupancy_events,
            "occupancy_bytes": recovered_metrics.occupancy_bytes,
            "max_buffer_size_bytes": MAX_BUFFER_SIZE_BYTES,
        })
    );

    // Loss is permitted, but a healthy recovery must reconcile whatever remains
    // and eventually stop claiming occupied space once the sink is reachable.
    let (initially_drained, pre_probe_metrics) =
        wait_for_empty_buffer(&client, &args.metrics_url, time::Duration::from_secs(180)).await;
    assert_always!(
        initially_drained,
        "the recovered disk buffer drains instead of remaining permanently stuck",
        &json!({
            "occupancy_events": pre_probe_metrics.map(|metrics| metrics.occupancy_events),
            "occupancy_bytes": pre_probe_metrics.map(|metrics| metrics.occupancy_bytes),
            "received_events": pre_probe_metrics.map(|metrics| metrics.received_events),
            "sent_events": pre_probe_metrics.map(|metrics| metrics.sent_events),
            "discarded_events": pre_probe_metrics.map(|metrics| metrics.discarded_events),
        })
    );
    if !initially_drained {
        return;
    }
    // Three empty metric polls span the one-second stale-file cleanup interval,
    // so the physical snapshot is settled rather than racing normal deletion.
    assert_physical_bounds(disk_snapshot(&args.buffer_dir), "after recovery drain");

    // Each selected source payload is 64 KiB and becomes a 128 KiB hex field.
    // Twenty individually small records still force a 2 MiB data-file rollover
    // without exercising the writer's large-record path.
    let mut probe_ids = Vec::with_capacity(PROBE_COUNT);
    let submission_deadline = time::Instant::now() + time::Duration::from_secs(180);
    while probe_ids.len() < PROBE_COUNT && time::Instant::now() < submission_deadline {
        let Some(id) = claim_rollover_probe(&client, &args.oracle_url).await else {
            time::sleep(time::Duration::from_secs(1)).await;
            continue;
        };
        if post_probe(&client, &args.source_url, id).await {
            probe_ids.push(id);
        } else {
            time::sleep(time::Duration::from_secs(1)).await;
        }
    }
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
            "occupancy_events": post_probe_metrics.map(|metrics| metrics.occupancy_events),
            "occupancy_bytes": post_probe_metrics.map(|metrics| metrics.occupancy_bytes),
            "received_events": post_probe_metrics.map(|metrics| metrics.received_events),
            "sent_events": post_probe_metrics.map(|metrics| metrics.sent_events),
            "discarded_events": post_probe_metrics.map(|metrics| metrics.discarded_events),
        })
    );
    if finally_drained {
        assert_physical_bounds(disk_snapshot(&args.buffer_dir), "after fresh progress");
    }
}

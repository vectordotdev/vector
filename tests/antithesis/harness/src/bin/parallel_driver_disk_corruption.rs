//! Fault-phase corruption injector for the disk-buffer soundness scenario.
//!
//! Each Antithesis invocation finds the most recently modified non-empty data
//! file, which is the observable writer-active segment, and flips two bytes of
//! the final complete record's checksum without changing the file length. A
//! marker keyed by file generation limits the mutation to once per physical
//! segment so scheduling cannot restore the checksum, even after file IDs wrap.

#![allow(clippy::disallowed_types)] // antithesis assert macros expand to once_cell::Lazy

#[cfg(target_os = "linux")]
extern crate antithesis_instrumentation;

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use antithesis_sdk::{antithesis_init, assert_reachable};
use clap::Parser;
use memmap2::MmapMut;
use serde_json::{json, Value};
use tokio::time;
use vector_buffers::corrupt_disk_v2_record_checksum;

const CORRUPTED_BYTES: usize = 2;
const PRESSURE_HIGH_WATERMARK_BYTES: f64 = 5_242_880.0;

#[derive(Parser)]
struct Args {
    #[arg(long, env = "ORACLE_URL", default_value = "http://127.0.0.1:8686")]
    oracle_url: String,
    #[arg(
        long,
        env = "VECTOR_METRICS_URL",
        default_value = "http://vector:9598/metrics"
    )]
    metrics_url: String,
    #[arg(long, env = "VECTOR_DATA_DIR", default_value = "/var/lib/vector")]
    vector_data_dir: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DataFile {
    id: u16,
    path: PathBuf,
    generation: u64,
    last_modified: SystemTime,
}

#[derive(Debug, Eq, PartialEq)]
struct Corruption {
    data_file_id: u16,
    path: PathBuf,
    file_size: u64,
    original_checksum: u32,
    corrupted_checksum: u32,
}

fn parse_data_file_id(path: &Path) -> Option<u16> {
    path.file_name()?
        .to_str()?
        .strip_prefix("buffer-data-")?
        .strip_suffix(".dat")?
        .parse()
        .ok()
}

#[cfg(unix)]
fn file_generation(metadata: &fs::Metadata) -> u64 {
    metadata.ino()
}

#[cfg(not(unix))]
fn file_generation(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

fn most_recent_data_file(candidates: impl IntoIterator<Item = DataFile>) -> Option<DataFile> {
    candidates
        .into_iter()
        .max_by_key(|candidate| candidate.last_modified)
}

fn active_data_file(buffer_dir: &Path) -> io::Result<Option<DataFile>> {
    let mut candidates = Vec::new();
    for entry in fs::read_dir(buffer_dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(id) = parse_data_file_id(&path) else {
            continue;
        };
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if !metadata.is_file() || metadata.len() == 0 {
            continue;
        }
        candidates.push(DataFile {
            id,
            path,
            generation: file_generation(&metadata),
            last_modified: metadata.modified()?,
        });
    }
    Ok(most_recent_data_file(candidates))
}

fn claim_marker(
    state_dir: &Path,
    data_file_id: u16,
    generation: u64,
) -> io::Result<Option<(PathBuf, File)>> {
    fs::create_dir_all(state_dir)?;
    let marker_path = state_dir.join(format!("buffer-data-{data_file_id}-{generation}.claimed"));
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker_path)
    {
        Ok(marker) => Ok(Some((marker_path, marker))),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(None),
        Err(error) => Err(error),
    }
}

fn corrupt_active_data_file(
    candidate: &DataFile,
    state_dir: &Path,
) -> io::Result<Option<Corruption>> {
    corrupt_active_data_file_with(candidate, state_dir, corrupt_disk_v2_record_checksum)
}

fn corrupt_active_data_file_with(
    candidate: &DataFile,
    state_dir: &Path,
    corrupt_checksum: impl FnOnce(&mut [u8]) -> Result<(u32, u32), String>,
) -> io::Result<Option<Corruption>> {
    let Some((marker_path, marker)) = claim_marker(state_dir, candidate.id, candidate.generation)?
    else {
        return Ok(None);
    };

    let result = (|| {
        marker.sync_all()?;
        let data_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&candidate.path)?;
        let file_size = data_file.metadata()?.len();
        if file_size == 0 {
            return Ok(None);
        }

        let mut data_file_mmap = unsafe { MmapMut::map_mut(&data_file)? };
        let (original_checksum, corrupted_checksum) = corrupt_checksum(&mut data_file_mmap)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        data_file_mmap.flush()?;
        Ok(Some(Corruption {
            data_file_id: candidate.id,
            path: candidate.path.clone(),
            file_size,
            original_checksum,
            corrupted_checksum,
        }))
    })();

    if !matches!(result, Ok(Some(_))) {
        drop(fs::remove_file(marker_path));
    }
    result
}

async fn ingest_gate_is_closed(client: &reqwest::Client, oracle_url: &str) -> bool {
    let Ok(response) = client
        .get(format!("{oracle_url}/report"))
        .timeout(time::Duration::from_secs(3))
        .send()
        .await
    else {
        return false;
    };
    let Ok(report) = response.json::<Value>().await else {
        return false;
    };
    report
        .get("ingest_blocked")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn disk_buffer_occupancy_bytes(body: &str) -> Option<f64> {
    let mut matches = 0usize;
    let sum = body
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let sample = fields.next()?;
            let value = fields.next()?;
            if sample.starts_with("vector_buffer_size_bytes")
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

async fn observed_buffer_occupancy(client: &reqwest::Client, metrics_url: &str) -> Option<f64> {
    let response = client
        .get(metrics_url)
        .timeout(time::Duration::from_secs(2))
        .send()
        .await
        .ok()?;
    let body = response.text().await.ok()?;
    disk_buffer_occupancy_bytes(&body)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    antithesis_init();
    let args = Args::parse();
    let client = reqwest::Client::new();

    if !ingest_gate_is_closed(&client, &args.oracle_url).await {
        return;
    }
    let Some(occupancy_bytes) = observed_buffer_occupancy(&client, &args.metrics_url).await else {
        return;
    };
    if occupancy_bytes < PRESSURE_HIGH_WATERMARK_BYTES {
        return;
    }

    let data_dir = Path::new(&args.vector_data_dir);
    let buffer_dir = data_dir.join("buffer/v2/out");
    let state_dir = data_dir.join("antithesis-corruption-state");
    let Ok(Some(active)) = active_data_file(&buffer_dir) else {
        return;
    };
    let Ok(Some(corruption)) = corrupt_active_data_file(&active, &state_dir) else {
        return;
    };
    assert_reachable!(
        "the corruption driver modifies the active data-file checksum in place",
        &json!({
            "data_file_id": corruption.data_file_id,
            "path": corruption.path,
            "corrupted_bytes": CORRUPTED_BYTES,
            "file_size": corruption.file_size,
            "original_checksum": corruption.original_checksum,
            "corrupted_checksum": corruption.corrupted_checksum,
            "occupancy_bytes": occupancy_bytes,
            "pressure_high_watermark_bytes": PRESSURE_HIGH_WATERMARK_BYTES,
        })
    );
}

#[cfg(test)]
mod tests {
    use super::{
        active_data_file, corrupt_active_data_file_with, disk_buffer_occupancy_bytes,
        most_recent_data_file, parse_data_file_id, DataFile,
    };
    use std::{
        fs,
        path::PathBuf,
        time::{Duration, SystemTime},
    };

    #[test]
    fn parses_only_disk_buffer_data_file_names() {
        assert_eq!(
            parse_data_file_id(PathBuf::from("buffer-data-42.dat").as_path()),
            Some(42)
        );
        assert_eq!(
            parse_data_file_id(PathBuf::from("buffer.db").as_path()),
            None
        );
    }

    #[test]
    fn reads_only_the_target_disk_buffer_occupancy() {
        let body = r#"
vector_buffer_size_bytes{buffer_id="out",buffer_type="memory"} 100
vector_buffer_size_bytes{buffer_id="other",buffer_type="disk"} 200
vector_buffer_max_size_bytes{buffer_id="out",buffer_type="disk"} 8388608
vector_buffer_size_bytes{buffer_id="out",buffer_type="disk"} 5242880
"#;

        assert_eq!(disk_buffer_occupancy_bytes(body), Some(5_242_880.0));
    }

    #[test]
    fn most_recent_data_file_wins_across_id_wraparound() {
        let older = DataFile {
            id: u16::MAX,
            path: PathBuf::from("buffer-data-65535.dat"),
            generation: 1,
            last_modified: SystemTime::UNIX_EPOCH + Duration::from_secs(1),
        };
        let newer = DataFile {
            id: 0,
            path: PathBuf::from("buffer-data-0.dat"),
            generation: 2,
            last_modified: SystemTime::UNIX_EPOCH + Duration::from_secs(2),
        };

        let active = most_recent_data_file([older, newer]).unwrap();
        assert_eq!(active.id, 0);
    }

    #[test]
    fn active_data_file_checksum_is_modified_in_place_only_once() {
        let temp = tempfile::tempdir().unwrap();
        let buffer_dir = temp.path().join("buffer");
        let state_dir = temp.path().join("state");
        fs::create_dir(&buffer_dir).unwrap();
        fs::write(buffer_dir.join("buffer-data-7.dat"), [1, 2, 3, 4]).unwrap();

        let active = active_data_file(&buffer_dir).unwrap().unwrap();
        let corruption = corrupt_active_data_file_with(&active, &state_dir, |data_file| {
            let tail_start = data_file.len() - 2;
            for byte in &mut data_file[tail_start..] {
                *byte ^= u8::MAX;
            }
            Ok((0x0304, 0xfcfb))
        })
        .unwrap()
        .expect("first mutation should win the marker claim");
        assert_eq!(corruption.file_size, 4);
        assert_eq!(corruption.original_checksum, 0x0304);
        assert_eq!(corruption.corrupted_checksum, 0xfcfb);
        assert_eq!(fs::read(&active.path).unwrap(), [1, 2, 252, 251]);
        assert_eq!(fs::metadata(&active.path).unwrap().len(), 4);

        assert!(
            corrupt_active_data_file_with(&active, &state_dir, |_| unreachable!())
                .unwrap()
                .is_none()
        );
        assert_eq!(fs::read(&active.path).unwrap(), [1, 2, 252, 251]);
        assert_eq!(fs::metadata(&active.path).unwrap().len(), 4);

        fs::write(&active.path, [1, 2, 3, 4]).unwrap();
        let next_generation = DataFile {
            generation: active.generation + 1,
            ..active
        };
        assert!(
            corrupt_active_data_file_with(&next_generation, &state_dir, |data_file| {
                data_file[2] ^= u8::MAX;
                data_file[3] ^= u8::MAX;
                Ok((0x0304, 0xfcfb))
            })
            .unwrap()
            .is_some()
        );
    }
}

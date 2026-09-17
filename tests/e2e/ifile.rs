//! Real-binary tests: `cargo test --no-default-features --features
//! sources-ifile,sources-internal_metrics,transforms-filter,sinks-console --test e2e ifile::`.
//! Every test uses real files and filtered source metrics. Timeouts bound failures;
//! readiness comes from observed output, not a startup sleep.

use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    process::Stdio,
    time::Duration,
};

use flate2::{Compression, write::GzEncoder};
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::mpsc::{self, UnboundedReceiver},
    task::JoinHandle,
    time::timeout,
};

const DEADLINE: Duration = Duration::from_secs(10);

#[derive(Debug, Default)]
struct Observed {
    messages: Vec<String>,
    received_events: f64,
}

impl Observed {
    fn record(&mut self, line: &str) -> vector::Result<()> {
        let event: Value = serde_json::from_str(line)?;
        if let Some(message) = event.get("message").and_then(Value::as_str) {
            self.messages.push(message.to_owned());
        } else {
            if event["tags"]["component_id"] != "files"
                || event["tags"]["component_kind"] != "source"
            {
                return Err(format!("metric from an unexpected component: {event}").into());
            }
            if event["name"] == "component_received_events_total" {
                if event["kind"] != "absolute" {
                    return Err(format!("expected an absolute counter: {event}").into());
                }
                self.received_events = event["counter"]["value"]
                    .as_f64()
                    .ok_or("missing received-event counter value")?;
            }
        }
        Ok(())
    }
}

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    input: PathBuf,
    data: PathBuf,
}

impl Fixture {
    fn new() -> vector::Result<Self> {
        let temp = tempfile::tempdir()?;
        // macOS notifications report canonical paths, e.g. /private/var, not /var.
        let root = temp.path().canonicalize()?;
        let input = root.join("input");
        let data = root.join("data");
        std::fs::create_dir_all(&input)?;
        std::fs::create_dir(&data)?;
        Ok(Self {
            _temp: temp,
            root,
            input,
            data,
        })
    }

    fn write(&self, name: &str, records: &[String]) -> vector::Result<()> {
        std::fs::write(self.input.join(name), lines(records))?;
        Ok(())
    }

    fn append(&self, name: &str, records: &[String]) -> vector::Result<()> {
        let mut file = OpenOptions::new()
            .append(true)
            .open(self.input.join(name))?;
        file.write_all(lines(records).as_bytes())?;
        file.sync_all()?;
        Ok(())
    }

    fn checkpoint_position(&self) -> vector::Result<u64> {
        let value: Value =
            serde_json::from_slice(&std::fs::read(self.data.join("files/checkpoints.json"))?)?;
        let checkpoints = value["checkpoints"]
            .as_array()
            .ok_or("missing checkpoints")?;
        assert_eq!(checkpoints.len(), 1, "{value}");
        Ok(checkpoints[0]["position"]
            .as_u64()
            .ok_or("missing position")?)
    }

    fn start(&self, pattern: &str, options: Value) -> vector::Result<Running> {
        let mut source = json!({
            "type": "ifile",
            "include": [self.input.join(pattern)],
            "read_from": "beginning",
            "internal_metrics": {"include_file_tag": false},
            "acknowledgements": {"enabled": true}
        });
        source
            .as_object_mut()
            .unwrap()
            .extend(options.as_object().unwrap().clone());
        let config = self.root.join("vector.yaml");
        std::fs::write(
            &config,
            serde_yaml::to_string(&json!({
                "data_dir": self.data,
                "sources": {
                    "files": source,
                    "metrics": {"type": "internal_metrics", "scrape_interval_secs": 0.1}
                },
                "transforms": {
                    "files_metrics": {
                        "type": "filter", "inputs": ["metrics"],
                        "condition": ".tags.component_id == \"files\" && .tags.component_kind == \"source\""
                    }
                },
                "sinks": {
                    "observed": {
                        "type": "console", "inputs": ["files", "files_metrics"],
                        "target": "stdout", "encoding": {"codec": "json"},
                        "acknowledgements": {"enabled": true}
                    }
                }
            }))?,
        )?;
        let stderr_path = self.root.join("stderr.log");
        let mut child = Command::new(env!("CARGO_BIN_EXE_vector"))
            .arg("--config")
            .arg(config)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(File::create(&stderr_path)?)
            .kill_on_drop(true)
            .spawn()?;
        let mut stdout = BufReader::new(child.stdout.take().expect("stdout is piped")).lines();
        let (send, output) = mpsc::unbounded_channel();
        let reader = tokio::spawn(async move {
            while let Some(line) = stdout.next_line().await? {
                if send.send(line).is_err() {
                    break;
                }
            }
            Ok(())
        });
        Ok(Running {
            child,
            output,
            reader,
            stderr_path,
            observed: Observed::default(),
        })
    }
}

struct Running {
    child: Child,
    output: UnboundedReceiver<String>,
    reader: JoinHandle<std::io::Result<()>>,
    stderr_path: PathBuf,
    observed: Observed,
}

impl Running {
    async fn wait_for(
        &mut self,
        description: &str,
        ready: impl Fn(&Observed) -> bool,
    ) -> vector::Result<()> {
        let result: vector::Result<()> = async {
            timeout(DEADLINE, async {
                while !ready(&self.observed) {
                    let line = self
                        .output
                        .recv()
                        .await
                        .ok_or("Vector stdout closed early")?;
                    self.observed.record(&line)?;
                }
                vector::Result::Ok(())
            })
            .await
            .map_err(|_| {
                format!(
                    "timed out waiting for {description}; {} messages, received_events={}",
                    self.observed.messages.len(),
                    self.observed.received_events
                )
            })?
        }
        .await;
        result.map_err(|error| {
            format!(
                "{error}\nVector stderr:\n{}",
                std::fs::read_to_string(&self.stderr_path).unwrap_or_default()
            )
            .into()
        })
    }

    async fn wait_count(&mut self, count: usize) -> vector::Result<()> {
        self.wait_for(&format!("{count} records and source counter"), |seen| {
            seen.messages.len() >= count && seen.received_events >= count as f64
        })
        .await
    }

    async fn stop(mut self, signal: Signal) -> vector::Result<Observed> {
        let pid = self.child.id().ok_or("Vector exited before shutdown")?;
        kill(Pid::from_raw(pid as i32), signal)?;
        let status = timeout(DEADLINE, self.child.wait())
            .await
            .map_err(|_| "Vector did not exit within the deadline")??;
        timeout(DEADLINE, self.reader).await???;
        while let Some(line) = self.output.recv().await {
            self.observed.record(&line)?;
        }
        if signal == Signal::SIGTERM {
            assert!(
                status.success(),
                "Vector exited with {status}:\n{}",
                std::fs::read_to_string(&self.stderr_path)?
            );
        } else {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(status.signal(), Some(signal as i32));
        }
        Ok(self.observed)
    }
}

fn records(prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|i| format!("{prefix}-{i:04}")).collect()
}

fn lines(records: &[String]) -> String {
    format!("{}\n", records.join("\n"))
}

#[tokio::test]
async fn discovers_files_before_and_after_startup() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    std::fs::create_dir(fixture.input.join("existing"))?;
    let expected = records("discovery", 3);
    fixture.write("existing/before.log", &expected[..1])?;
    let mut run = fixture.start("*/*.log", json!({}))?;
    run.wait_count(1).await?;
    fixture.write("existing/after.log", &expected[1..2])?;
    run.wait_count(2).await?;
    std::fs::create_dir(fixture.input.join("new-directory"))?;
    fixture.write("new-directory/new.log", &expected[2..])?;
    run.wait_count(3).await?;
    let seen = run.stop(Signal::SIGTERM).await?;
    assert_eq!(seen.messages, expected);
    assert_eq!(seen.received_events, 3.0);
    Ok(())
}

#[tokio::test]
async fn rotation_rename_and_create() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let before = records("before-rotation", 3);
    let after = records("after-rotation", 3);
    fixture.write("active.log", &before)?;
    let mut run = fixture.start("*.log", json!({}))?;
    run.wait_count(before.len()).await?;
    std::fs::rename(
        fixture.input.join("active.log"),
        fixture.input.join("rotated.1"),
    )?;
    fixture.write("active.log", &after)?;
    run.wait_count(before.len() + after.len()).await?;
    assert_eq!(
        run.stop(Signal::SIGTERM).await?.messages,
        [before, after].concat()
    );
    Ok(())
}

#[tokio::test]
async fn rotation_reads_writes_through_old_handle() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let before = records("old-inode-before", 2);
    let late = records("old-inode-after", 2);
    let replacement = records("new-inode", 2);
    fixture.write("active.log", &before)?;
    let mut writer = OpenOptions::new()
        .append(true)
        .open(fixture.input.join("active.log"))?;
    let mut run = fixture.start("*.log", json!({}))?;
    run.wait_count(2).await?;
    // The rotated name is intentionally outside the include glob.
    std::fs::rename(
        fixture.input.join("active.log"),
        fixture.input.join("rotated.1"),
    )?;
    fixture.write("active.log", &replacement)?;
    run.wait_count(4).await?;
    writer.write_all(lines(&late).as_bytes())?;
    writer.sync_all()?;
    run.wait_count(6).await?;
    assert_eq!(
        run.stop(Signal::SIGTERM).await?.messages,
        [before, replacement, late].concat()
    );
    Ok(())
}

#[tokio::test]
async fn rotation_copy_truncate() -> vector::Result<()> {
    copy_truncate(json!({"fingerprint": {"strategy": "device_and_inode"}})).await
}

#[tokio::test]
async fn rotation_copy_truncate_checksum() -> vector::Result<()> {
    copy_truncate(json!({})).await
}

async fn copy_truncate(options: Value) -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let before = records("copy-before", 20);
    let after = records("copy-after", 3);
    fixture.write("active.log", &before)?;
    let mut run = fixture.start("*.log", options)?;
    run.wait_count(before.len()).await?;
    std::fs::copy(
        fixture.input.join("active.log"),
        fixture.input.join("rotated.1"),
    )?;
    // Rewrites the same inode, with a length smaller than its previous read offset.
    fixture.write("active.log", &after)?;
    run.wait_count(before.len() + after.len()).await?;
    assert_eq!(
        run.stop(Signal::SIGTERM).await?.messages,
        [before, after].concat()
    );
    Ok(())
}

#[tokio::test]
async fn graceful_restart_resumes_exactly() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let before = records("graceful-before", 10);
    let after = records("graceful-after", 10);
    fixture.write("active.log", &before)?;
    let options = json!({"checkpoint_interval": 3_600_000});
    let mut run = fixture.start("*.log", options.clone())?;
    run.wait_count(before.len()).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, before);
    assert_eq!(fixture.checkpoint_position()?, lines(&before).len() as u64);
    fixture.append("active.log", &after)?;
    let mut run = fixture.start("*.log", options)?;
    run.wait_count(after.len()).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, after);
    assert_eq!(
        fixture.checkpoint_position()?,
        lines(&[before, after].concat()).len() as u64
    );
    Ok(())
}

#[tokio::test]
async fn abrupt_restart_replays_only_since_checkpoint_without_loss() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let committed = records("committed", 3);
    let pending = records("pending", 100);
    let options = json!({"checkpoint_interval": 3_600_000, "max_read_bytes": 1});
    fixture.write("active.log", &committed)?;
    let mut run = fixture.start("*.log", options.clone())?;
    run.wait_count(committed.len()).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, committed);
    let offset = fixture.checkpoint_position()?;
    assert_eq!(offset, lines(&committed).len() as u64);

    fixture.append("active.log", &pending)?;
    let mut run = fixture.start("*.log", options.clone())?;
    run.wait_count(3).await?;
    let interrupted = run.stop(Signal::SIGKILL).await?.messages;
    assert!(!interrupted.is_empty() && interrupted.len() < pending.len());
    assert_eq!(interrupted, pending[..interrupted.len()]);
    assert_eq!(fixture.checkpoint_position()?, offset);

    // Documented guarantee: an abrupt stop can replay uncheckpointed data.
    // Require every pending record, but no replay from before the durable checkpoint.
    let mut run = fixture.start("*.log", options)?;
    run.wait_count(pending.len()).await?;
    assert_eq!(run.stop(Signal::SIGTERM).await?.messages, pending);
    Ok(())
}

#[tokio::test]
async fn gzip_exact_output_across_read_batches() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let expected = records("gzip", 30);
    let mut gzip = GzEncoder::new(
        File::create(fixture.input.join("compressed.log"))?,
        Compression::default(),
    );
    gzip.write_all(lines(&expected).as_bytes())?;
    gzip.finish()?.sync_all()?;
    let mut run = fixture.start("*.log", json!({"max_read_bytes": 1}))?;
    run.wait_count(expected.len()).await?;
    let seen = run.stop(Signal::SIGTERM).await?;
    assert_eq!(seen.messages, expected);
    assert_eq!(seen.received_events, 30.0);
    Ok(())
}

#[tokio::test]
async fn backlog_does_not_starve_new_small_file() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let backlog = records("backlog", 2000);
    let small = records("small", 1);
    fixture.write("large.log", &backlog)?;
    let mut run = fixture.start(
        "*.log",
        json!({"max_read_bytes": 64, "oldest_first": false}),
    )?;
    run.wait_count(1).await?;
    assert!(run.observed.messages.len() < backlog.len());
    fixture.write("small.log", &small)?;
    run.wait_for("small file while backlog remains", |seen| {
        seen.messages.contains(&small[0])
    })
    .await?;
    let seen = run.stop(Signal::SIGTERM).await?;
    let small_position = seen
        .messages
        .iter()
        .position(|line| line == &small[0])
        .unwrap();
    assert!(
        small_position < backlog.len(),
        "small file waited for the entire backlog"
    );
    assert_eq!(
        seen.messages
            .iter()
            .filter(|line| *line == &small[0])
            .count(),
        1
    );
    let large: Vec<_> = seen
        .messages
        .into_iter()
        .filter(|line| line != &small[0])
        .collect();
    assert!(
        large.len() < backlog.len(),
        "shutdown drained the entire backlog"
    );
    assert_eq!(large, backlog[..large.len()]);
    Ok(())
}

#[tokio::test]
async fn shutdown_saves_final_acknowledged_offset_before_exit() -> vector::Result<()> {
    let fixture = Fixture::new()?;
    let expected = records("shutdown", 200);
    fixture.write("active.log", &expected)?;
    let options = json!({"max_read_bytes": 1, "checkpoint_interval": 3_600_000});
    let mut run = fixture.start("*.log", options.clone())?;
    run.wait_count(3).await?;
    let first = run.stop(Signal::SIGTERM).await?.messages;
    assert!(!first.is_empty() && first.len() < expected.len());
    assert_eq!(first, expected[..first.len()]);
    assert_eq!(fixture.checkpoint_position()?, lines(&first).len() as u64);
    let mut run = fixture.start("*.log", options)?;
    run.wait_count(expected.len() - first.len()).await?;
    let second = run.stop(Signal::SIGTERM).await?.messages;
    assert_eq!([first, second].concat(), expected);
    Ok(())
}

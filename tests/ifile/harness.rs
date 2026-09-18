//! Shared real-file fixtures, Vector process lifecycle, and output collection.

use super::common::lines;

use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    process::Stdio,
    time::Duration,
};

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
pub(super) struct Observed {
    pub(super) messages: Vec<String>,
    pub(super) received_events: f64,
    pub(super) open_files: Option<f64>,
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
            if event["name"] == "open_files" {
                self.open_files = event["gauge"]["value"].as_f64();
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

pub(super) struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    pub(super) input: PathBuf,
    data: PathBuf,
}

impl Fixture {
    pub(super) fn new() -> vector::Result<Self> {
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

    pub(super) fn write(&self, name: &str, records: &[String]) -> vector::Result<()> {
        std::fs::write(self.input.join(name), lines(records))?;
        Ok(())
    }

    pub(super) fn append(&self, name: &str, records: &[String]) -> vector::Result<()> {
        let mut file = OpenOptions::new()
            .append(true)
            .open(self.input.join(name))?;
        file.write_all(lines(records).as_bytes())?;
        file.sync_all()?;
        Ok(())
    }

    pub(super) fn checkpoint_position(&self) -> vector::Result<u64> {
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

    pub(super) fn start(&self, pattern: &str, options: Value) -> vector::Result<Running> {
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
            .args(["--threads", "1"])
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

pub(super) struct Running {
    child: Child,
    output: UnboundedReceiver<String>,
    reader: JoinHandle<std::io::Result<()>>,
    stderr_path: PathBuf,
    pub(super) observed: Observed,
}

impl Running {
    pub(super) async fn wait_for(
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

    pub(super) async fn wait_count(&mut self, count: usize) -> vector::Result<()> {
        self.wait_for(&format!("{count} records and source counter"), |seen| {
            seen.messages.len() >= count && seen.received_events >= count as f64
        })
        .await
    }

    pub(super) async fn stop(mut self, signal: Signal) -> vector::Result<Observed> {
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

//! `observer` is a program that inspects a prometheus with a configured query,
//! writes the result out to disk.
use argh::FromArgs;
use prometheus_parser::GroupKind;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::collections::HashSet;
use std::{
    borrow::Cow,
    fmt,
    fmt::Debug,
    io::Read,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, SystemTimeError, UNIX_EPOCH},
};
use tokio::{
    fs,
    io::{AsyncWriteExt, BufWriter},
    runtime::Builder,
    sync::mpsc::{channel, Receiver, Sender},
    time::{interval_at, Instant},
};
use tracing::{debug, error, info, instrument};
use uuid::Uuid;

fn default_config_path() -> String {
    "/etc/vector/soak/observer.yaml".to_string()
}

static TERMINATE: AtomicBool = AtomicBool::new(false);

#[derive(FromArgs)]
/// vector soak `observer` options
struct Opts {
    /// path on disk to the configuration file
    #[argh(option, default = "default_config_path()")]
    config_path: String,
}

/// Target configuration
#[derive(Debug, Deserialize)]
pub struct Target {
    id: String,
    url: String,
    /// Collection of metrics to filter _for_. If empty all metrics are collected.
    #[serde(default)]
    metrics: HashSet<String>,
}

/// Main configuration struct for this program
#[derive(Debug, Deserialize)]
pub struct Config {
    /// The name of the experiment being observed
    pub experiment_name: String,
    /// The variant of the experiment, generally 'baseline' or 'comparison'
    pub variant: Variant,
    /// The targets for this experiment.
    pub targets: Vec<Target>,
    /// The file to record captures into
    pub capture_path: String,
}

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("File manager channel closed before receiving samples"))]
    NoSamples,
    #[snafu(display("Reqwest error: {}", error))]
    Reqwest { error: reqwest::Error },
    #[snafu(display("Not a valid URI with error: {}", error))]
    Http { error: http::uri::InvalidUri },
    #[snafu(display("IO error: {}", error))]
    Io { error: std::io::Error },
    #[snafu(display("Could not parse float: {}", error))]
    ParseFloat { error: std::num::ParseFloatError },
    #[snafu(display("Could not serialize output: {}", error))]
    Json { error: serde_json::Error },
    #[snafu(display("Could not deserialize config: {}", error))]
    Yaml { error: serde_yaml::Error },
    #[snafu(display("Could not deserialize prometheus response: {}", error))]
    Prometheus {
        error: prometheus_parser::ParserError,
    },
    #[snafu(display("Could not query time: {}", error))]
    Time { error: SystemTimeError },
}

impl From<SystemTimeError> for Error {
    fn from(error: SystemTimeError) -> Self {
        Self::Time { error }
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(error: serde_yaml::Error) -> Self {
        Self::Yaml { error }
    }
}

impl From<prometheus_parser::ParserError> for Error {
    fn from(error: prometheus_parser::ParserError) -> Self {
        Self::Prometheus { error }
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::Reqwest { error }
    }
}

impl From<http::uri::InvalidUri> for Error {
    fn from(error: http::uri::InvalidUri) -> Self {
        Self::Http { error }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io { error }
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json { error }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Status {
    Success,
    Error,
}

struct CaptureManager {
    capture_path: String,
    rcv: Receiver<String>,
}

impl fmt::Debug for CaptureManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CaptureManager")
            .field("capture_path", &self.capture_path)
            .finish()
    }
}

impl CaptureManager {
    fn new(capture_path: String, rcv: Receiver<String>) -> Self {
        Self { capture_path, rcv }
    }

    #[instrument]
    async fn run(mut self) -> Result<(), Error> {
        // Wait until the first capture comes across the receiver before
        // creating the capture file. This allows control to observe wehther the
        // capture file exists and act accordingly.
        let mut wtr = if let Some(msg) = self.rcv.recv().await {
            let mut wtr = BufWriter::new(fs::File::create(self.capture_path).await?);
            wtr.write_all(msg.as_bytes()).await?;
            wtr.write_all(b"\n").await?;
            wtr
        } else {
            return Err(Error::NoSamples);
        };
        // Capture any remaining samples and write them out to capture file.
        while let Some(msg) = self.rcv.recv().await {
            wtr.write_all(msg.as_bytes()).await?;
            wtr.write_all(b"\n").await?;
        }
        info!("All sender channels closed, flushing writer and exiting.");
        wtr.flush().await?;
        Ok(())
    }
}

struct TargetWorker {
    snd: Sender<String>,
    experiment_name: String,
    target_id: String,
    url: Url,
    run_id: Arc<Uuid>,
    variant: Variant,
    target_metrics: HashSet<String>,
}

impl fmt::Debug for TargetWorker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TargetWorker")
            .field("run_id", &self.run_id)
            .field("target_id", &self.target_id)
            .field("target_metrics", &self.target_metrics)
            .field("url", &self.url.as_str())
            .finish()
    }
}

impl TargetWorker {
    fn new(
        snd: Sender<String>,
        experiment_name: String,
        target_id: String,
        url: Url,
        run_id: Arc<Uuid>,
        variant: Variant,
        target_metrics: HashSet<String>,
    ) -> Self {
        Self {
            snd,
            experiment_name,
            target_id,
            url,
            run_id,
            variant,
            target_metrics,
        }
    }

    #[instrument]
    async fn run(self) -> Result<(), Error> {
        let client: reqwest::Client = reqwest::Client::new();
        let mut timer = interval_at(Instant::now(), Duration::from_secs(1));

        let mut fetch_index = 0;
        while !TERMINATE.load(Ordering::Relaxed) {
            timer.tick().await;

            let now_ms: u128 = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
            let request = client.get(self.url.clone()).build()?;
            match client.execute(request).await {
                Ok(response) => {
                    let body = response.text().await?;
                    let metric_groups = prometheus_parser::parse_text(&body)?;
                    if metric_groups.is_empty() {
                        debug!("failed to request body: {:?}", body);
                    }
                    // Note that the target is up and online.
                    let output = Output {
                        run_id: Cow::Borrowed(&self.run_id),
                        experiment: Cow::Borrowed(&self.experiment_name),
                        variant: self.variant,
                        target: Cow::Borrowed(&self.target_id),
                        time: now_ms,
                        fetch_index,
                        metric_name: Cow::Borrowed("up"),
                        metric_kind: MetricKind::Gauge,
                        value: 1.0,
                    };
                    let buf = serde_json::to_string(&output)?;
                    self.snd
                        .send(buf)
                        .await
                        .expect("could not send over channel");
                    // Pull the metrics set in configuration, re-emit them in our format.
                    for (kind, metric_name, mm) in metric_groups
                        .into_iter()
                        .filter(|m| {
                            self.target_metrics.is_empty() || self.target_metrics.contains(&m.name)
                        })
                        .filter_map(|m| match m.metrics {
                            GroupKind::Summary(..)
                            | GroupKind::Histogram(..)
                            | GroupKind::Untyped(..) => None,
                            GroupKind::Counter(mm) => Some((MetricKind::Counter, m.name, mm)),
                            GroupKind::Gauge(mm) => Some((MetricKind::Gauge, m.name, mm)),
                        })
                    {
                        for (k, v) in mm.iter() {
                            let timestamp = k.timestamp.map(|x| x as u128).unwrap_or(now_ms);
                            let value = v.value;
                            let output = Output {
                                run_id: Cow::Borrowed(&self.run_id),
                                experiment: Cow::Borrowed(&self.experiment_name),
                                variant: self.variant,
                                target: Cow::Borrowed(&self.target_id),
                                time: timestamp,
                                fetch_index,
                                metric_name: Cow::Borrowed(&metric_name),
                                metric_kind: kind,
                                value,
                            };
                            let buf = serde_json::to_string(&output)?;
                            self.snd
                                .send(buf)
                                .await
                                .expect("could not send over channel");
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Did not receive a response from {} with error: {}",
                        self.target_id, e
                    );
                    // Note that the target is NOT online.
                    let output = Output {
                        run_id: Cow::Borrowed(&self.run_id),
                        experiment: Cow::Borrowed(&self.experiment_name),
                        variant: self.variant,
                        target: Cow::Borrowed(&self.target_id),
                        time: now_ms,
                        fetch_index,
                        metric_name: Cow::Borrowed("up"),
                        metric_kind: MetricKind::Gauge,
                        value: 0.0,
                    };
                    let buf = serde_json::to_string(&output)?;
                    self.snd
                        .send(buf)
                        .await
                        .expect("could not send over channel");
                }
            };
            fetch_index = fetch_index.wrapping_add(1);
        }

        info!("Received terminate signal");
        Ok(())
    }
}

struct Worker {
    experiment_name: String,
    variant: Variant,
    targets: Vec<(Target, Url)>,
    capture_path: String,
}

impl Worker {
    fn new(config: Config) -> Self {
        let mut targets = Vec::with_capacity(config.targets.len());
        for target in config.targets {
            let url = target.url.parse::<Url>().expect("failed to parse URL");
            targets.push((target, url));
        }

        Self {
            experiment_name: config.experiment_name,
            variant: config.variant,
            targets,
            capture_path: config.capture_path,
        }
    }

    async fn run(self) -> Result<(), Error> {
        let (snd, rcv) = channel(1024);

        let run_id: Arc<Uuid> = Arc::new(Uuid::new_v4());

        let jh = tokio::spawn({
            let capture_manager = CaptureManager::new(self.capture_path, rcv);
            capture_manager.run()
        });
        for (target, url) in self.targets.into_iter() {
            let tp = TargetWorker::new(
                snd.clone(),
                self.experiment_name.clone(),
                target.id,
                url,
                Arc::clone(&run_id),
                self.variant,
                target.metrics,
            );
            tokio::spawn(tp.run());
        }
        drop(snd);

        // Wait for a terminate signal to come in, flip TERMINATE to true and
        // then wait for spawned tasks to complete.
        #[cfg(target_family = "unix")]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut signals = signal(SignalKind::terminate())?;
            signals.recv().await;
            info!("Received SIGTERM, beginning shut down.");
        }
        #[cfg(target_family = "windows")]
        {
            use tokio::signal;
            signal::ctrl_c().await?;
            info!("Received ctrl-c, beginning shut down.");
        }
        TERMINATE.store(true, Ordering::Relaxed);

        // The file manager is the last component of this program that properly
        // shuts down, doing so when all of its sender channels are gone.
        jh.await
            .expect("could not join capture file writer")
            .expect("capture file writer did not shut down properly");

        info!("Final component safely shut down. Bye. :)");

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Baseline,
    Comparison,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    Counter,
    Gauge,
}

pub enum VariantError {
    Unknown,
}

impl fmt::Display for VariantError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VariantError::Unknown => write!(f, "unknown, must be baseline|comparison"),
        }
    }
}

impl FromStr for Variant {
    type Err = VariantError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "baseline" => Ok(Self::Baseline),
            "comparison" => Ok(Self::Comparison),
            _ => Err(VariantError::Unknown),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Output<'a> {
    #[serde(borrow)]
    /// An id that is mostly unique to this run, allowing us to distinguish
    /// duplications of the same observational setup.
    pub run_id: Cow<'a, Uuid>,
    #[serde(borrow)]
    pub experiment: Cow<'a, str>,
    pub variant: Variant,
    pub target: Cow<'a, str>,
    pub time: u128,
    pub fetch_index: u64,
    pub metric_name: Cow<'a, str>,
    pub metric_kind: MetricKind,
    pub value: f64,
}

fn get_config() -> Result<Config, Error> {
    let ops: Opts = argh::from_env();
    let mut file: std::fs::File = std::fs::OpenOptions::new()
        .read(true)
        .open(ops.config_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    serde_yaml::from_str(&contents).map_err(|e| e.into())
}

fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();
    let config: Config = get_config()?;
    debug!("CONFIG: {:?}", config);
    let runtime = Builder::new_current_thread()
        .enable_time()
        .enable_io()
        .build()
        .unwrap();
    let worker = Worker::new(config);
    runtime.block_on(worker.run())?;
    Ok(())
}

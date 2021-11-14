//! `observer` is a program that inspects a prometheus with a configured query,
//! writes the result out to disk. It replaces curl-in-a-loop in our soak infra.
use argh::FromArgs;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::io::Read;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::runtime::Builder;
use tokio::time::sleep;
use tracing::{debug, error, info};

fn default_config_path() -> String {
    "/etc/vector/soak/observer.toml".to_string()
}

#[derive(FromArgs)]
/// vector soak `observer` options
struct Opts {
    /// path on disk to the configuration file
    #[argh(option, default = "default_config_path()")]
    config_path: String,
}

/// Main configuration struct for this program
#[derive(Debug, Deserialize)]
pub struct Config {
    /// The time to sleep prior to beginning query scrapes
    pub startup_delay_seconds: u64,
    /// Location to scrape prometheus
    pub prometheus: String,
    /// The name of the experiment being observed
    pub experiment_name: String,
    /// The vector ID associated with the experiment
    pub vector_id: String,
    /// The queries to make of the experiment
    pub queries: Vec<String>,
    /// The file to record captures into
    pub capture_path: String,
}

#[derive(Debug, Snafu)]
enum Error {
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
}

#[derive(Debug, Serialize)]
pub struct Query<'a> {
    query: &'a str,
    value: f64,
}

#[derive(Debug, Serialize)]
pub struct Output<'a> {
    experiment: &'a str,
    vector_id: &'a str,
    time: f64,
    queries: Vec<Query<'a>>,
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

impl From<std::num::ParseFloatError> for Error {
    fn from(error: std::num::ParseFloatError) -> Self {
        Self::ParseFloat { error }
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryResultValue {
    time: f64,
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryResult {
    value: QueryResultValue,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryData {
    result_type: String,
    result: Vec<QueryResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryResponse {
    status: Status,
    data: QueryData,
}

struct Worker {
    url: Url,
    vector_id: String,
    experiment_name: String,
    queries: Vec<String>,
    startup_delay: u64,
    capture_path: String,
}

impl Worker {
    fn new(config: Config) -> Self {
        let queries = Vec::new();
        for query in config.queries {
            let url = format!("{}/api/v1/query?query={}", config.prometheus, query);
            queries.push(url);
        }

        Self {
            url: url.parse::<Url>().unwrap(),
            vector_id: config.vector_id,
            experiment_name: config.experiment_name,
            queries,
            startup_delay: config.startup_delay_seconds,
            capture_path: config.capture_path,
        }
    }

    async fn run(self) -> Result<(), Error> {
        let client: reqwest::Client = reqwest::Client::new();

        let mut file = File::create(self.capture_path).await?;

        info!(
            "observing startup delay sleep of {} seconds",
            self.startup_delay
        );
        sleep(Duration::from_secs(self.startup_delay)).await;
        info!("finished with sleep");

        file.write_all(b"EXPERIMENT\tVECTOR-ID\tTIME\tQUERY\tVALUE\n")
            .await?;
        file.flush().await?;
        loop {
            let request = client.get(self.url.clone()).build()?;
            let body = client
                .execute(request)
                .await?
                .json::<QueryResponse>()
                .await?;
            debug!("body: {:?}", body.data);

            if !body.data.result.is_empty() {
                let time = body.data.result[0].value.time;
                let value = body.data.result[0].value.value.parse::<f64>()?;
                let output = serde_json::to_string(&Output {
                    experiment: &self.experiment_name,
                    vector_id: &self.vector_id,
                    time,
                    queries: vec![Query {
                        query: &self.query,
                        value,
                    }],
                })?;
                file.write_all(output.as_bytes()).await?;
                file.write_all(b"\n").await?;
                file.flush().await?;
            } else {
                error!("failed to request body: {:?}", body.data);
            }
            sleep(Duration::from_secs(1)).await;
        }
    }
}

fn get_config() -> Config {
    let ops: Opts = argh::from_env();
    let mut file: std::fs::File = std::fs::OpenOptions::new()
        .read(true)
        .open(ops.config_path)
        .unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    toml::from_str(&contents).unwrap()
}

fn main() {
    tracing_subscriber::fmt().init();
    let config: Config = get_config();
    let runtime = Builder::new_current_thread()
        .enable_time()
        .enable_io()
        .build()
        .unwrap();
    let worker = Worker::new(config);
    runtime.block_on(worker.run()).unwrap();
}

//! `observer` is a program that inspects a prometheus with a configured query,
//! writes the result out to disk. It replaces curl-in-a-loop in our soak infra.
use std::{borrow::Cow, fs, io::Read, time::Duration};

use argh::FromArgs;
use reqwest::Url;
use serde::Deserialize;
use snafu::Snafu;
use tokio::{runtime::Builder, time::sleep};
use tracing::{debug, error, info};

fn default_config_path() -> String {
    "/etc/vector/soak/observer.yaml".to_string()
}

#[derive(FromArgs)]
/// vector soak `observer` options
struct Opts {
    /// path on disk to the configuration file
    #[argh(option, default = "default_config_path()")]
    config_path: String,
}

/// Query configuration
#[derive(Debug, Deserialize)]
pub struct Query {
    id: String,
    query: String,
    unit: soak::Unit,
}

/// Main configuration struct for this program
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Location to scrape prometheus
    pub prometheus: String,
    /// The name of the experiment being observed
    pub experiment_name: String,
    /// The variant of the experiment, generally 'baseline' or 'comparison'
    pub variant: soak::Variant,
    /// The vector ID associated with the experiment
    pub vector_id: String,
    /// The queries to make of the experiment
    pub queries: Vec<Query>,
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
    result: Vec<QueryResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryResponse {
    data: QueryData,
}

struct Worker {
    vector_id: String,
    experiment_name: String,
    variant: soak::Variant,
    queries: Vec<(Query, Url)>,
    capture_path: String,
}

impl Worker {
    fn new(config: Config) -> Self {
        let mut queries = Vec::with_capacity(config.queries.len());
        for query in config.queries {
            let url = format!("{}/api/v1/query?query={}", config.prometheus, query.query);
            let url = url.parse::<Url>().unwrap();
            queries.push((query, url));
        }

        Self {
            vector_id: config.vector_id,
            experiment_name: config.experiment_name,
            variant: config.variant,
            queries,
            capture_path: config.capture_path,
        }
    }

    async fn run(self) -> Result<(), Error> {
        let client: reqwest::Client = reqwest::Client::new();

        let file = fs::File::create(self.capture_path)?;
        let mut wtr = csv::Writer::from_writer(file);
        for fetch_index in 0..u64::max_value() {
            for (query, url) in &self.queries {
                let request = client.get(url.clone()).build()?;
                let body = client
                    .execute(request)
                    .await?
                    .json::<QueryResponse>()
                    .await?;
                debug!("body: {:?}", body.data);

                if !body.data.result.is_empty() {
                    let time = body.data.result[0].value.time;
                    let value = body.data.result[0].value.value.parse::<f64>()?;
                    let output = soak::Output {
                        experiment: Cow::Borrowed(&self.experiment_name),
                        variant: self.variant,
                        vector_id: Cow::Borrowed(&self.vector_id),
                        time,
                        query_id: Cow::Borrowed(&query.id),
                        query: Cow::Borrowed(&query.query),
                        value,
                        unit: query.unit,
                        fetch_index,
                    };
                    info!("{}", serde_json::to_string(&output)?);
                    wtr.serialize(&output).expect("could not serialize");
                    wtr.flush()?;
                } else {
                    error!("failed to request body: {:?}", body.data);
                }
            }
            sleep(Duration::from_secs(1)).await;
        }
        // SAFETY: The only way to reach this point is to break the above loop
        // -- which we do not -- or to traverse u64::MAX seconds, implying that
        // the computer running this program has been migrated away from the
        // Earth meanwhile as our dear craddle is now well encompassed by the
        // sun. Unless we moved the Earth.
        unreachable!()
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
    serde_yaml::from_str(&contents).unwrap()
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

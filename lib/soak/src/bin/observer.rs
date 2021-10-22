//! `observer` is a program that inspects a prometheus with a configured query,
//! writes the result out to disk. It replaces curl-in-a-loop in our soak infra.
use argh::FromArgs;
use reqwest::Url;
use serde::Deserialize;
use snafu::Snafu;
use std::io::Read;
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::time::sleep;

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
    /// The query to make of the experiment
    pub query: String,
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
    query: String,
    startup_delay: u64,
}

impl Worker {
    fn new(config: Config) -> Self {
        let url = format!(
            "{}/api/v1/query?query={}",
            config.prometheus,
            config.query.clone()
        );

        Self {
            url: url.parse::<Url>().unwrap(),
            vector_id: config.vector_id,
            experiment_name: config.experiment_name,
            query: config.query,
            startup_delay: config.startup_delay_seconds,
        }
    }

    async fn run(self) -> Result<(), Error> {
        let client: reqwest::Client = reqwest::Client::new();

        sleep(Duration::from_secs(self.startup_delay)).await;
        println!("EXPERIMENT\tVECTOR-ID\tTIME\tQUERY\tVALUE");
        for _ in 0..60 {
            let request = client.get(self.url.clone()).build()?;
            let body = client
                .execute(request)
                .await?
                .json::<QueryResponse>()
                .await?;

            if !body.data.result.is_empty() {
                let time = body.data.result[0].value.time;
                let value = body.data.result[0].value.value.parse::<f64>()?;
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    &self.experiment_name, &self.vector_id, time, &self.query, value
                );
            } else {
                // TODO log error to stderr or what not
            }
            sleep(Duration::from_secs(1)).await;
        }
        Ok(())
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
    let config: Config = get_config();
    let runtime = Builder::new_current_thread()
        .enable_time()
        .enable_io()
        .build()
        .unwrap();
    let worker = Worker::new(config);
    runtime.block_on(worker.run()).unwrap();
}

#![cfg(all(test, feature = "sources-generator"))]

use super::controller::ControllerStatistics;
use crate::{
    config::{self, DataType, SinkConfig, SinkContext},
    event::{metric::MetricValue, Event},
    metrics::{self, capture_metrics, get_controller},
    sinks::{
        util::{
            retries::RetryLogic, BatchSettings, EncodedLength, InFlightLimit, TowerRequestConfig,
            VecBuffer,
        },
        Healthcheck, VectorSink,
    },
    sources::generator::GeneratorConfig,
    test_util::{
        start_topology,
        stats::{HistogramStats, LevelTimeHistogram, WeightedSumStats},
    },
};
use core::task::Context;
use futures::{
    compat::Future01CompatExt,
    future::{self, pending, BoxFuture},
    FutureExt,
};
use futures01::Sink;
use rand::{distributions::Exp1, prelude::*};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    collections::HashMap,
    fs::{read_dir, File},
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
    task::Poll,
};
use tokio::time::{self, delay_until, Duration, Instant};
use tower::Service;

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize)]
struct TestParams {
    // The number of requests to issue.
    requests: usize,

    // The time interval between requests.
    interval: Option<f64>,

    // The delay is the base time every request takes return.
    delay: f64,

    // The jitter is the amount of per-request response time randomness,
    // as a fraction of `delay`. The average response time will be
    // `delay * (1 + jitter)` and will have an exponential distribution
    // with λ=1.
    #[serde(default)]
    jitter: f64,

    // The concurrency scale is the rate at which requests' delay
    // increases at higher concurrency levels.
    #[serde(default)]
    concurrency_scale: f64,

    // The number of outstanding requests at which requests will return
    // with an error.
    #[serde(default)]
    concurrency_defer: usize,

    // The number of outstanding requests at which requests will be dropped.
    #[serde(default)]
    concurrency_drop: usize,

    #[serde(default = "default_in_flight_limit")]
    in_flight_limit: InFlightLimit,
}

fn default_in_flight_limit() -> InFlightLimit {
    InFlightLimit::Auto
}

#[derive(Debug, Default, Serialize)]
struct TestConfig {
    request: TowerRequestConfig,
    params: TestParams,

    // The statistics collected by running a test must be local to that
    // test and retained past the completion of the topology. So, they
    // are created by `Default` and may be cloned to retain a handle.
    #[serde(skip)]
    stats: Arc<Mutex<Statistics>>,
    // Oh, the horror!
    #[serde(skip)]
    controller_stats: Arc<Mutex<Arc<Mutex<ControllerStatistics>>>>,
}

#[typetag::serialize(name = "test")]
impl SinkConfig for TestConfig {
    fn build(&self, cx: SinkContext) -> Result<(VectorSink, Healthcheck), crate::Error> {
        let batch = BatchSettings::default().events(1).bytes(9999).timeout(9999);
        let request = self.request.unwrap_with(&TowerRequestConfig::default());
        let sink = request
            .batch_sink(
                TestRetryLogic,
                TestSink::new(self),
                VecBuffer::new(batch.size),
                batch.timeout,
                cx.acker(),
            )
            .sink_map_err(|e| panic!("Fatal test sink error: {}", e));
        let healthcheck = future::ok(()).boxed();

        // Dig deep to get at the internal controller statistics
        let stats = Arc::clone(
            &sink
                .get_ref()
                .get_ref()
                .get_ref()
                .get_ref()
                .controller
                .stats,
        );
        *self.controller_stats.lock().unwrap() = stats;

        Ok((VectorSink::Futures01Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "test"
    }

    fn typetag_deserialize(&self) {
        unimplemented!("not intended for use in real configs")
    }
}

#[derive(Clone, Debug)]
struct TestSink {
    stats: Arc<Mutex<Statistics>>,
    params: TestParams,
}

impl TestSink {
    fn new(config: &TestConfig) -> Self {
        Self {
            stats: Arc::clone(&config.stats),
            params: config.params,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Response {
    Ok,
}

impl crate::sinks::util::sink::Response for Response {}

// The TestSink service doesn't actually do anything with the data, it
// just delays a while depending on the configured parameters and then
// yields a result.
impl Service<Vec<Event>> for TestSink {
    type Response = Response;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _request: Vec<Event>) -> Self::Future {
        let now = Instant::now();
        let mut stats = self.stats.lock().expect("Poisoned stats lock");
        stats.in_flight.adjust(1, now.into());
        let in_flight = stats.in_flight.level();

        let params = self.params;
        let delay = Duration::from_secs_f64(
            params.delay
                * (1.0
                    + (in_flight - 1) as f64 * params.concurrency_scale
                    + thread_rng().sample(Exp1) * params.jitter),
        );

        if params.concurrency_drop > 0 && in_flight >= params.concurrency_drop {
            stats.in_flight.adjust(-1, now.into());
            Box::pin(pending())
        } else {
            let stats2 = Arc::clone(&self.stats);
            Box::pin(async move {
                delay_until(now + delay).await;
                let mut stats = stats2.lock().expect("Poisoned stats lock");
                let in_flight = stats.in_flight.level();
                stats.in_flight.adjust(-1, Instant::now().into());

                if params.concurrency_defer > 0 && in_flight >= params.concurrency_defer {
                    Err(Error::Deferred)
                } else {
                    Ok(Response::Ok)
                }
            })
        }
    }
}

impl EncodedLength for Event {
    fn encoded_length(&self) -> usize {
        1 // Dummy implementation, unused
    }
}

#[derive(Clone, Copy, Debug, Snafu)]
enum Error {
    Deferred,
}

#[derive(Clone, Copy)]
struct TestRetryLogic;

impl RetryLogic for TestRetryLogic {
    type Response = Response;
    type Error = Error;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }
}

#[derive(Debug, Default)]
struct Statistics {
    in_flight: LevelTimeHistogram,
}

#[derive(Debug)]
struct TestResults {
    stats: Statistics,
    cstats: ControllerStatistics,
}

async fn run_test(params: TestParams) -> TestResults {
    let _ = metrics::init();

    let test_config = TestConfig {
        request: TowerRequestConfig {
            in_flight_limit: params.in_flight_limit,
            rate_limit_num: Some(9999),
            timeout_secs: Some(1),
            ..Default::default()
        },
        params,
        ..Default::default()
    };

    let stats = Arc::clone(&test_config.stats);
    let cstats = Arc::clone(&test_config.controller_stats);

    let mut config = config::Config::builder();
    let generator =
        GeneratorConfig::repeat(vec!["line 1".into()], params.requests, params.interval);
    config.add_source("in", generator);
    config.add_sink("out", &["in"], test_config);

    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

    let controller = get_controller().unwrap();

    // Give time for the generator to start and queue all its data, and
    // all the requests to resolve to a response.
    let delay = params.interval.unwrap_or(0.0) * (params.requests as f64) + 1.0;
    // This is crude and dumb, but it works, and the tests run fast and
    // the results are highly repeatable.
    let msecs = (delay * 1000.0) as usize;
    for _ in 0..msecs {
        time::advance(Duration::from_millis(1)).await;
    }
    topology.stop().compat().await.unwrap();

    let stats = Arc::try_unwrap(stats)
        .expect("Failed to unwrap stats Arc")
        .into_inner()
        .expect("Failed to unwrap stats Mutex");

    let cstats = Arc::try_unwrap(cstats)
        .expect("Failed to unwrap controller_stats Arc")
        .into_inner()
        .expect("Failed to unwrap controller_stats Mutex");
    let cstats = Arc::try_unwrap(cstats)
        .expect("Failed to unwrap controller_stats Arc")
        .into_inner()
        .expect("Failed to unwrap controller_stats Mutex");

    let metrics = capture_metrics(&controller)
        .map(Event::into_metric)
        .map(|event| (event.name.clone(), event))
        .collect::<HashMap<_, _>>();
    // Ensure basic statistics are captured, don't actually examine them
    assert!(
        matches!(metrics.get("auto_concurrency_observed_rtt").unwrap().value,
                 MetricValue::Distribution { .. })
    );
    assert!(
        matches!(metrics.get("auto_concurrency_averaged_rtt").unwrap().value,
                 MetricValue::Distribution { .. })
    );
    if params.in_flight_limit == InFlightLimit::Auto {
        assert!(
            matches!(metrics.get("auto_concurrency_limit").unwrap().value,
                     MetricValue::Distribution { .. })
        );
    }
    assert!(
        matches!(metrics.get("auto_concurrency_in_flight").unwrap().value,
                 MetricValue::Distribution { .. })
    );

    TestResults { stats, cstats }
}

#[derive(Debug)]
enum FailureMode {
    ExceededMinimum,
    ExceededMaximum,
}

#[derive(Debug)]
struct Failure {
    stat_name: String,
    mode: FailureMode,
    value: f64,
    reference: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct Range(f64, f64);

impl Range {
    fn assert_usize(&self, value: usize, name1: &str, name2: &str) -> Option<Failure> {
        if value < self.0 as usize {
            Some(Failure {
                stat_name: format!("{} {}", name1, name2),
                mode: FailureMode::ExceededMinimum,
                value: value as f64,
                reference: self.0,
            })
        } else if value > self.1 as usize {
            Some(Failure {
                stat_name: format!("{} {}", name1, name2),
                mode: FailureMode::ExceededMaximum,
                value: value as f64,
                reference: self.1,
            })
        } else {
            None
        }
    }

    fn assert_f64(&self, value: f64, name1: &str, name2: &str) -> Option<Failure> {
        if value < self.0 {
            Some(Failure {
                stat_name: format!("{} {}", name1, name2),
                mode: FailureMode::ExceededMinimum,
                value,
                reference: self.0,
            })
        } else if value > self.1 {
            Some(Failure {
                stat_name: format!("{} {}", name1, name2),
                mode: FailureMode::ExceededMaximum,
                value,
                reference: self.1,
            })
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct ResultTest {
    min: Option<Range>,
    max: Option<Range>,
    mode: Option<Range>,
    mean: Option<Range>,
}

impl ResultTest {
    fn compare_histogram(&self, stat: HistogramStats, name: &str) -> Vec<Failure> {
        vec![
            self.min
                .and_then(|range| range.assert_usize(stat.min, name, "min")),
            self.max
                .and_then(|range| range.assert_usize(stat.max, name, "max")),
            self.mean
                .and_then(|range| range.assert_f64(stat.mean, name, "mean")),
            self.mode
                .and_then(|range| range.assert_usize(stat.mode, name, "mode")),
        ]
        .into_iter()
        .filter_map(|f| f)
        .collect::<Vec<_>>()
    }

    fn compare_weighted_sum(&self, stat: WeightedSumStats, name: &str) -> Vec<Failure> {
        vec![
            self.min
                .and_then(|range| range.assert_f64(stat.min, name, "min")),
            self.max
                .and_then(|range| range.assert_f64(stat.max, name, "max")),
            self.mean
                .and_then(|range| range.assert_f64(stat.mean, name, "mean")),
        ]
        .into_iter()
        .filter_map(|f| f)
        .collect::<Vec<_>>()
    }
}

#[derive(Debug, Deserialize)]
struct ControllerResults {
    observed_rtt: Option<ResultTest>,
    averaged_rtt: Option<ResultTest>,
    concurrency_limit: Option<ResultTest>,
    in_flight: Option<ResultTest>,
}

#[derive(Debug, Deserialize)]
struct StatsResults {
    in_flight: Option<ResultTest>,
}

#[derive(Debug, Deserialize)]
struct TestInput {
    params: TestParams,
    stats: StatsResults,
    controller: ControllerResults,
}

async fn run_compare(file_path: PathBuf, input: TestInput) {
    eprintln!("Running test in {:?}", file_path);

    let results = run_test(input.params).await;

    let mut failures = Vec::new();

    if let Some(test) = input.stats.in_flight {
        let in_flight = results.stats.in_flight.stats().unwrap();
        failures.extend(test.compare_histogram(in_flight, "stats in_flight"));
    }

    if let Some(test) = input.controller.in_flight {
        let in_flight = results.cstats.in_flight.stats().unwrap();
        failures.extend(test.compare_histogram(in_flight, "controller in_flight"));
    }

    if let Some(test) = input.controller.concurrency_limit {
        let concurrency_limit = results.cstats.concurrency_limit.stats().unwrap();
        failures.extend(test.compare_histogram(concurrency_limit, "controller concurrency_limit"));
    }

    if let Some(test) = input.controller.observed_rtt {
        let observed_rtt = results.cstats.observed_rtt.stats().unwrap();
        failures.extend(test.compare_weighted_sum(observed_rtt, "controller observed_rtt"));
    }

    if let Some(test) = input.controller.averaged_rtt {
        let averaged_rtt = results.cstats.averaged_rtt.stats().unwrap();
        failures.extend(test.compare_weighted_sum(averaged_rtt, "controller averaged_rtt"));
    }

    for failure in &failures {
        let mode = match failure.mode {
            FailureMode::ExceededMinimum => "minimum",
            FailureMode::ExceededMaximum => "maximum",
        };
        eprintln!(
            "Comparison failed: {} = {}; {} = {}",
            failure.stat_name, failure.value, mode, failure.reference
        );
    }
    assert!(failures.is_empty(), "{:#?}", results);
}

#[tokio::test]
async fn all_tests() {
    const PATH: &str = "tests/data/auto-concurrency";

    // Read and parse everything first
    let mut entries = read_dir(PATH)
        .expect("Could not open data directory")
        .map(|entry| entry.expect("Could not read data directory").path())
        .filter_map(|file_path| {
            if (file_path.extension().map(|ext| ext == "toml")).unwrap_or(false) {
                let mut data = String::new();
                File::open(&file_path)
                    .unwrap()
                    .read_to_string(&mut data)
                    .unwrap();
                let input: TestInput = toml::from_str(&data)
                    .unwrap_or_else(|error| panic!("Invalid TOML in {:?}: {:?}", file_path, error));
                Some((file_path, input))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    entries.sort_unstable_by_key(|entry| entry.0.to_string_lossy().to_string());

    time::pause();

    // Then run all the tests
    for (file_path, input) in entries {
        run_compare(file_path, input).await;
    }
}

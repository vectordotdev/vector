#![cfg(all(test, feature = "sources-demo_logs"))]
#![allow(clippy::print_stderr)] //tests

use core::task::Context;
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    fs::File,
    future::pending,
    io::Read,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    task::Poll,
};

use futures::{
    channel::oneshot,
    future::{self, BoxFuture},
    stream, FutureExt, SinkExt,
};
use rand::{thread_rng, Rng};
use rand_distr::Exp1;
use rstest::*;
use serde::Deserialize;
use snafu::Snafu;
use tokio::time::{self, sleep, Duration, Instant};
use tower::Service;
use vector_lib::configurable::configurable_component;
use vector_lib::json_size::JsonSize;

use super::controller::ControllerStatistics;
use super::AdaptiveConcurrencySettings;
use crate::{
    config::{self, AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    event::{metric::MetricValue, Event},
    metrics,
    sinks::{
        util::{
            retries::{JitterMode, RetryLogic},
            BatchSettings, Concurrency, EncodedEvent, EncodedLength, TowerRequestConfig, VecBuffer,
        },
        Healthcheck, VectorSink,
    },
    sources::demo_logs::DemoLogsConfig,
    test_util::{
        self, start_topology,
        stats::{HistogramStats, LevelTimeHistogram, TimeHistogram, WeightedSumStats},
    },
};

/// Request handling action when the request limit has been exceeded.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
enum Action {
    #[derivative(Default)]
    /// Additional requests will return with an error.
    Defer,

    /// Additional requests will be silently dropped.
    Drop,
}

/// Limit parameters for sink's ARC behavior.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
struct LimitParams {
    /// The amount a request's delay increases at higher levels of the variable.
    #[serde(default)]
    scale: f64,

    /// The point above which a request's delay increases at an exponential scale rather than a linear scale.
    knee_start: Option<usize>,

    /// The exponent value when the request's delay increase is in the exponential region.
    knee_exp: Option<f64>,

    /// The level above which more requests will be denied.
    limit: Option<usize>,

    #[configurable(derived)]
    #[serde(default)]
    action: Action,
}

impl LimitParams {
    fn action_at_level(&self, level: usize) -> Option<Action> {
        self.limit
            .and_then(|limit| (level > limit).then_some(self.action))
    }

    fn scale(&self, level: usize) -> f64 {
        ((level - 1) as f64).mul_add(
            self.scale,
            self.knee_start
                .map(|knee| {
                    self.knee_exp
                        .unwrap_or(self.scale + 1.0)
                        .powf(level.saturating_sub(knee) as f64)
                        - 1.0
                })
                .unwrap_or(0.0),
        )
    }
}

/// Test parameters for the sink's ARC behavior.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
struct TestParams {
    /// The number of requests to issue.
    requests: usize,

    /// The time interval between requests.
    #[serde(default = "default_interval")]
    interval: f64,

    /// The minimum duration that a request takes to return.
    delay: f64,

    /// The amount of per-request response time randomness, as a fraction of `delay`.
    ///
    /// The average response time will be `delay * (1 + jitter)` and will have an exponential
    /// distribution with λ=1.
    #[serde(default)]
    jitter: f64,

    #[configurable(derived)]
    #[serde(default)]
    concurrency_limit_params: LimitParams,

    #[configurable(derived)]
    #[serde(default)]
    rate: LimitParams,

    #[configurable(derived)]
    #[serde(default = "default_concurrency")]
    concurrency: Concurrency,

    #[configurable(derived)]
    #[serde(default)]
    adaptive_concurrency: AdaptiveConcurrencySettings,
}

const fn default_interval() -> f64 {
    0.0
}

const fn default_concurrency() -> Concurrency {
    Concurrency::Adaptive
}

/// Configuration for the `test_arc` sink.
#[configurable_component(sink("test_arc", "Test (adaptive concurrency)."))]
#[derive(Clone, Debug, Default)]
pub struct TestConfig {
    #[configurable(derived)]
    request: TowerRequestConfig,

    #[configurable(derived)]
    params: TestParams,

    // The statistics collected by running a test must be local to that
    // test and retained past the completion of the topology. So, they
    // are created by `Default` and may be cloned to retain a handle.
    #[serde(skip)]
    control: Arc<Mutex<TestController>>,

    // Oh, the horror!
    #[serde(skip)]
    controller_stats: Arc<Mutex<Arc<Mutex<ControllerStatistics>>>>,
}

impl_generate_config_from_default!(TestConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "test_arc")]
impl SinkConfig for TestConfig {
    async fn build(&self, _cx: SinkContext) -> Result<(VectorSink, Healthcheck), crate::Error> {
        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 1;
        batch_settings.timeout = Duration::from_secs(9999);

        let request = self.request.into_settings();
        let sink = request
            .batch_sink(
                TestRetryLogic,
                TestSink::new(self),
                VecBuffer::new(batch_settings.size),
                batch_settings.timeout,
            )
            .with_flat_map(|event| {
                stream::iter(Some(Ok(EncodedEvent::new(event, 0, JsonSize::zero()))))
            })
            .sink_map_err(|error| panic!("Fatal test sink error: {}", error));
        let healthcheck = future::ok(()).boxed();

        // Dig deep to get at the internal controller statistics
        let stats = Arc::clone(
            &Pin::new(&sink.get_ref().get_ref().get_ref().get_ref())
                .get_ref()
                .controller
                .stats,
        );
        *self.controller_stats.lock().unwrap() = stats;

        #[allow(deprecated)]
        Ok((VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }
}

#[derive(Clone, Debug)]
struct TestSink {
    control: Arc<Mutex<TestController>>,
    params: TestParams,
}

impl TestSink {
    fn new(config: &TestConfig) -> Self {
        Self {
            control: Arc::clone(&config.control),
            params: config.params,
        }
    }

    fn delay_at(&self, in_flight: usize, rate: usize) -> f64 {
        self.params.delay
            * thread_rng().sample::<f64, _>(Exp1).mul_add(
                self.params.jitter,
                1.0 + self.params.concurrency_limit_params.scale(in_flight)
                    + self.params.rate.scale(rate),
            )
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
        let mut control = self.control.lock().expect("Poisoned control lock");
        let stats = &mut control.stats;
        stats.start_request(now);
        let in_flight = stats.in_flight.level();
        let rate = stats.requests.len();

        let action = self
            .params
            .concurrency_limit_params
            .action_at_level(in_flight)
            .or_else(|| self.params.rate.action_at_level(rate));
        match action {
            None => {
                let delay = self.delay_at(in_flight, rate);
                respond_after(Ok(Response::Ok), delay, Arc::clone(&self.control))
            }
            Some(Action::Defer) => {
                let delay = self.delay_at(1, 1);
                respond_after(Err(Error::Deferred), delay, Arc::clone(&self.control))
            }
            Some(Action::Drop) => {
                control.end_request(now, false);
                Box::pin(pending())
            }
        }
    }
}

fn respond_after(
    response: Result<Response, Error>,
    delay: f64,
    control: Arc<Mutex<TestController>>,
) -> BoxFuture<'static, Result<Response, Error>> {
    Box::pin(async move {
        sleep(Duration::from_secs_f64(delay)).await;
        let mut control = control.lock().expect("Poisoned control lock");
        control.end_request(Instant::now(), matches!(response, Ok(Response::Ok)));
        response
    })
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
struct TestController {
    todo: usize,
    send_done: Option<oneshot::Sender<()>>,
    stats: Statistics,
}

#[derive(Default)]
struct Statistics {
    completed: usize,
    in_flight: LevelTimeHistogram,
    rate: TimeHistogram,
    requests: VecDeque<Instant>,
}

impl fmt::Debug for Statistics {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        fmt.debug_struct("SharedData")
            .field("completed", &self.completed)
            .field("in_flight", &self.in_flight)
            .field("rate", &self.rate.stats())
            .field("requests", &self.requests.len())
            .finish()
    }
}

impl TestController {
    fn new(todo: usize, send_done: oneshot::Sender<()>) -> Self {
        Self {
            todo,
            send_done: Some(send_done),
            stats: Default::default(),
        }
    }

    fn end_request(&mut self, now: Instant, completed: bool) {
        self.stats.end_request(now, completed);
        if self.stats.completed >= self.todo {
            if let Some(done) = self.send_done.take() {
                done.send(()).expect("Could not send done signal");
            }
        }
    }
}

impl Statistics {
    fn start_request(&mut self, now: Instant) {
        self.prune_old_requests(now);
        self.requests.push_back(now);
        self.rate.add(self.requests.len(), now.into());
        self.in_flight.adjust(1, now.into());
    }

    fn end_request(&mut self, now: Instant, completed: bool) {
        self.prune_old_requests(now);
        self.rate.add(self.requests.len(), now.into());
        self.in_flight.adjust(-1, now.into());
        self.completed += completed as usize;
    }

    /// Prune any requests that are more than one second old. The
    /// `requests` deque is used to track the rate at which requests are
    /// being issued. As such, it needs to be pruned of old requests any
    /// time a request status changes. Since all requests are inserted
    /// in chronological order, this function simply looks at the head
    /// of the deque and pops off all entries that are more than one
    /// second old. In this way, the length is always equal to the
    /// number of requests per second.
    fn prune_old_requests(&mut self, now: Instant) {
        let then = now - Duration::from_secs(1);
        while let Some(&first) = self.requests.front() {
            if first > then {
                break;
            }
            self.requests.pop_front();
        }
    }
}

#[derive(Debug)]
struct TestResults {
    stats: Statistics,
    cstats: ControllerStatistics,
}

async fn run_test(params: TestParams) -> TestResults {
    test_util::trace_init();
    let (send_done, is_done) = oneshot::channel();

    let test_config = TestConfig {
        request: TowerRequestConfig {
            concurrency: params.concurrency,
            rate_limit_num: 9999,
            timeout_secs: 1,
            retry_jitter_mode: JitterMode::None,
            adaptive_concurrency: params.adaptive_concurrency,
            ..Default::default()
        },
        params,
        control: Arc::new(Mutex::new(TestController::new(params.requests, send_done))),
        controller_stats: Default::default(),
    };

    let control = Arc::clone(&test_config.control);
    let cstats = Arc::clone(&test_config.controller_stats);

    let mut config = config::Config::builder();
    let demo_logs = DemoLogsConfig::repeat(
        vec!["line 1".into()],
        params.requests,
        Duration::from_secs_f64(params.interval),
        None,
    );
    config.add_source("in", demo_logs);
    config.add_sink("out", &["in"], test_config);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let controller = metrics::Controller::get().unwrap();

    is_done.await.expect("Test failed to complete");
    topology.stop().await;

    let control = Arc::try_unwrap(control)
        .expect("Failed to unwrap control Arc")
        .into_inner()
        .expect("Failed to unwrap control Mutex");
    let stats = control.stats;

    let cstats = Arc::try_unwrap(cstats)
        .expect("Failed to unwrap controller_stats Arc")
        .into_inner()
        .expect("Failed to unwrap controller_stats Mutex");
    let cstats = Arc::try_unwrap(cstats)
        .expect("Failed to unwrap controller_stats Arc")
        .into_inner()
        .expect("Failed to unwrap controller_stats Mutex");

    let metrics = controller
        .capture_metrics()
        .into_iter()
        .map(|metric| (metric.name().to_string(), metric))
        .collect::<HashMap<_, _>>();
    // Ensure basic statistics are captured, don't actually examine them
    assert!(matches!(
        metrics
            .get("adaptive_concurrency_observed_rtt")
            .unwrap()
            .value(),
        &MetricValue::AggregatedHistogram { .. }
    ));
    assert!(matches!(
        metrics
            .get("adaptive_concurrency_averaged_rtt")
            .unwrap()
            .value(),
        &MetricValue::AggregatedHistogram { .. }
    ));
    if params.concurrency == Concurrency::Adaptive {
        assert!(matches!(
            metrics.get("adaptive_concurrency_limit").unwrap().value(),
            &MetricValue::AggregatedHistogram { .. }
        ));
    }
    assert!(matches!(
        metrics
            .get("adaptive_concurrency_in_flight")
            .unwrap()
            .value(),
        &MetricValue::AggregatedHistogram { .. }
    ));

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
        .flatten()
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
        .flatten()
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
    rate: Option<ResultTest>,
}

#[derive(Debug, Deserialize)]
struct TestInput {
    params: TestParams,
    stats: StatsResults,
    controller: ControllerResults,
}

async fn run_compare(input: TestInput) {
    let results = run_test(input.params).await;

    let mut failures = Vec::new();

    if let Some(test) = input.stats.in_flight {
        let in_flight = results.stats.in_flight.stats().unwrap();
        failures.extend(test.compare_histogram(in_flight, "stats in_flight"));
    }

    if let Some(test) = input.stats.rate {
        let rate = results.stats.rate.stats().unwrap();
        failures.extend(test.compare_histogram(rate, "stats rate"));
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

#[rstest]
#[tokio::test]
async fn all_tests(#[files("tests/data/adaptive-concurrency/*.toml")] file_path: PathBuf) {
    let mut data = String::new();
    File::open(&file_path)
        .unwrap()
        .read_to_string(&mut data)
        .unwrap();
    let input: TestInput = toml::from_str(&data)
        .unwrap_or_else(|error| panic!("Invalid TOML in {:?}: {:?}", file_path, error));

    time::pause();

    // The first delay takes just slightly longer than all the rest, which causes the first
    // statistic to be inaccurate. Throw in a dummy delay to take up this delay "slack".
    sleep(Duration::from_millis(1)).await;
    time::advance(Duration::from_millis(1)).await;

    run_compare(input).await;
}

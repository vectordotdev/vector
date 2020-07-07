#![cfg(all(feature = "sources-generator"))]

use core::task::Context;
use futures::{compat::Future01CompatExt, future::BoxFuture};
use futures01::{future, Sink};
use serde::Serialize;
use snafu::Snafu;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::{Duration, Instant};
use tokio01::timer::Delay;
use tower03::Service;
use vector::{
    assert_between,
    event::{metric::MetricValue, Event, Metric},
    metrics::{capture_metrics, get_controller, init as metrics_init},
    sinks::{
        util::{retries2::RetryLogic, service2::TowerRequestConfig, BatchSettings, VecBuffer},
        Healthcheck, RouterSink,
    },
    sources::generator::GeneratorConfig,
    test_util::{block_on, runtime, shutdown_on_idle},
    topology::{
        self,
        config::{self, DataType, SinkConfig, SinkContext},
    },
};

mod support;
use support::stats::{LevelTimeHistogram, WeightedSum};

#[derive(Copy, Clone, Debug, Default, Serialize)]
struct TestParams {
    // The delay is the base time every request takes return.
    #[serde(default)]
    delay: Duration,

    // The concurrency scale is the rate at which requests' delay
    // increases at higher concurrency levels.
    #[serde(default)]
    concurrency_scale: f64,
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
}

#[typetag::serialize(name = "test")]
impl SinkConfig for TestConfig {
    fn build(&self, cx: SinkContext) -> Result<(RouterSink, Healthcheck), vector::Error> {
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
        let healthcheck = future::ok(());
        Ok((Box::new(sink), Box::new(healthcheck)))
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
            stats: config.stats.clone(),
            params: config.params,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Response {
    Ok,
}

impl vector::sinks::util::sink::Response for Response {}

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
        let mut stats = self.stats.lock().expect("Poisoned stats lock");
        stats.in_flight.adjust(1);

        let delay = self
            .params
            .delay
            .mul_f64(1.0 + stats.in_flight.level() as f64 * self.params.concurrency_scale);
        let delay = Delay::new(Instant::now() + delay).compat();

        let stats = self.stats.clone();
        Box::pin(async move {
            delay.await.expect("Delay failed!");
            let mut stats = stats.lock().expect("Poisoned stats lock");
            stats.in_flight.adjust(-1);
            Ok(Response::Ok)
        })
    }
}

#[derive(Clone, Copy, Debug, Snafu)]
enum Error {}

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

impl Statistics {
    fn report(&self) {
        eprintln!("in_flight = {}", self.in_flight);
    }
}

type MetricSet = HashMap<String, Metric>;

fn run_test(lines: usize, params: TestParams) -> (f64, Statistics, MetricSet) {
    metrics_init().ok();

    let test_config = TestConfig {
        request: TowerRequestConfig {
            in_flight_limit: Some(10),
            rate_limit_num: Some(9999),
            ..Default::default()
        },
        params,
        ..Default::default()
    };

    let stats = test_config.stats.clone();

    let mut config = config::Config::empty();
    config.add_source("in", GeneratorConfig::repeat(vec!["line 1".into()], lines));
    config.add_sink("out", &["in"], test_config);

    let mut rt = runtime();

    let start = Instant::now();
    let (topology, _crash) = rt
        .block_on_std(topology::start(config, false))
        .expect("Failed to start topology");

    let controller = get_controller().unwrap();

    // Give time for the generator to start and queue all its data.
    std::thread::sleep(Duration::from_secs(1));
    block_on(topology.stop()).unwrap();
    shutdown_on_idle(rt);
    let duration = (Instant::now() - start).as_secs_f64();

    let stats = Arc::try_unwrap(stats)
        .expect("Failed to unwrap stats Arc")
        .into_inner()
        .expect("Failed to unwrap stats Mutex");
    stats.report();

    let metrics = capture_metrics(&controller)
        .map(Event::into_metric)
        .map(|event| (event.name.clone(), event))
        .collect::<MetricSet>();

    (duration, stats, metrics)
}

#[test]
fn constant_link() {
    let (duration, stats, metrics) = run_test(
        200,
        TestParams {
            delay: Duration::from_millis(100),
            ..Default::default()
        },
    );

    // With a constant response time link and enough responses, the
    // limiter will ramp up to or near the maximum concurrency (timing
    // issues may prevent it from hitting exactly the maximum without
    // running the test for an infinite amount of time),
    assert_between!(stats.in_flight.max().unwrap(), 9, 10);
    // and will spend most of its time in the top half of the
    // concurrency range.
    assert_between!(stats.in_flight.mean().unwrap(), 5.0, 10.0);
    // Normal times for 200 requests range from 3-4 seconds.
    assert_between!(duration, 2.9, 4.0);

    let observed_rtt = metric_mean(&metrics, "auto_concurrency_observed_rtt");
    assert_between!(observed_rtt, 100_000_000.0, 110_000_000.0);
    let averaged_rtt = metric_mean(&metrics, "auto_concurrency_averaged_rtt");
    assert_between!(averaged_rtt, 100_000_000.0, 110_000_000.0);
    let concurrency_limit = metric_mean(&metrics, "auto_concurrency_limit");
    assert_between!(concurrency_limit, 5.0, 10.0);
    let in_flight = metric_mean(&metrics, "auto_concurrency_in_flight");
    assert_between!(in_flight, 5.0, 10.0);
}

#[test]
fn slow_link() {
    let (duration, stats, metrics) = run_test(
        100,
        TestParams {
            delay: Duration::from_millis(100),
            concurrency_scale: 1.0,
        },
    );

    // With a link that slows down heavily as concurrency increases, the
    // limiter will keep the concurrency low (timing skews occasionally
    // has it reaching 3, but usually just 2),
    assert_between!(stats.in_flight.max().unwrap(), 1, 3);
    // and it will spend most of its time between 1 and 2.
    let in_flight_mean = stats.in_flight.mean().unwrap();
    assert_between!(in_flight_mean, 1.0, 2.0);
    // Normal times range widely depending if it hits 3 in flight.
    assert_between!(duration, 15.0, 20.0);

    let observed_rtt = metric_mean(&metrics, "auto_concurrency_observed_rtt");
    assert_between!(observed_rtt, 100_000_000.0, 310_000_000.0);
    let averaged_rtt = metric_mean(&metrics, "auto_concurrency_averaged_rtt");
    assert_between!(averaged_rtt, 100_000_000.0, 310_000_000.0);
    let concurrency_limit = metric_mean(&metrics, "auto_concurrency_limit");
    assert_between!(concurrency_limit, 1.0, 2.0);
    let in_flight = metric_mean(&metrics, "auto_concurrency_in_flight");
    assert_between!(in_flight, 0.5, 2.0);
}

fn metric_mean(metrics: &MetricSet, name: &str) -> f64 {
    let metric = metrics.get(name).unwrap();
    match &metric.value {
        MetricValue::Distribution {
            values,
            sample_rates,
        } => values
            .iter()
            .zip(sample_rates.iter())
            .fold(WeightedSum::default(), |mut ws, (&value, &rate)| {
                ws.add(value, rate as f64);
                ws
            })
            .mean()
            .unwrap_or_else(|| panic!("No data for metric {}", name)),
        _ => panic!("Expected distribution metric for {}", name),
    }
}

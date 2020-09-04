// Only run the test suite on unix systems, as the timings on especially
// MacOS are too variable to produce reliable results in these tests.
#![cfg(all(test, not(target_os = "macos"), feature = "sources-generator"))]

use super::controller::ControllerStatistics;
use super::MAX_CONCURRENCY;
use crate::{
    assert_within,
    config::{self, DataType, SinkConfig, SinkContext},
    event::{metric::MetricValue, Event},
    metrics::{self, capture_metrics, get_controller},
    sinks::{
        util::{
            retries::RetryLogic, BatchSettings, EncodedLength, InFlightLimit, TowerRequestConfig,
            VecBuffer,
        },
        Healthcheck, RouterSink,
    },
    sources::generator::GeneratorConfig,
    test_util::{start_topology, stats::LevelTimeHistogram},
};
use core::task::Context;
use futures::{
    compat::Future01CompatExt,
    future::{pending, BoxFuture},
};
use futures01::{future, Sink};
use rand::{distributions::Exp1, prelude::*};
use serde::Serialize;
use snafu::Snafu;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::{Duration, Instant};
use tokio::time::{delay_for, delay_until};
use tower::Service;

#[derive(Copy, Clone, Debug, Default, Serialize)]
struct TestParams {
    // The delay is the base time every request takes return.
    #[serde(default)]
    delay: Duration,

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
    fn build(&self, cx: SinkContext) -> Result<(RouterSink, Healthcheck), crate::Error> {
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
        stats.in_flight.adjust(1, now);
        let in_flight = stats.in_flight.level();

        let params = self.params;
        let delay = params.delay.mul_f64(
            1.0 + (in_flight - 1) as f64 * params.concurrency_scale
                + thread_rng().sample(Exp1) * params.jitter,
        );
        let delay = delay_until((now + delay).into());

        if params.concurrency_drop > 0 && in_flight >= params.concurrency_drop {
            stats.in_flight.adjust(-1, now);
            Box::pin(pending())
        } else {
            let stats2 = Arc::clone(&self.stats);
            Box::pin(async move {
                delay.await;
                let mut stats = stats2.lock().expect("Poisoned stats lock");
                let in_flight = stats.in_flight.level();
                stats.in_flight.adjust(-1, Instant::now());

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
struct TestData {
    stats: Statistics,
    cstats: ControllerStatistics,
}

async fn run_test(lines: usize, interval: Option<f64>, params: TestParams) -> TestData {
    run_test4(lines, interval, params, InFlightLimit::Auto).await
}

async fn run_test4(
    lines: usize,
    interval: Option<f64>,
    params: TestParams,
    in_flight_limit: InFlightLimit,
) -> TestData {
    let _ = metrics::init();

    let test_config = TestConfig {
        request: TowerRequestConfig {
            in_flight_limit,
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
    let generator = GeneratorConfig::repeat(vec!["line 1".into()], lines, interval);
    config.add_source("in", generator);
    config.add_sink("out", &["in"], test_config);

    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

    let controller = get_controller().unwrap();

    // Give time for the generator to start and queue all its data.
    let delay = interval.unwrap_or(0.0) * (lines as f64) + 1.0;
    delay_for(Duration::from_secs_f64(delay)).await;
    topology.stop().compat().await.unwrap();
    //shutdown_on_idle(rt);

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
    if in_flight_limit == InFlightLimit::Auto {
        assert!(
            matches!(metrics.get("auto_concurrency_limit").unwrap().value,
                     MetricValue::Distribution { .. })
        );
    }
    assert!(
        matches!(metrics.get("auto_concurrency_in_flight").unwrap().value,
                 MetricValue::Distribution { .. })
    );

    TestData { stats, cstats }
}

#[tokio::test]
async fn fixed_concurrency() {
    // Simulate a very jittery link, but with a fixed concurrency
    let results = run_test4(
        200,
        None,
        TestParams {
            delay: Duration::from_millis(100),
            jitter: 0.5,
            ..Default::default()
        },
        InFlightLimit::Fixed(10),
    )
    .await;

    let in_flight = results.stats.in_flight.stats().unwrap();
    assert_eq!(in_flight.max, 10, "{:#?}", results);
    assert_eq!(in_flight.mode, 10, "{:#?}", results);

    // Even with jitter, the concurrency limit should never vary
    let concurrency_limit = results.cstats.concurrency_limit.stats().unwrap();
    assert_eq!(concurrency_limit.min, 10, "{:#?}", results);
    assert_eq!(concurrency_limit.max, 10, "{:#?}", results);
    let in_flight = results.cstats.in_flight.stats().unwrap();
    assert_eq!(in_flight.max, 10, "{:#?}", results);
    assert_eq!(in_flight.mode, 10, "{:#?}", results);
}

#[tokio::test]
async fn constant_link() {
    let results = run_test(
        500,
        None,
        TestParams {
            delay: Duration::from_millis(100),
            ..Default::default()
        },
    )
    .await;

    // With a constant response time link and enough responses, the
    // limiter will ramp up towards the maximum concurrency.
    let in_flight = results.stats.in_flight.stats().unwrap();
    assert_within!(in_flight.max, 10, MAX_CONCURRENCY, "{:#?}", results);
    assert_within!(
        in_flight.mean,
        6.0,
        MAX_CONCURRENCY as f64,
        "{:#?}",
        results
    );

    let observed_rtt = results.cstats.observed_rtt.stats().unwrap();
    assert_within!(observed_rtt.min, 0.090, 0.120, "{:#?}", results);
    assert_within!(observed_rtt.max, 0.090, 0.130, "{:#?}", results);
    assert_within!(observed_rtt.mean, 0.090, 0.120, "{:#?}", results);
    let averaged_rtt = results.cstats.averaged_rtt.stats().unwrap();
    assert_within!(averaged_rtt.min, 0.090, 0.120, "{:#?}", results);
    assert_within!(averaged_rtt.max, 0.090, 0.130, "{:#?}", results);
    assert_within!(averaged_rtt.mean, 0.090, 0.120, "{:#?}", results);
    let concurrency_limit = results.cstats.concurrency_limit.stats().unwrap();
    assert_within!(concurrency_limit.max, 9, MAX_CONCURRENCY, "{:#?}", results);
    assert_within!(
        concurrency_limit.mean,
        5.0,
        MAX_CONCURRENCY as f64,
        "{:#?}",
        results
    );
    let c_in_flight = results.cstats.in_flight.stats().unwrap();
    assert_within!(c_in_flight.max, 9, MAX_CONCURRENCY, "{:#?}", results);
    assert_within!(
        c_in_flight.mean,
        6.5,
        MAX_CONCURRENCY as f64,
        "{:#?}",
        results
    );
}

#[tokio::test]
async fn defers_at_high_concurrency() {
    let results = run_test(
        500,
        None,
        TestParams {
            delay: Duration::from_millis(100),
            concurrency_defer: 5,
            ..Default::default()
        },
    )
    .await;

    // With a constant time link that gives deferrals over a certain
    // concurrency, the limiter will ramp up to that concurrency and
    // then drop down repeatedly. Note that, due to the timing of the
    // adjustment, this may actually occasionally go over the error
    // limit above, but it will be rare.
    let in_flight = results.stats.in_flight.stats().unwrap();
    assert_within!(in_flight.max, 4, 6, "{:#?}", results);
    // Since the concurrency will drop down by half each time, the
    // average will be below this maximum.
    assert_within!(in_flight.mode, 2, 4, "{:#?}", results);
    assert_within!(in_flight.mean, 2.0, 4.0, "{:#?}", results);

    let observed_rtt = results.cstats.observed_rtt.stats().unwrap();
    assert_within!(observed_rtt.min, 0.090, 0.120, "{:#?}", results);
    assert_within!(observed_rtt.max, 0.090, 0.130, "{:#?}", results);
    assert_within!(observed_rtt.mean, 0.090, 0.120, "{:#?}", results);
    let averaged_rtt = results.cstats.averaged_rtt.stats().unwrap();
    assert_within!(averaged_rtt.min, 0.090, 0.120, "{:#?}", results);
    assert_within!(averaged_rtt.max, 0.090, 0.130, "{:#?}", results);
    assert_within!(averaged_rtt.mean, 0.090, 0.120, "{:#?}", results);
    let concurrency_limit = results.cstats.concurrency_limit.stats().unwrap();
    assert_within!(concurrency_limit.max, 5, 6, "{:#?}", results);
    assert_within!(concurrency_limit.mode, 2, 5, "{:#?}", results);
    assert_within!(concurrency_limit.mean, 2.0, 4.9, "{:#?}", results);
    let c_in_flight = results.cstats.in_flight.stats().unwrap();
    assert_within!(c_in_flight.max, 5, 6, "{:#?}", results);
    assert_within!(c_in_flight.mode, 2, 4, "{:#?}", results);
    assert_within!(c_in_flight.mean, 2.0, 4.0, "{:#?}", results);
}

#[tokio::test]
async fn drops_at_high_concurrency() {
    let results = run_test(
        500,
        None,
        TestParams {
            delay: Duration::from_millis(100),
            concurrency_drop: 5,
            ..Default::default()
        },
    )
    .await;

    // Since our internal framework doesn't track the "dropped"
    // requests, the values won't be representative of the actual number
    // of requests in flight (tracked below in the internal stats).
    let in_flight = results.stats.in_flight.stats().unwrap();
    assert_within!(in_flight.max, 4, 5, "{:#?}", results);
    assert_within!(in_flight.mode, 3, 4, "{:#?}", results);
    assert_within!(in_flight.mean, 1.5, 3.5, "{:#?}", results);

    let observed_rtt = results.cstats.observed_rtt.stats().unwrap();
    assert_within!(observed_rtt.min, 0.090, 0.125, "{:#?}", results);
    assert_within!(observed_rtt.max, 0.090, 0.125, "{:#?}", results);
    assert_within!(observed_rtt.mean, 0.090, 0.125, "{:#?}", results);
    let averaged_rtt = results.cstats.averaged_rtt.stats().unwrap();
    assert_within!(averaged_rtt.min, 0.090, 0.125, "{:#?}", results);
    assert_within!(averaged_rtt.max, 0.090, 0.125, "{:#?}", results);
    assert_within!(averaged_rtt.mean, 0.090, 0.125, "{:#?}", results);
    let concurrency_limit = results.cstats.concurrency_limit.stats().unwrap();
    assert_within!(concurrency_limit.max, 8, 15, "{:#?}", results);
    assert_within!(concurrency_limit.mode, 5, 10, "{:#?}", results);
    assert_within!(concurrency_limit.mean, 5.0, 10.0, "{:#?}", results);
    let c_in_flight = results.cstats.in_flight.stats().unwrap();
    assert_within!(c_in_flight.max, 8, 15, "{:#?}", results);
    assert_within!(c_in_flight.mode, 5, 10, "{:#?}", results);
    assert_within!(c_in_flight.mean, 5.0, 10.0, "{:#?}", results);
}

#[tokio::test]
async fn slow_link() {
    let results = run_test(
        200,
        None,
        TestParams {
            delay: Duration::from_millis(100),
            concurrency_scale: 1.0,
            ..Default::default()
        },
    )
    .await;

    // With a link that slows down heavily as concurrency increases, the
    // limiter will keep the concurrency low (timing skews occasionally
    // has it reaching 3, but usually just 2),
    let in_flight = results.stats.in_flight.stats().unwrap();
    assert_within!(in_flight.max, 1, 3, "{:#?}", results);
    // and it will spend most of its time between 1 and 2.
    assert_within!(in_flight.mode, 1, 2, "{:#?}", results);
    assert_within!(in_flight.mean, 1.0, 2.0, "{:#?}", results);

    let observed_rtt = results.cstats.observed_rtt.stats().unwrap();
    assert_within!(observed_rtt.min, 0.090, 0.120, "{:#?}", results);
    assert_within!(observed_rtt.mean, 0.090, 0.310, "{:#?}", results);
    let averaged_rtt = results.cstats.averaged_rtt.stats().unwrap();
    assert_within!(averaged_rtt.min, 0.090, 0.120, "{:#?}", results);
    assert_within!(averaged_rtt.mean, 0.090, 0.310, "{:#?}", results);
    let concurrency_limit = results.cstats.concurrency_limit.stats().unwrap();
    assert_within!(concurrency_limit.mode, 1, 3, "{:#?}", results);
    assert_within!(concurrency_limit.mean, 1.0, 2.0, "{:#?}", results);
    let c_in_flight = results.cstats.in_flight.stats().unwrap();
    assert_within!(c_in_flight.max, 1, 3, "{:#?}", results);
    assert_within!(c_in_flight.mode, 1, 2, "{:#?}", results);
    assert_within!(c_in_flight.mean, 1.0, 2.0, "{:#?}", results);
}

#[tokio::test]
async fn slow_send_1() {
    let results = run_test(
        100,
        Some(0.100),
        TestParams {
            delay: Duration::from_millis(50),
            ..Default::default()
        },
    )
    .await;

    // With a generator running slower than the link can process, the
    // limiter will never raise the concurrency above 1.
    let in_flight = results.stats.in_flight.stats().unwrap();
    assert_eq!(in_flight.max, 1, "{:#?}", results);
    assert_eq!(in_flight.mode, 1, "{:#?}", results);
    assert_within!(in_flight.mean, 0.5, 1.0, "{:#?}", results);

    let observed_rtt = results.cstats.observed_rtt.stats().unwrap();
    assert_within!(observed_rtt.min, 0.045, 0.060, "{:#?}", results);
    assert_within!(observed_rtt.mean, 0.045, 0.060, "{:#?}", results);
    let averaged_rtt = results.cstats.averaged_rtt.stats().unwrap();
    assert_within!(averaged_rtt.min, 0.045, 0.060, "{:#?}", results);
    assert_within!(averaged_rtt.mean, 0.045, 0.060, "{:#?}", results);
    let concurrency_limit = results.cstats.concurrency_limit.stats().unwrap();
    assert_eq!(concurrency_limit.mode, 1, "{:#?}", results);
    assert_eq!(concurrency_limit.mean, 1.0, "{:#?}", results);
    let c_in_flight = results.cstats.in_flight.stats().unwrap();
    assert_eq!(c_in_flight.max, 1, "{:#?}", results);
    assert_eq!(c_in_flight.mode, 1, "{:#?}", results);
    assert_within!(c_in_flight.mean, 0.5, 1.0, "{:#?}", results);
}

#[tokio::test]
async fn slow_send_2() {
    let results = run_test(
        100,
        Some(0.050),
        TestParams {
            delay: Duration::from_millis(50),
            ..Default::default()
        },
    )
    .await;

    // With a generator running at the same speed as the link RTT, the
    // limiter will keep the limit around 1-2 depending on timing jitter.
    let in_flight = results.stats.in_flight.stats().unwrap();
    assert_within!(in_flight.max, 1, 3, "{:#?}", results);
    assert_within!(in_flight.mode, 1, 2, "{:#?}", results);
    assert_within!(in_flight.mean, 0.5, 2.0, "{:#?}", results);

    let observed_rtt = results.cstats.observed_rtt.stats().unwrap();
    assert_within!(observed_rtt.min, 0.045, 0.060, "{:#?}", results);
    assert_within!(observed_rtt.mean, 0.045, 0.110, "{:#?}", results);
    let averaged_rtt = results.cstats.averaged_rtt.stats().unwrap();
    assert_within!(averaged_rtt.min, 0.045, 0.060, "{:#?}", results);
    assert_within!(averaged_rtt.mean, 0.045, 0.110, "{:#?}", results);
    let concurrency_limit = results.cstats.concurrency_limit.stats().unwrap();
    assert_within!(concurrency_limit.mode, 1, 2, "{:#?}", results);
    assert_within!(concurrency_limit.mean, 1.0, 2.0, "{:#?}", results);
    let c_in_flight = results.cstats.in_flight.stats().unwrap();
    assert_within!(c_in_flight.max, 1, 3, "{:#?}", results);
    assert_within!(c_in_flight.mode, 1, 2, "{:#?}", results);
    assert_within!(c_in_flight.mean, 1.0, 2.0, "{:#?}", results);
}

#[tokio::test]
async fn medium_send() {
    let results = run_test(
        500,
        Some(0.025),
        TestParams {
            delay: Duration::from_millis(100),
            ..Default::default()
        },
    )
    .await;

    let in_flight = results.stats.in_flight.stats().unwrap();
    // With a generator running at four times the speed as the link RTT,
    // the limiter will keep around 4-5 requests in flight depending on
    // timing jitter.
    assert_within!(in_flight.mode, 4, 5, "{:#?}", results);
    assert_within!(in_flight.mean, 4.0, 6.0, "{:#?}", results);

    let observed_rtt = results.cstats.observed_rtt.stats().unwrap();
    assert_within!(observed_rtt.min, 0.090, 0.120, "{:#?}", results);
    assert_within!(observed_rtt.mean, 0.090, 0.120, "{:#?}", results);
    let averaged_rtt = results.cstats.averaged_rtt.stats().unwrap();
    assert_within!(averaged_rtt.min, 0.090, 0.120, "{:#?}", results);
    assert_within!(averaged_rtt.mean, 0.090, 0.500, "{:#?}", results);
    let concurrency_limit = results.cstats.concurrency_limit.stats().unwrap();
    assert_within!(concurrency_limit.max, 4, MAX_CONCURRENCY, "{:#?}", results);
    let c_in_flight = results.cstats.in_flight.stats().unwrap();
    assert_within!(c_in_flight.max, 4, MAX_CONCURRENCY, "{:#?}", results);
    assert_within!(c_in_flight.mode, 4, 5, "{:#?}", results);
    assert_within!(c_in_flight.mean, 4.0, 5.0, "{:#?}", results);
}

#[tokio::test]
async fn jittery_link_small() {
    let results = run_test(
        500,
        None,
        TestParams {
            delay: Duration::from_millis(100),
            jitter: 0.1,
            ..Default::default()
        },
    )
    .await;

    // Jitter can cause concurrency management to vary widely, though it
    // will typically reach the maximum of 10 in flight.
    let in_flight = results.stats.in_flight.stats().unwrap();
    assert_within!(in_flight.max, 15, 30, "{:#?}", results);
    assert_within!(in_flight.mean, 4.0, 20.0, "{:#?}", results);

    let observed_rtt = results.cstats.observed_rtt.stats().unwrap();
    assert_within!(observed_rtt.mean, 0.090, 0.130, "{:#?}", results);
    let averaged_rtt = results.cstats.averaged_rtt.stats().unwrap();
    assert_within!(averaged_rtt.mean, 0.090, 0.130, "{:#?}", results);
    let concurrency_limit = results.cstats.concurrency_limit.stats().unwrap();
    assert_within!(concurrency_limit.max, 10, 30, "{:#?}", results);
    assert_within!(concurrency_limit.mean, 6.0, 20.0, "{:#?}", results);
    let c_in_flight = results.cstats.in_flight.stats().unwrap();
    assert_within!(c_in_flight.max, 15, 30, "{:#?}", results);
    assert_within!(c_in_flight.mean, 6.0, 20.0, "{:#?}", results);
}

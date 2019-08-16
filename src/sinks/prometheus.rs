use crate::{
    buffers::Acker,
    event::{metric::Direction, Metric},
    topology::config::{DataType, SinkConfig},
    Event,
};
use futures::{future, try_ready, Async, AsyncSink, Future, Sink};
use hyper::{
    header::HeaderValue, service::service_fn, Body, Method, Request, Response, Server, StatusCode,
};
use prometheus::{Encoder, Registry, TextEncoder};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    ops::Add,
    sync::{
        mpsc::{channel, Sender},
        Arc,
    },
    time::{Duration, Instant},
};
use stream_cancel::{Trigger, Tripwire};
use tokio::timer::Delay;
use tracing::field;

/// Should be greater than 1ms to avoid accidentaly causing infinite loop.
/// Limits minimal acceptable flush_period for PrometheusSinkConfig.
/// 3ms to account for timer and time source inprecisions.
const MIN_FLUSH_PERIOD_MS: u64 = 3; //ms

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct PrometheusSinkConfig {
    #[serde(default = "default_address")]
    pub address: SocketAddr,
    #[serde(default = "default_histogram_buckets")]
    pub buckets: Vec<f64>,
    /// Should be greater than 1 ms to avoid accidentaly causing infinite loop
    #[serde(default = "default_flush_period")]
    pub flush_period: Duration,
}

pub fn default_histogram_buckets() -> Vec<f64> {
    vec![
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ]
}

pub fn default_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};

    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9598)
}

pub fn default_flush_period() -> Duration {
    Duration::from_secs(60)
}

#[typetag::serde(name = "prometheus")]
impl SinkConfig for PrometheusSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        // Checks
        if self.flush_period < Duration::from_millis(MIN_FLUSH_PERIOD_MS) {
            return Err(format!(
                "Flush period for sets must be greater or equal to {} ms",
                MIN_FLUSH_PERIOD_MS
            ));
        }

        // Build
        let sink = Box::new(PrometheusSink::new(self.clone(), acker));
        let healthcheck = Box::new(future::ok(()));

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }
}

struct PrometheusSink {
    registry: Arc<Registry>,
    server_shutdown_trigger: Option<Trigger>,
    flush_channel: Option<Sender<prometheus::IntGauge>>,
    config: PrometheusSinkConfig,
    counters: HashMap<String, prometheus::Counter>,
    gauges: HashMap<String, prometheus::Gauge>,
    histograms: HashMap<String, prometheus::Histogram>,
    sets: HashMap<String, (prometheus::IntGauge, HashSet<String>)>,
    acker: Acker,
}

fn handle(
    req: Request<Body>,
    registry: &Registry,
) -> Box<dyn Future<Item = Response<Body>, Error = hyper::Error> + Send> {
    let mut response = Response::new(Body::empty());

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            let mut buffer = vec![];
            let encoder = TextEncoder::new();
            let metric_families = registry.gather();
            encoder.encode(&metric_families, &mut buffer).unwrap();
            *response.body_mut() = buffer.into();

            response.headers_mut().insert(
                "Content-Type",
                HeaderValue::from_static("text/plain; version=0.0.4"),
            );
        }
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    }

    info!(
        message = "request complete",
        response_code = field::debug(response.status())
    );
    Box::new(future::ok(response))
}

impl PrometheusSink {
    fn new(config: PrometheusSinkConfig, acker: Acker) -> Self {
        Self {
            registry: Arc::new(Registry::new()),
            server_shutdown_trigger: None,
            flush_channel: None,
            config,
            counters: HashMap::new(),
            gauges: HashMap::new(),
            histograms: HashMap::new(),
            sets: HashMap::new(),
            acker,
        }
    }

    fn with_counter(&mut self, name: String, f: impl Fn(&prometheus::Counter)) {
        if let Some(counter) = self.counters.get(&name) {
            f(counter);
        } else {
            let counter = prometheus::Counter::new(name.clone(), name.clone()).unwrap();
            if let Err(e) = self.registry.register(Box::new(counter.clone())) {
                error!("Error registering Prometheus counter: {}", e);
            };
            f(&counter);
            self.counters.insert(name, counter);
        }
    }

    fn with_gauge(&mut self, name: String, f: impl Fn(&prometheus::Gauge)) {
        if let Some(gauge) = self.gauges.get(&name) {
            f(gauge);
        } else {
            let gauge = prometheus::Gauge::new(name.clone(), name.clone()).unwrap();
            if let Err(e) = self.registry.register(Box::new(gauge.clone())) {
                error!("Error registering Prometheus gauge: {}", e);
            };
            f(&gauge);
            self.gauges.insert(name.clone(), gauge);
        }
    }

    fn with_histogram(&mut self, name: String, f: impl Fn(&prometheus::Histogram)) {
        if let Some(hist) = self.histograms.get(&name) {
            f(hist);
        } else {
            let buckets = self.config.buckets.clone();
            let opts = prometheus::HistogramOpts::new(name.clone(), name.clone()).buckets(buckets);
            let hist = prometheus::Histogram::with_opts(opts).unwrap();
            if let Err(e) = self.registry.register(Box::new(hist.clone())) {
                error!("Error registering Prometheus histogram: {}", e);
            };
            f(&hist);
            self.histograms.insert(name, hist);
        }
    }

    /// Calls f with entry corresponding to name. Creates entry if needed.
    fn with_set(
        &mut self,
        name: String,
        f: impl FnOnce(&mut (prometheus::IntGauge, HashSet<String>)),
    ) {
        if let Some(set) = self.sets.get_mut(&name) {
            f(set);
        } else {
            let counter = prometheus::IntGauge::new(name.clone(), name.clone()).unwrap();
            if let Err(e) = self.registry.register(Box::new(counter.clone())) {
                error!("Error registering Prometheus gauge for set: {}", e);
            };

            // Send counter to flusher
            if let Some(ch) = self.flush_channel.as_mut() {
                if let Err(e) = ch.send(counter.clone()) {
                    error!("Error sending Prometheus gauge to flusher: {}", e);
                }
            }

            let mut set = (counter, HashSet::new());
            f(&mut set);
            self.sets.insert(name, set);
        }
    }

    fn start_server_if_needed(&mut self) {
        if self.server_shutdown_trigger.is_some() {
            return;
        }

        let registry = Arc::clone(&self.registry);
        let new_service = move || {
            let registry = Arc::clone(&registry);

            service_fn(move |req| {
                info_span!(
                    "prometheus_server",
                    method = field::debug(req.method()),
                    path = field::debug(req.uri().path()),
                )
                .in_scope(|| handle(req, &registry))
            })
        };

        let (trigger, tripwire) = Tripwire::new();

        let server = Server::bind(&self.config.address)
            .serve(new_service)
            .with_graceful_shutdown(tripwire.clone())
            .map_err(|e| eprintln!("server error: {}", e));

        tokio::spawn(server);
        self.server_shutdown_trigger = Some(trigger);

        self.start_flusher(tripwire);
    }

    /// Flusher will stop when tripwire is done
    fn start_flusher(&mut self, mut tripwire: Tripwire) {
        let (send, recv) = channel();
        self.flush_channel = Some(send);

        let period = self.config.flush_period;
        let mut timer = Delay::new(Instant::now().add(period));

        let mut counters = Vec::new();
        let flusher = future::poll_fn(move || {
            // Check for shutdown
            while tripwire.poll() == Ok(Async::NotReady) {
                // Check messages
                counters.extend(recv.try_iter());

                // Check timer
                try_ready!(timer.poll().map_err(|_| ()));

                // Reset values
                for counter in counters.iter() {
                    counter.set(0);
                }

                // Reset timer
                timer.reset(Instant::now().add(period));
            }
            Ok(Async::Ready(()))
        });

        tokio::spawn(flusher);
    }
}

impl Sink for PrometheusSink {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(
        &mut self,
        event: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        self.start_server_if_needed();

        match event.into_metric() {
            Metric::Counter { name, val, .. } => {
                self.with_counter(name, |counter| counter.inc_by(val))
            }
            Metric::Gauge {
                name,
                val,
                direction,
                ..
            } => self.with_gauge(name, |gauge| match direction {
                None => gauge.set(val),
                Some(Direction::Plus) => gauge.add(val),
                Some(Direction::Minus) => gauge.sub(val),
            }),
            Metric::Histogram {
                name,
                val,
                sample_rate,
                ..
            } => self.with_histogram(name, |hist| {
                for _ in 0..sample_rate {
                    hist.observe(val);
                }
            }),
            Metric::Set { name, val, .. } => {
                // Sets are implemented using promethius integer gauges.
                self.with_set(name, move |&mut (ref mut counter, ref mut set)| {
                    // Check if counter was reseted
                    if counter.get() < set.len() as i64 {
                        // Counter was reseted
                        set.clear();
                    }
                    // Check for uniques of value
                    if set.insert(val) {
                        // Val is a new unique value, therefor gauge should be incremented.
                        counter.add(1);
                        // There is a possiblity that counter was reseted between counter.get counter.add
                        // so that needs to be checked
                        match counter.get() {
                            // Reseted after counter.add
                            0 => set.clear(),
                            // Reseted between first counter.get and counter.add
                            1 if set.len() > 1 => {
                                // Outside world could see metric as 1, if they so happen to
                                // request metrics between counter.add and following counter.set.
                                // But this glitch is ok since either way flushes are scheduled
                                // to happen in periods with best effort basis.
                                counter.set(0);
                                set.clear();
                            }
                            // Everything is fine
                            _ => (),
                        }
                    }
                });
            }
        }

        self.acker.ack(1);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        self.start_server_if_needed();

        Ok(Async::Ready(()))
    }
}

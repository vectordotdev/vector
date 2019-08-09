use crate::{
    buffers::Acker,
    event::{metric::Direction, Metric},
    topology::config::{DataType, SinkConfig},
    Event,
};
use futures::{future, Async, AsyncSink, Future, Sink};
use hyper::{
    header::HeaderValue, service::service_fn, Body, Method, Request, Response, Server, StatusCode,
};
use prometheus::{Encoder, Registry, TextEncoder};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use stream_cancel::{Trigger, Tripwire};
use tracing::field;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct PrometheusSinkConfig {
    #[serde(default = "default_address")]
    pub address: SocketAddr,
    #[serde(default = "default_histogram_buckets")]
    pub buckets: Vec<f64>,
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

#[typetag::serde(name = "prometheus")]
impl SinkConfig for PrometheusSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
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
    config: PrometheusSinkConfig,
    counters: HashMap<String, prometheus::Counter>,
    gauges: HashMap<String, prometheus::Gauge>,
    histograms: HashMap<String, prometheus::Histogram>,
    acker: Acker,
}

fn handle(
    req: Request<Body>,
    registry: &Registry,
) -> Box<Future<Item = Response<Body>, Error = hyper::Error> + Send> {
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
            config,
            counters: HashMap::new(),
            gauges: HashMap::new(),
            histograms: HashMap::new(),
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
            .with_graceful_shutdown(tripwire)
            .map_err(|e| eprintln!("server error: {}", e));

        tokio::spawn(server);
        self.server_shutdown_trigger = Some(trigger);
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
            Metric::Counter {
                name,
                val,
                timestamp: _,
            } => self.with_counter(name, |counter| counter.inc_by(val)),
            Metric::Gauge {
                name,
                val,
                direction,
                timestamp: _,
            } => self.with_gauge(name, |gauge| match direction {
                None => gauge.set(val),
                Some(Direction::Plus) => gauge.add(val),
                Some(Direction::Minus) => gauge.sub(val),
            }),
            Metric::Histogram {
                name,
                val,
                sample_rate,
                timestamp: _,
            } => self.with_histogram(name, |hist| {
                for _ in 0..sample_rate {
                    hist.observe(val);
                }
            }),
            Metric::Set { .. } => {
                trace!("Sets are not supported in Prometheus sink");
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

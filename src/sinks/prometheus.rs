use crate::{
    buffers::Acker,
    event::{metric::Direction, Metric},
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
use tokio_trace::field;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct PrometheusSinkConfig {
    #[serde(default = "default_address")]
    pub address: SocketAddr,
    pub counters: Vec<String>,
    pub gauges: Vec<String>,
}

pub fn default_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};

    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9999)
}

#[typetag::serde(name = "prometheus")]
impl crate::topology::config::SinkConfig for PrometheusSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = Box::new(PrometheusSink::new(
            self.address,
            self.counters.clone(),
            self.gauges.clone(),
            acker,
        ));
        let healthcheck = Box::new(future::ok(()));

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> crate::topology::config::DataType {
        crate::topology::config::DataType::Metric
    }
}

struct PrometheusSink {
    registry: Arc<Registry>,
    server_shutdown_trigger: Option<Trigger>,
    address: SocketAddr,
    counters: HashMap<String, prometheus::Counter>,
    gauges: HashMap<String, prometheus::Gauge>,
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
    fn new(address: SocketAddr, counters: Vec<String>, gauges: Vec<String>, acker: Acker) -> Self {
        let registry = Registry::new();

        let counters = counters
            .into_iter()
            .map(|name| {
                let counter = prometheus::Counter::new(name.clone(), name.clone()).unwrap();
                registry.register(Box::new(counter.clone())).unwrap();

                (name, counter)
            })
            .collect();

        let gauges = gauges
            .into_iter()
            .map(|name| {
                let gauge = prometheus::Gauge::new(name.clone(), name.clone()).unwrap();
                registry.register(Box::new(gauge.clone())).unwrap();

                (name, gauge)
            })
            .collect();

        Self {
            registry: Arc::new(registry),
            server_shutdown_trigger: None,
            address,
            counters,
            gauges,
            acker,
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
                .enter(|| handle(req, &registry))
            })
        };

        let (trigger, tripwire) = Tripwire::new();

        let server = Server::bind(&self.address)
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

        match event.as_metric() {
            Metric::Counter {
                name,
                val,
                // TODO: take sampling into account
                sampling: _,
            } => {
                if let Some(counter) = self.counters.get_mut(name) {
                    counter.inc_by(*val as f64);
                }
            }
            Metric::Gauge {
                name,
                val,
                direction,
            } => {
                if let Some(gauge) = self.gauges.get_mut(name) {
                    let val = *val as f64;
                    match direction {
                        None => gauge.set(val),
                        Some(Direction::Plus) => gauge.add(val),
                        Some(Direction::Minus) => gauge.sub(val),
                    }
                }
            }
            _ => {
                // TODO: support all the metric types
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

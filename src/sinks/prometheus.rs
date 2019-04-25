use crate::buffers::Acker;
use crate::Record;
use futures::{future, Async, AsyncSink, Future, Sink};
use hyper::service::service_fn;
use hyper::{header::HeaderValue, Body, Method, Request, Response, Server, StatusCode};
use prometheus::{Encoder, Registry, TextEncoder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use stream_cancel::{Trigger, Tripwire};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct PrometheusSinkConfig {
    #[serde(default = "default_address")]
    pub address: SocketAddr,
    pub counters: Vec<Counter>,
    pub gauges: Vec<Gauge>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq, Hash)]
pub struct Counter {
    key: Atom,
    label: String,
    doc: String,
    parse_value: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq, Hash)]
pub struct Gauge {
    key: Atom,
    label: String,
    doc: String,
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
}

struct PrometheusSink {
    registry: Arc<Registry>,
    server_shutdown_trigger: Option<Trigger>,
    address: SocketAddr,
    counters: HashMap<Counter, prometheus::Counter>,
    gauges: HashMap<Gauge, prometheus::Gauge>,
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

    Box::new(future::ok(response))
}

impl PrometheusSink {
    fn new(address: SocketAddr, counters: Vec<Counter>, gauges: Vec<Gauge>, acker: Acker) -> Self {
        let registry = Registry::new();

        let counters = counters
            .into_iter()
            .map(|config| {
                let counter =
                    prometheus::Counter::new(config.label.clone(), config.doc.clone()).unwrap();
                registry.register(Box::new(counter.clone())).unwrap();

                (config, counter)
            })
            .collect();

        let gauges = gauges
            .into_iter()
            .map(|config| {
                let gauge =
                    prometheus::Gauge::new(config.label.clone(), config.doc.clone()).unwrap();
                registry.register(Box::new(gauge.clone())).unwrap();

                (config, gauge)
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

            service_fn(move |req| handle(req, &registry))
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
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(
        &mut self,
        record: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        self.start_server_if_needed();

        for (field, counter) in &self.counters {
            if let Some(val) = record.get(&field.key) {
                if field.parse_value {
                    let val = val.to_string_lossy();

                    if let Ok(count) = val.parse() {
                        counter.inc_by(count);
                    } else {
                        warn!(
                            "Unable to parse value from field {} with value {}",
                            field.key, val
                        );
                    }
                } else {
                    counter.inc_by(1.0);
                }
            }
        }

        for (field, gauge) in &self.gauges {
            if let Some(val) = record.get(&field.key) {
                let val = val.to_string_lossy();

                if let Ok(count) = val.parse() {
                    gauge.add(count);
                } else {
                    warn!(
                        "Unable to parse value from field {} with value {}",
                        field.key, val
                    );
                }
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

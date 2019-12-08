use crate::{
    buffers::Acker,
    event::metric::{MetricKind, MetricValue},
    sinks::util::MetricEntry,
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
    Event,
};
use futures::{future, Async, AsyncSink, Future, Sink};
use hyper::{
    header::HeaderValue, service::service_fn, Body, Method, Request, Response, Server, StatusCode,
};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, RwLock},
};
use stream_cancel::{Trigger, Tripwire};
use tracing::field;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct PrometheusSinkConfig {
    pub namespace: String,
    #[serde(default = "default_address")]
    pub address: SocketAddr,
    #[serde(default = "default_histogram_buckets")]
    pub buckets: Vec<f64>,
    #[serde(default = "default_flush_period")]
    pub flush_period_sec: u64,
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

pub fn default_flush_period() -> u64 {
    60
}

inventory::submit! {
    SinkDescription::new_without_default::<PrometheusSinkConfig>("prometheus")
}

#[typetag::serde(name = "prometheus")]
impl SinkConfig for PrometheusSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = Box::new(PrometheusSink::new(self.clone(), cx.acker()));
        let healthcheck = Box::new(future::ok(()));

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "prometheus"
    }
}

struct PrometheusSink {
    server_shutdown_trigger: Option<Trigger>,
    config: PrometheusSinkConfig,
    metrics: Arc<RwLock<HashSet<MetricEntry>>>,
    acker: Acker,
}

fn encode_namespace(namespace: &str, name: &str) -> String {
    if !namespace.is_empty() {
        format!("{}_{}", namespace, name)
    } else {
        name.to_string()
    }
}

fn encode_tags(tags: &Option<HashMap<String, String>>) -> String {
    if let Some(tags) = tags {
        let mut parts: Vec<_> = tags
            .iter()
            .map(|(name, value)| format!("{}=\"{}\"", name, value))
            .collect();

        parts.sort();
        format!("{{{}}}", parts.join(","))
    } else {
        String::from("")
    }
}

fn encode_tags_with_extra(
    tags: &Option<HashMap<String, String>>,
    label: String,
    value: String,
) -> String {
    let mut parts: Vec<_> = if let Some(tags) = tags {
        tags.iter()
            .chain(vec![(&label, &value)])
            .map(|(name, value)| format!("{}=\"{}\"", name, value))
            .collect()
    } else {
        vec![format!("{}=\"{}\"", label, value)]
    };

    parts.sort();
    format!("{{{}}}", parts.join(","))
}

fn handle(
    req: Request<Body>,
    namespace: &str,
    metrics: &HashSet<MetricEntry>,
) -> Box<dyn Future<Item = Response<Body>, Error = hyper::Error> + Send> {
    let mut response = Response::new(Body::empty());

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            let mut w = Vec::new();

            let mut metrics: Vec<_> = metrics.iter().collect();
            metrics.sort_by(|a, b| a.partial_cmp(b).unwrap());

            for metric in metrics {
                let metric = &metric.0;
                if metric.kind.is_absolute() {
                    let name = &metric.name;
                    let tags = &metric.tags;
                    let fullname = encode_namespace(&namespace, &name);
                    // todo: metric families
                    match &metric.value {
                        MetricValue::Counter { value } => {
                            let tags = encode_tags(tags);
                            writeln!(&mut w, "# HELP {} {}", fullname, name).unwrap();
                            writeln!(&mut w, "# TYPE {} counter", fullname).unwrap();
                            writeln!(&mut w, "{}{} {}", fullname, tags, value).unwrap();
                        }
                        MetricValue::Gauge { value } => {
                            let tags = encode_tags(tags);
                            writeln!(&mut w, "# HELP {} {}", fullname, name).unwrap();
                            writeln!(&mut w, "# TYPE {} gauge", fullname).unwrap();
                            writeln!(&mut w, "{}{} {}", fullname, tags, value).unwrap();
                        }
                        MetricValue::Set { values } => {
                            let tags = encode_tags(tags);
                            writeln!(&mut w, "# HELP {} {}", fullname, name).unwrap();
                            writeln!(&mut w, "# TYPE {} gauge", fullname).unwrap();
                            writeln!(&mut w, "{}{} {}", fullname, tags, values.len()).unwrap();
                        }
                        MetricValue::AggregatedHistogram {
                            buckets,
                            counts,
                            count,
                            sum,
                        } => {
                            writeln!(&mut w, "# HELP {} {}", fullname, name).unwrap();
                            writeln!(&mut w, "# TYPE {} histogram", fullname).unwrap();
                            for (b, c) in buckets.iter().zip(counts.iter()) {
                                writeln!(
                                    &mut w,
                                    "{}_bucket{} {}",
                                    fullname,
                                    encode_tags_with_extra(tags, "le".to_string(), b.to_string()),
                                    c
                                )
                                .unwrap();
                            }
                            writeln!(
                                &mut w,
                                "{}_bucket{} {}",
                                fullname,
                                encode_tags_with_extra(tags, "le".to_string(), "+Inf".to_string()),
                                count
                            )
                            .unwrap();
                            writeln!(&mut w, "{}_sum{} {}", fullname, encode_tags(tags), sum)
                                .unwrap();
                            writeln!(&mut w, "{}_count{} {}", fullname, encode_tags(tags), count)
                                .unwrap();
                        }
                        MetricValue::AggregatedSummary {
                            quantiles,
                            values,
                            count,
                            sum,
                        } => {
                            writeln!(&mut w, "# HELP {} {}", fullname, name).unwrap();
                            writeln!(&mut w, "# TYPE {} summary", fullname).unwrap();
                            for (q, v) in quantiles.iter().zip(values.iter()) {
                                writeln!(
                                    &mut w,
                                    "{}{} {}",
                                    fullname,
                                    encode_tags_with_extra(
                                        tags,
                                        "quantile".to_string(),
                                        q.to_string()
                                    ),
                                    v
                                )
                                .unwrap();
                            }
                            writeln!(&mut w, "{}_sum{} {}", fullname, encode_tags(tags), sum)
                                .unwrap();
                            writeln!(&mut w, "{}_count{} {}", fullname, encode_tags(tags), count)
                                .unwrap();
                        }
                        _ => {}
                    }
                }
            }

            *response.body_mut() = w.into();

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
            server_shutdown_trigger: None,
            config,
            metrics: Arc::new(RwLock::new(HashSet::new())),
            acker,
        }
    }

    fn start_server_if_needed(&mut self) {
        if self.server_shutdown_trigger.is_some() {
            return;
        }

        let metrics = Arc::clone(&self.metrics);
        let namespace = self.config.namespace.clone();
        let new_service = move || {
            let metrics = Arc::clone(&metrics);
            let namespace = namespace.clone();

            service_fn(move |req| {
                let metrics = metrics.read().unwrap();
                info_span!(
                    "prometheus_server",
                    method = field::debug(req.method()),
                    path = field::debug(req.uri().path()),
                )
                .in_scope(|| handle(req, &namespace, &metrics))
            })
        };

        let (trigger, tripwire) = Tripwire::new();

        let server = Server::bind(&self.config.address)
            .serve(new_service)
            .with_graceful_shutdown(tripwire.clone())
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

        let item = event.into_metric();
        let mut metrics = self.metrics.write().unwrap();

        match item.kind {
            // todo: sets flush
            MetricKind::Incremental => {
                let new = MetricEntry(item.clone().into_absolute());
                if let Some(MetricEntry(mut existing)) = metrics.take(&new) {
                    existing.add(&item);
                    metrics.insert(MetricEntry(existing));
                } else {
                    metrics.insert(new);
                };
            }
            MetricKind::Absolute => {
                let new = MetricEntry(item);
                metrics.replace(new);
            }
        };

        self.acker.ack(1);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        self.start_server_if_needed();

        Ok(Async::Ready(()))
    }
}

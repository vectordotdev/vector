use crate::{
    buffers::Acker,
    config::{DataType, GenerateConfig, Resource, SinkConfig, SinkContext, SinkDescription},
    event::metric::MetricKind,
    sinks::{
        util::{statistic::validate_quantiles, MetricEntry, StreamSink},
        Healthcheck, VectorSink,
    },
    Event,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::{future, stream::BoxStream, FutureExt, StreamExt, TryFutureExt};
use hyper::{
    header::HeaderValue,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    collections::HashSet,
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, RwLock},
};
use stream_cancel::{Trigger, Tripwire};

use super::collector::{self, MetricCollector as _};

const MIN_FLUSH_PERIOD_SECS: u64 = 1;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Flush period for sets must be greater or equal to {} secs", min))]
    FlushPeriodTooShort { min: u64 },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrometheusExporterConfig {
    pub default_namespace: Option<String>,
    #[serde(default = "default_address")]
    pub address: SocketAddr,
    #[serde(default = "super::default_histogram_buckets")]
    pub buckets: Vec<f64>,
    #[serde(default = "super::default_summary_quantiles")]
    pub quantiles: Vec<f64>,
    #[serde(default = "default_flush_period_secs")]
    pub flush_period_secs: u64,
}

impl std::default::Default for PrometheusExporterConfig {
    fn default() -> Self {
        Self {
            default_namespace: None,
            address: default_address(),
            buckets: super::default_histogram_buckets(),
            quantiles: super::default_summary_quantiles(),
            flush_period_secs: default_flush_period_secs(),
        }
    }
}

fn default_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};

    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9598)
}

fn default_flush_period_secs() -> u64 {
    60
}

inventory::submit! {
    SinkDescription::new::<PrometheusExporterConfig>("prometheus")
}

inventory::submit! {
    SinkDescription::new::<PrometheusExporterConfig>("prometheus_exporter")
}

impl GenerateConfig for PrometheusExporterConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(&Self::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_exporter")]
impl SinkConfig for PrometheusExporterConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        if self.flush_period_secs < MIN_FLUSH_PERIOD_SECS {
            return Err(Box::new(BuildError::FlushPeriodTooShort {
                min: MIN_FLUSH_PERIOD_SECS,
            }));
        }

        validate_quantiles(&self.quantiles)?;

        let sink = PrometheusExporter::new(self.clone(), cx.acker());
        let healthcheck = future::ok(()).boxed();

        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "prometheus_exporter"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![self.address.into()]
    }
}

// Add a compatibility alias to avoid breaking existing configs
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PrometheusCompatConfig {
    #[serde(flatten)]
    config: PrometheusExporterConfig,
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus")]
impl SinkConfig for PrometheusCompatConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        self.config.build(cx).await
    }

    fn input_type(&self) -> DataType {
        self.config.input_type()
    }

    fn sink_type(&self) -> &'static str {
        "prometheus"
    }

    fn resources(&self) -> Vec<Resource> {
        self.config.resources()
    }
}

struct PrometheusExporter {
    server_shutdown_trigger: Option<Trigger>,
    config: PrometheusExporterConfig,
    metrics: Arc<RwLock<IndexSet<MetricEntry>>>,
    last_flush_timestamp: Arc<RwLock<i64>>,
    acker: Acker,
}

fn handle(
    req: Request<Body>,
    default_namespace: Option<&str>,
    buckets: &[f64],
    quantiles: &[f64],
    expired: bool,
    metrics: &IndexSet<MetricEntry>,
) -> Response<Body> {
    let mut response = Response::new(Body::empty());

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            let mut s = collector::StringCollector::new();

            // output headers only once
            let mut processed_headers = HashSet::new();

            for metric in metrics {
                let name = &metric.0.name;
                if !processed_headers.contains(&name) {
                    s.encode_header(default_namespace, &metric.0);
                    processed_headers.insert(name);
                };

                s.encode_metric(default_namespace, &buckets, quantiles, expired, &metric.0);
            }

            *response.body_mut() = s.result.into();

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
        message = "Request complete.",
        response_code = ?response.status()
    );

    response
}

impl PrometheusExporter {
    fn new(config: PrometheusExporterConfig, acker: Acker) -> Self {
        Self {
            server_shutdown_trigger: None,
            config,
            metrics: Arc::new(RwLock::new(IndexSet::new())),
            last_flush_timestamp: Arc::new(RwLock::new(Utc::now().timestamp())),
            acker,
        }
    }

    fn start_server_if_needed(&mut self) {
        if self.server_shutdown_trigger.is_some() {
            return;
        }

        let metrics = Arc::clone(&self.metrics);
        let default_namespace = self.config.default_namespace.clone();
        let buckets = self.config.buckets.clone();
        let quantiles = self.config.quantiles.clone();
        let last_flush_timestamp = Arc::clone(&self.last_flush_timestamp);
        let flush_period_secs = self.config.flush_period_secs;

        let new_service = make_service_fn(move |_| {
            let metrics = Arc::clone(&metrics);
            let default_namespace = default_namespace.clone();
            let buckets = buckets.clone();
            let quantiles = quantiles.clone();
            let last_flush_timestamp = Arc::clone(&last_flush_timestamp);
            let flush_period_secs = flush_period_secs;

            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let metrics = metrics.read().unwrap();
                    let last_flush_timestamp = last_flush_timestamp.read().unwrap();
                    let interval = (Utc::now().timestamp() - *last_flush_timestamp) as u64;
                    let expired = interval > flush_period_secs;

                    let response = info_span!(
                        "prometheus_server",
                        method = ?req.method(),
                        path = ?req.uri().path(),
                    )
                    .in_scope(|| {
                        handle(
                            req,
                            default_namespace.as_deref(),
                            &buckets,
                            &quantiles,
                            expired,
                            &metrics,
                        )
                    });

                    future::ok::<_, Infallible>(response)
                }))
            }
        });

        let (trigger, tripwire) = Tripwire::new();

        let server = Server::bind(&self.config.address)
            .serve(new_service)
            .with_graceful_shutdown(tripwire.then(crate::stream::tripwire_handler))
            .map_err(|error| eprintln!("Server error: {}", error));

        tokio::spawn(server);
        self.server_shutdown_trigger = Some(trigger);
    }
}

#[async_trait]
impl StreamSink for PrometheusExporter {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.start_server_if_needed();
        while let Some(event) = input.next().await {
            let item = event.into_metric();
            let mut metrics = self.metrics.write().unwrap();

            match item.kind {
                MetricKind::Incremental => {
                    let new = MetricEntry(item.to_absolute());
                    if let Some(MetricEntry(mut existing)) = metrics.take(&new) {
                        if item.value.is_set() {
                            // sets need to be expired from time to time
                            // because otherwise they could grow infinitelly
                            let now = Utc::now().timestamp();
                            let interval = now - *self.last_flush_timestamp.read().unwrap();
                            if interval > self.config.flush_period_secs as i64 {
                                *self.last_flush_timestamp.write().unwrap() = now;
                                existing.reset();
                            }
                        }
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
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PrometheusExporterConfig>();
    }
}

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests {
    use super::*;
    use crate::{
        config::SinkContext,
        event::{Metric, MetricValue},
        http::HttpClient,
    };
    use futures::{stream, task::Poll};
    use serde_json::Value;
    use std::{pin::Pin, task::Context};
    use tokio::time::Duration;

    #[tokio::test]
    async fn prometheus_scrapes_metrics() {
        crate::test_util::trace_init();

        let start = Utc::now().timestamp();
        let address = "127.0.0.1:9101";

        let config = PrometheusExporterConfig {
            address: address.parse().unwrap(),
            ..Default::default()
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let (name, events) = make_gauges(123.4, 1);
        let stream = DeliverEventsAndPause { events };
        tokio::spawn(sink.run(stream));
        // Wait a bit for the prometheus server to scrape the metrics
        tokio::time::delay_for(Duration::from_secs(2)).await;

        // Now try to download them from prometheus
        let result = prometheus_query(&name).await;

        let data = &result["data"]["result"][0];
        assert_eq!(data["metric"]["__name__"], Value::String(name));
        assert_eq!(data["metric"]["instance"], Value::String(address.into()));
        assert_eq!(
            data["metric"]["some_tag"],
            Value::String("some_value".into())
        );
        assert!(data["value"][0].as_f64().unwrap() >= start as f64);
        assert_eq!(data["value"][1], Value::String("123.4".into()));
    }

    async fn prometheus_query(query: &str) -> Value {
        let uri = format!("http://127.0.0.1:9090/api/v1/query?query={}", query)
            .parse::<http::Uri>()
            .expect("Invalid query URL");
        let request = Request::post(uri)
            .body(Body::empty())
            .expect("Error creating request.");
        let result = HttpClient::new(None)
            .unwrap()
            .send(request)
            .await
            .expect("Could not fetch query");
        let result = hyper::body::to_bytes(result.into_body())
            .await
            .expect("Error fetching body");
        let result = String::from_utf8_lossy(&result).into_owned();
        serde_json::from_str(&result).expect("Invalid JSON from prometheus")
    }

    #[pin_project::pin_project]
    struct DeliverEventsAndPause<I> {
        events: I,
    }

    impl<I: Iterator<Item = Event>> stream::Stream for DeliverEventsAndPause<I> {
        type Item = Event;
        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let this = self.project();
            match this.events.next() {
                Some(event) => Poll::Ready(Some(event)),
                None => Poll::Pending,
            }
        }
    }

    fn make_gauges(value: f64, count: usize) -> (String, impl Iterator<Item = Event>) {
        let name = format!("gauge_{}", crate::test_util::random_string(16));
        let name2 = name.clone();
        let events = (0..count).map(move |_| {
            Metric {
                name: name2.clone(),
                namespace: None,
                timestamp: None,
                tags: Some(
                    vec![("some_tag".into(), "some_value".into())]
                        .into_iter()
                        .collect(),
                ),
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }
            .into()
        });
        (name, events)
    }
}

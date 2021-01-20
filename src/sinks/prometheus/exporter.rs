use crate::{
    buffers::Acker,
    config::{DataType, GenerateConfig, Resource, SinkConfig, SinkContext, SinkDescription},
    event::metric::MetricKind,
    internal_events::PrometheusServerRequestComplete,
    sinks::{
        util::{statistic::validate_quantiles, MetricEntry, StreamSink},
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsConfig},
    Event,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::{future, stream::BoxStream, FutureExt, StreamExt};
use hyper::{
    header::HeaderValue,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
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
    #[serde(alias = "namespace")]
    pub default_namespace: Option<String>,
    #[serde(default = "default_address")]
    pub address: SocketAddr,
    pub tls: Option<TlsConfig>,
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
            tls: None,
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
        vec![Resource::tcp(self.address)]
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
    metrics: Arc<RwLock<ExpiringMetrics>>,
    acker: Acker,
}

struct ExpiringMetrics {
    map: IndexMap<MetricEntry, bool>,
    last_flush_timestamp: i64,
}

fn handle(
    req: Request<Body>,
    default_namespace: Option<&str>,
    buckets: &[f64],
    quantiles: &[f64],
    expired: bool,
    metrics: &IndexMap<MetricEntry, bool>,
) -> Response<Body> {
    let mut response = Response::new(Body::empty());

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            let mut s = collector::StringCollector::new();

            for (MetricEntry(metric), _) in metrics {
                s.encode_metric(default_namespace, &buckets, quantiles, expired, metric);
            }

            *response.body_mut() = s.finish().into();

            response.headers_mut().insert(
                "Content-Type",
                HeaderValue::from_static("text/plain; version=0.0.4"),
            );
        }
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    }

    response
}

impl PrometheusExporter {
    fn new(config: PrometheusExporterConfig, acker: Acker) -> Self {
        Self {
            server_shutdown_trigger: None,
            config,
            metrics: Arc::new(RwLock::new(ExpiringMetrics {
                map: IndexMap::new(),
                last_flush_timestamp: Utc::now().timestamp(),
            })),
            acker,
        }
    }

    async fn start_server_if_needed(&mut self) {
        if self.server_shutdown_trigger.is_some() {
            return;
        }

        let metrics = Arc::clone(&self.metrics);
        let default_namespace = self.config.default_namespace.clone();
        let buckets = self.config.buckets.clone();
        let quantiles = self.config.quantiles.clone();
        let flush_period_secs = self.config.flush_period_secs;

        let new_service = make_service_fn(move |_| {
            let metrics = Arc::clone(&metrics);
            let default_namespace = default_namespace.clone();
            let buckets = buckets.clone();
            let quantiles = quantiles.clone();
            let flush_period_secs = flush_period_secs;

            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let metrics = metrics.read().unwrap();
                    let interval = (Utc::now().timestamp() - metrics.last_flush_timestamp) as u64;
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
                            &metrics.map,
                        )
                    });

                    emit!(PrometheusServerRequestComplete {
                        status_code: response.status(),
                    });

                    future::ok::<_, Infallible>(response)
                }))
            }
        });

        let (trigger, tripwire) = Tripwire::new();

        let tls = self.config.tls.clone();
        let address = self.config.address;

        tokio::spawn(async move {
            let tls = MaybeTlsSettings::from_config(&tls, true)
                .map_err(|error| eprintln!("Server TLS error: {}", error))?;
            let listener = tls
                .bind(&address)
                .await
                .map_err(|error| eprintln!("Server bind error: {}", error))?;

            Server::builder(hyper::server::accept::from_stream(listener.accept_stream()))
                .serve(new_service)
                .with_graceful_shutdown(tripwire.then(crate::stream::tripwire_handler))
                .await
                .map_err(|error| eprintln!("Server error: {}", error))?;

            Ok::<(), ()>(())
        });

        self.server_shutdown_trigger = Some(trigger);
    }
}

#[async_trait]
impl StreamSink for PrometheusExporter {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.start_server_if_needed().await;
        while let Some(event) = input.next().await {
            let item = event.into_metric();
            let mut metrics = self.metrics.write().unwrap();

            // sets need to be expired from time to time
            // because otherwise they could grow infinitelly
            let now = Utc::now().timestamp();
            let interval = now - metrics.last_flush_timestamp;
            if interval > self.config.flush_period_secs as i64 {
                metrics.last_flush_timestamp = now;

                metrics.map = metrics
                    .map
                    .drain(..)
                    .map(|(MetricEntry(mut metric), is_incremental_set)| {
                        if is_incremental_set {
                            metric.data.reset();
                        }
                        (MetricEntry(metric), is_incremental_set)
                    })
                    .collect();
            }

            match item.data.kind {
                MetricKind::Incremental => {
                    let mut new = MetricEntry(item.to_absolute());
                    if let Some((MetricEntry(mut existing), _)) = metrics.map.remove_entry(&new) {
                        existing.data.add(&item.data);
                        new = MetricEntry(existing);
                    }
                    metrics.map.insert(new, item.data.value.is_set());
                }
                MetricKind::Absolute => {
                    let new = MetricEntry(item);
                    metrics.map.remove(&new);
                    metrics.map.insert(new, false);
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
    use crate::{
        event::metric::{Metric, MetricData, MetricSeries, MetricValue},
        http::HttpClient,
        test_util::{random_string, trace_init},
        tls::MaybeTlsSettings,
    };
    use tokio::{sync::mpsc, time};

    const PROMETHEUS_ADDRESS_TLS: &str = "127.0.0.1:9102";

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PrometheusExporterConfig>();
    }

    #[tokio::test]
    async fn prometheus_tls() {
        trace_init();

        let mut tls_config = TlsConfig::test_config();
        tls_config.options.verify_hostname = Some(false);

        let config = PrometheusExporterConfig {
            address: PROMETHEUS_ADDRESS_TLS.parse().unwrap(),
            tls: Some(tls_config.clone()),
            ..Default::default()
        };
        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(sink.run(Box::pin(rx)));

        let (_name, event) = create_metric_gauge(None, 123.4);
        tx.send(event).expect("Failed to send.");
        let (_name, event) = tests::create_metric_set(None, vec!["0", "1", "2"]);
        tx.send(event).expect("Failed to send.");

        time::delay_for(time::Duration::from_millis(100)).await;

        let request = Request::get(format!("https://{}/metrics", PROMETHEUS_ADDRESS_TLS))
            .body(Body::empty())
            .expect("Error creating request.");
        let settings = MaybeTlsSettings::from_config(&Some(tls_config), false).unwrap();
        let result = HttpClient::new(settings)
            .unwrap()
            .send(request)
            .await
            .expect("Could not fetch query");

        assert!(result.status().is_success());
    }

    pub fn create_metric_gauge(name: Option<String>, value: f64) -> (String, Event) {
        create_metric(name, MetricValue::Gauge { value })
    }

    pub fn create_metric_set(name: Option<String>, values: Vec<&'static str>) -> (String, Event) {
        create_metric(
            name,
            MetricValue::Set {
                values: values.into_iter().map(Into::into).collect(),
            },
        )
    }

    pub fn create_metric(name: Option<String>, value: MetricValue) -> (String, Event) {
        let name = name.unwrap_or_else(|| format!("vector_set_{}", random_string(16)));
        let event = Metric::new(
            name.clone(),
            None,
            None,
            Some(
                vec![("some_tag".to_owned(), "some_value".to_owned())]
                    .into_iter()
                    .collect(),
            ),
            MetricKind::Incremental,
            value,
        )
        .into();
        (name, event)
    }

    #[tokio::test]
    async fn sink_absolute() {
        let config = PrometheusExporterConfig {
            address: PROMETHEUS_ADDRESS_TLS.parse().unwrap(),
            tls: None,
            ..Default::default()
        };
        let cx = SinkContext::new_test();

        let mut sink = PrometheusExporter::new(config, cx.acker());

        let m1 = Metric::new(
            "absolute".to_string(),
            None,
            None,
            Some(
                vec![("tag1".to_owned(), "value1".to_owned())]
                    .into_iter()
                    .collect(),
            ),
            MetricKind::Absolute,
            MetricValue::Counter { value: 32. },
        );

        let m2 = Metric {
            series: MetricSeries {
                tags: Some(
                    vec![("tag1".to_owned(), "value2".to_owned())]
                        .into_iter()
                        .collect(),
                ),
                ..m1.series.clone()
            },
            data: m1.data.clone(),
        };

        let metrics = vec![
            Event::Metric(Metric {
                series: m1.series.clone(),
                data: MetricData {
                    value: MetricValue::Counter { value: 32. },
                    ..m1.data.clone()
                },
            }),
            Event::Metric(Metric {
                series: m2.series.clone(),
                data: MetricData {
                    value: MetricValue::Counter { value: 33. },
                    ..m2.data.clone()
                },
            }),
            Event::Metric(Metric {
                series: m1.series.clone(),
                data: MetricData {
                    value: MetricValue::Counter { value: 40. },
                    ..m1.data.clone()
                },
            }),
        ];

        sink.run(Box::pin(futures::stream::iter(metrics)))
            .await
            .unwrap();

        let map = &sink.metrics.read().unwrap().map;

        assert_eq!(
            map.get_full(&MetricEntry(m1)).unwrap().1 .0.data.value,
            MetricValue::Counter { value: 40. }
        );

        assert_eq!(
            map.get_full(&MetricEntry(m2)).unwrap().1 .0.data.value,
            MetricValue::Counter { value: 33. }
        );
    }
}

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests {
    use super::*;
    use crate::{http::HttpClient, test_util::trace_init};
    use chrono::Utc;
    use serde_json::Value;
    use tokio::{sync::mpsc, time};

    const PROMETHEUS_ADDRESS: &str = "127.0.0.1:9101";

    #[tokio::test]
    async fn prometheus_metrics() {
        trace_init();

        prometheus_scrapes_metrics().await;
        time::delay_for(time::Duration::from_millis(500)).await;
        reset_on_flush_period().await;
    }

    async fn prometheus_scrapes_metrics() {
        let start = Utc::now().timestamp();

        let config = PrometheusExporterConfig {
            address: PROMETHEUS_ADDRESS.parse().unwrap(),
            ..Default::default()
        };
        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(sink.run(Box::pin(rx)));

        let (name, event) = tests::create_metric_gauge(None, 123.4);
        tx.send(event).expect("Failed to send.");

        // Wait a bit for the prometheus server to scrape the metrics
        time::delay_for(time::Duration::from_secs(2)).await;

        // Now try to download them from prometheus
        let result = prometheus_query(&name).await;

        let data = &result["data"]["result"][0];
        assert_eq!(data["metric"]["__name__"], Value::String(name));
        assert_eq!(
            data["metric"]["instance"],
            Value::String(PROMETHEUS_ADDRESS.into())
        );
        assert_eq!(
            data["metric"]["some_tag"],
            Value::String("some_value".into())
        );
        assert!(data["value"][0].as_f64().unwrap() >= start as f64);
        assert_eq!(data["value"][1], Value::String("123.4".into()));
    }

    async fn reset_on_flush_period() {
        let config = PrometheusExporterConfig {
            address: PROMETHEUS_ADDRESS.parse().unwrap(),
            flush_period_secs: 3,
            ..Default::default()
        };
        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(sink.run(Box::pin(rx)));

        let (name1, event) = tests::create_metric_set(None, vec!["0", "1", "2"]);
        tx.send(event).expect("Failed to send.");
        let (name2, event) = tests::create_metric_set(None, vec!["3", "4", "5"]);
        tx.send(event).expect("Failed to send.");

        // Wait a bit for the prometheus server to scrape the metrics
        time::delay_for(time::Duration::from_secs(2)).await;

        // Now try to download them from prometheus
        let result = prometheus_query(&name1).await;
        assert_eq!(
            result["data"]["result"][0]["value"][1],
            Value::String("3".into())
        );
        let result = prometheus_query(&name2).await;
        assert_eq!(
            result["data"]["result"][0]["value"][1],
            Value::String("3".into())
        );

        // Wait a bit for expired metrics
        time::delay_for(time::Duration::from_secs(3)).await;

        let (name1, event) = tests::create_metric_set(Some(name1), vec!["6", "7"]);
        tx.send(event).expect("Failed to send.");
        let (name2, event) = tests::create_metric_set(Some(name2), vec!["8", "9"]);
        tx.send(event).expect("Failed to send.");

        // Wait a bit for the prometheus server to scrape the metrics
        time::delay_for(time::Duration::from_secs(2)).await;

        // Now try to download them from prometheus
        let result = prometheus_query(&name1).await;
        assert_eq!(
            result["data"]["result"][0]["value"][1],
            Value::String("2".into())
        );
        let result = prometheus_query(&name2).await;
        assert_eq!(
            result["data"]["result"][0]["value"][1],
            Value::String("2".into())
        );
    }

    async fn prometheus_query(query: &str) -> Value {
        let url = format!("http://127.0.0.1:9090/api/v1/query?query={}", query);
        let request = Request::post(url)
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
        let result = String::from_utf8_lossy(&result);
        serde_json::from_str(result.as_ref()).expect("Invalid JSON from prometheus")
    }
}

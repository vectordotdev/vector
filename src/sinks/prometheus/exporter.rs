use std::{
    convert::Infallible,
    hash::Hash,
    mem::{discriminant, Discriminant},
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use futures::{future, stream::BoxStream, FutureExt, StreamExt};
use hyper::{
    header::HeaderValue,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use snafu::Snafu;
use stream_cancel::{Trigger, Tripwire};
use vector_core::{buffers::Acker, event::metric::MetricSeries};

use super::collector::{MetricCollector, StringCollector};
use crate::{
    config::{DataType, GenerateConfig, Resource, SinkConfig, SinkContext, SinkDescription},
    event::{
        metric::{Metric, MetricData, MetricKind, MetricValue},
        Event,
    },
    internal_events::PrometheusServerRequestComplete,
    sinks::{
        util::{
            buffer::metrics::{MetricNormalize, MetricNormalizer, MetricSet},
            statistic::validate_quantiles,
            StreamSink,
        },
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsConfig},
};

const MIN_FLUSH_PERIOD_SECS: u64 = 1;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Flush period for sets must be greater or equal to {} secs", min))]
    FlushPeriodTooShort { min: u64 },
}

#[serde_as]
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
    #[serde(default = "default_distributions_as_summaries")]
    pub distributions_as_summaries: bool,
    #[serde(default = "default_flush_period_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    pub flush_period_secs: Duration,
}

impl Default for PrometheusExporterConfig {
    fn default() -> Self {
        Self {
            default_namespace: None,
            address: default_address(),
            tls: None,
            buckets: super::default_histogram_buckets(),
            quantiles: super::default_summary_quantiles(),
            distributions_as_summaries: default_distributions_as_summaries(),
            flush_period_secs: default_flush_period_secs(),
        }
    }
}

fn default_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};

    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9598)
}

const fn default_distributions_as_summaries() -> bool {
    false
}

const fn default_flush_period_secs() -> Duration {
    Duration::from_secs(60)
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
        if self.flush_period_secs.as_secs() < MIN_FLUSH_PERIOD_SECS {
            return Err(Box::new(BuildError::FlushPeriodTooShort {
                min: MIN_FLUSH_PERIOD_SECS,
            }));
        }

        validate_quantiles(&self.quantiles)?;

        let sink = PrometheusExporter::new(self.clone(), cx.acker());
        let healthcheck = future::ok(()).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
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
    metrics: Arc<RwLock<IndexMap<MetricRef, (Metric, MetricMetadata)>>>,
    acker: Acker,
}

/// Expiration metadata for a metric.
#[derive(Clone, Copy, Debug)]
struct MetricMetadata {
    expiration_window: Duration,
    expires_at: Instant,
}

impl MetricMetadata {
    pub fn new(expiration_window: Duration) -> Self {
        Self {
            expiration_window,
            expires_at: Instant::now() + expiration_window,
        }
    }

    /// Resets the expiration deadline.
    pub fn refresh(&mut self) {
        self.expires_at = Instant::now() + self.expiration_window;
    }

    /// Whether or not the referenced metric has expired yet.
    pub fn has_expired(&self, now: Instant) -> bool {
        self.expires_at >= now
    }
}

// Composite identifier that uniquely represents a metric.
//
// Instead of simply working off of the name (series) alone, we include the metric kind as well as
// the type (counter, gauge, etc) and any subtype information like histogram buckets.
//
// Specifically, though, we do _not_ include the actual metric value.  This type is used
// specifically to look up the entry in a map for a metric in the sense of "get the metric whose
// name is X and type is Y and has these tags".
#[derive(Clone, Debug)]
struct MetricRef {
    series: MetricSeries,
    kind: MetricKind,
    value: Discriminant<MetricValue>,
    bounds: Option<Vec<f64>>,
}

impl MetricRef {
    /// Creates a `MetricRef` based on the given `Metric`.
    pub fn from_metric(metric: &Metric) -> Self {
        // Either the buckets for an aggregated histogram, or the quantiles for an aggregated summary.
        let bounds = match metric.value() {
            MetricValue::AggregatedHistogram { buckets, .. } => {
                Some(buckets.iter().map(|b| b.upper_limit).collect())
            }
            MetricValue::AggregatedSummary { quantiles, .. } => {
                Some(quantiles.iter().map(|q| q.quantile).collect())
            }
            _ => None,
        };

        Self {
            series: metric.series().clone(),
            kind: metric.kind(),
            value: discriminant(metric.value()),
            bounds,
        }
    }
}

impl PartialEq for MetricRef {
    fn eq(&self, other: &Self) -> bool {
        self.series == other.series
            && self.kind == other.kind
            && self.value == other.value
            && self.bounds == other.bounds
    }
}

impl Eq for MetricRef {}

impl Hash for MetricRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.series.hash(state);
        self.kind.hash(state);
        self.value.hash(state);
        if let Some(bounds) = &self.bounds {
            for bound in bounds {
                bound.to_bits().hash(state);
            }
        }
    }
}

struct PrometheusExporterMetricNormalizer {
    distributions_as_summaries: bool,
    buckets: Vec<f64>,
}

impl MetricNormalize for PrometheusExporterMetricNormalizer {
    fn apply_state(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        let new_metric = match metric.value() {
            MetricValue::Distribution { .. } => {
                // Convert the distribution as-is, and then let the normalizer absolute-ify it.
                let (series, data, metadata) = metric.into_parts();
                let (ts, kind, value) = data.into_parts();

                let new_value = if self.distributions_as_summaries {
                    // We use a sketch when in summary mode because they're actually able to be
                    // merged and provide correct output, unlike the aggregated summaries that
                    // we handle from _sources_ like Prometheus.  The collector code itself
                    // will render sketches as aggregated summaries, so we have continuity there.
                    value
                        .distribution_to_sketch()
                        .expect("value should be distribution already")
                } else {
                    value
                        .distribution_to_agg_histogram(&self.buckets)
                        .expect("value should be distribution already")
                };

                let data = MetricData::from_parts(ts, kind, new_value);
                Metric::from_parts(series, data, metadata)
            }
            _ => metric,
        };

        state.make_absolute(new_metric)
    }
}

fn handle(
    req: Request<Body>,
    default_namespace: Option<&str>,
    buckets: &[f64],
    quantiles: &[f64],
    metrics: &IndexMap<MetricRef, (Metric, MetricMetadata)>,
) -> Response<Body> {
    let mut response = Response::new(Body::empty());

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            let mut collector = StringCollector::new();

            for (_, (metric, _)) in metrics {
                collector.encode_metric(default_namespace, buckets, quantiles, metric);
            }

            *response.body_mut() = collector.finish().into();

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
            metrics: Arc::new(RwLock::new(IndexMap::new())),
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

        let new_service = make_service_fn(move |_| {
            let metrics = Arc::clone(&metrics);
            let default_namespace = default_namespace.clone();
            let buckets = buckets.clone();
            let quantiles = quantiles.clone();

            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let metrics = metrics.read().unwrap();

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
                            &metrics,
                        )
                    });

                    emit!(&PrometheusServerRequestComplete {
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
            #[allow(clippy::print_stderr)]
            let tls = MaybeTlsSettings::from_config(&tls, true)
                .map_err(|error| eprintln!("Server TLS error: {}", error))?;
            #[allow(clippy::print_stderr)]
            let listener = tls
                .bind(&address)
                .await
                .map_err(|error| eprintln!("Server bind error: {}", error))?;

            #[allow(clippy::print_stderr)]
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
impl StreamSink<Event> for PrometheusExporter {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.start_server_if_needed().await;

        let mut last_flush = Instant::now();
        let flush_period = self.config.flush_period_secs;
        let mut normalizer = MetricNormalizer::from(PrometheusExporterMetricNormalizer {
            distributions_as_summaries: self.config.distributions_as_summaries,
            buckets: self.config.buckets.clone(),
        });

        while let Some(event) = input.next().await {
            // If we've exceed our flush interval, go through all of the metrics we're currently
            // tracking and remove any which have exceeded the the flush interval in terms of not
            // having been updated within that long of a time.
            //
            // TODO: Can we be smarter about this? As is, we might wait up to 2x the flush period to
            // remove an expired metric depending on how things line up.  It'd be cool to _check_
            // for expired metrics more often, but we also don't want to check _way_ too often, like
            // every second, since then we're constantly iterating through every metric, etc etc.
            if last_flush.elapsed() > self.config.flush_period_secs {
                last_flush = Instant::now();

                let mut metrics = self.metrics.write().unwrap();

                let metrics_to_expire = metrics
                    .iter()
                    .filter(|(_, (_, metadata))| !metadata.has_expired(last_flush))
                    .map(|(metric_ref, _)| metric_ref.clone())
                    .collect::<Vec<_>>();

                for metric_ref in metrics_to_expire {
                    metrics.remove(&metric_ref);
                    normalizer.get_state_mut().remove(&metric_ref.series);
                }
            }

            // Now process the metric we got.
            let metric = event.into_metric();
            if let Some(normalized) = normalizer.apply(metric) {
                // We have a normalized metric, in absolute form.  If we're already aware of this
                // metric, update its expiration deadline, otherwise, start tracking it.
                let mut metrics = self.metrics.write().unwrap();

                let metric_ref = MetricRef::from_metric(&normalized);
                match metrics.get_mut(&metric_ref) {
                    Some((data, metadata)) => {
                        *data = normalized;
                        metadata.refresh();
                    }
                    None => {
                        metrics.insert(metric_ref, (normalized, MetricMetadata::new(flush_period)));
                    }
                }
            }

            self.acker.ack(1);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use indoc::indoc;
    use pretty_assertions::assert_eq;
    use tokio::{sync::mpsc, time};
    use tokio_stream::wrappers::UnboundedReceiverStream;
    use vector_core::{event::StatisticKind, samples};

    use super::*;
    use crate::{
        config::ProxyConfig,
        event::metric::{Metric, MetricValue},
        http::HttpClient,
        sinks::prometheus::{distribution_to_agg_histogram, distribution_to_ddsketch},
        test_util::{next_addr, random_string, trace_init},
        tls::MaybeTlsSettings,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PrometheusExporterConfig>();
    }

    #[tokio::test]
    async fn prometheus_notls() {
        export_and_fetch_simple(None).await;
    }

    #[tokio::test]
    async fn prometheus_tls() {
        let mut tls_config = TlsConfig::test_config();
        tls_config.options.verify_hostname = Some(false);
        export_and_fetch_simple(Some(tls_config)).await;
    }

    #[tokio::test]
    async fn updates_timestamps() {
        let timestamp1 = Utc::now();
        let (name, event1) = create_metric_gauge(None, 123.4);
        let event1 = Event::from(event1.into_metric().with_timestamp(Some(timestamp1)));
        let (_, event2) = create_metric_gauge(Some(name.clone()), 12.0);
        let timestamp2 = timestamp1 + Duration::seconds(1);
        let event2 = Event::from(event2.into_metric().with_timestamp(Some(timestamp2)));
        let events = vec![event1, event2];

        let body = export_and_fetch(None, events).await;
        let timestamp = timestamp2.timestamp_millis();
        assert_eq!(
            body,
            format!(
                indoc! {r#"
                    # HELP {name} {name}
                    # TYPE {name} gauge
                    {name}{{some_tag="some_value"}} 135.4 {timestamp}
                "#},
                name = name,
                timestamp = timestamp
            )
        );
    }

    async fn export_and_fetch(tls_config: Option<TlsConfig>, events: Vec<Event>) -> String {
        trace_init();

        let client_settings = MaybeTlsSettings::from_config(&tls_config, false).unwrap();
        let proto = client_settings.http_protocol_name();

        let address = next_addr();
        let config = PrometheusExporterConfig {
            address,
            tls: tls_config,
            ..Default::default()
        };
        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(sink.run(Box::pin(UnboundedReceiverStream::new(rx))));

        for event in events {
            tx.send(event.into()).expect("Failed to send event.");
        }

        time::sleep(time::Duration::from_millis(100)).await;

        let request = Request::get(format!("{}://{}/metrics", proto, address))
            .body(Body::empty())
            .expect("Error creating request.");
        let proxy = ProxyConfig::default();
        let result = HttpClient::new(client_settings, &proxy)
            .unwrap()
            .send(request)
            .await
            .expect("Could not fetch query");

        assert!(result.status().is_success());

        let body = result.into_body();
        let bytes = hyper::body::to_bytes(body)
            .await
            .expect("Reading body failed");
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    async fn export_and_fetch_simple(tls_config: Option<TlsConfig>) {
        let (name1, event1) = create_metric_gauge(None, 123.4);
        let (name2, event2) = tests::create_metric_set(None, vec!["0", "1", "2"]);
        let events = vec![event1, event2];

        let body = export_and_fetch(tls_config, events).await;

        assert!(body.contains(&format!(
            indoc! {r#"
               # HELP {name} {name}
               # TYPE {name} gauge
               {name}{{some_tag="some_value"}} 123.4
            "#},
            name = name1
        )));
        assert!(body.contains(&format!(
            indoc! {r#"
               # HELP {name} {name}
               # TYPE {name} gauge
               {name}{{some_tag="some_value"}} 3
            "#},
            name = name2
        )));
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
        let event = Metric::new(name.clone(), MetricKind::Incremental, value)
            .with_tags(Some(
                vec![("some_tag".to_owned(), "some_value".to_owned())]
                    .into_iter()
                    .collect(),
            ))
            .into();
        (name, event)
    }

    #[tokio::test]
    async fn sink_absolute() {
        let config = PrometheusExporterConfig {
            address: next_addr(), // Not actually bound, just needed to fill config
            tls: None,
            ..Default::default()
        };
        let cx = SinkContext::new_test();

        let sink = Box::new(PrometheusExporter::new(config, cx.acker()));

        let m1 = Metric::new(
            "absolute",
            MetricKind::Absolute,
            MetricValue::Counter { value: 32. },
        )
        .with_tags(Some(
            vec![("tag1".to_owned(), "value1".to_owned())]
                .into_iter()
                .collect(),
        ));

        let m2 = m1.clone().with_tags(Some(
            vec![("tag1".to_owned(), "value2".to_owned())]
                .into_iter()
                .collect(),
        ));

        let metrics = vec![
            Event::Metric(m1.clone().with_value(MetricValue::Counter { value: 32. })),
            Event::Metric(m2.clone().with_value(MetricValue::Counter { value: 33. })),
            Event::Metric(m1.clone().with_value(MetricValue::Counter { value: 40. })),
        ];

        let metrics_handle = Arc::clone(&sink.metrics);

        sink.run(Box::pin(futures::stream::iter(metrics)))
            .await
            .unwrap();

        let metrics_after = metrics_handle.read().unwrap();

        let expected_m1 = metrics_after
            .get(&MetricRef::from_metric(&m1))
            .expect("m1 should exist");
        let expected_m1_value = MetricValue::Counter { value: 40. };
        assert_eq!(expected_m1.0.value(), &expected_m1_value);

        let expected_m2 = metrics_after
            .get(&MetricRef::from_metric(&m2))
            .expect("m2 should exist");
        let expected_m2_value = MetricValue::Counter { value: 33. };
        assert_eq!(expected_m2.0.value(), &expected_m2_value);
    }

    #[tokio::test]
    async fn sink_distributions_as_histograms() {
        // When we get summary distributions, unless we've been configured to actually emit
        // summaries for distributions, we just forcefully turn them into histograms.  This is
        // simpler and uses less memory, as aggregated histograms are better supported by Prometheus
        // since they can actually be aggregated anywhere in the pipeline -- so long as the buckets
        // are the same -- without loss of accuracy.

        // This expects that the default for the sink is to render distributions as aggregated histograms.
        let config = PrometheusExporterConfig {
            address: next_addr(), // Not actually bound, just needed to fill config
            tls: None,
            ..Default::default()
        };
        let buckets = config.buckets.clone();
        let cx = SinkContext::new_test();

        let sink = Box::new(PrometheusExporter::new(config, cx.acker()));

        // Define a series of incremental distribution updates.
        let base_summary_metric = Metric::new(
            "distrib_summary",
            MetricKind::Incremental,
            MetricValue::Distribution {
                statistic: StatisticKind::Summary,
                samples: samples!(1.0 => 1, 3.0 => 2),
            },
        );

        let base_histogram_metric = Metric::new(
            "distrib_histo",
            MetricKind::Incremental,
            MetricValue::Distribution {
                statistic: StatisticKind::Histogram,
                samples: samples!(7.0 => 1, 9.0 => 2),
            },
        );

        let metrics = vec![
            base_summary_metric.clone(),
            base_summary_metric
                .clone()
                .with_value(MetricValue::Distribution {
                    statistic: StatisticKind::Summary,
                    samples: samples!(1.0 => 2, 2.9 => 1),
                }),
            base_summary_metric
                .clone()
                .with_value(MetricValue::Distribution {
                    statistic: StatisticKind::Summary,
                    samples: samples!(1.0 => 4, 3.2 => 1),
                }),
            base_histogram_metric.clone(),
            base_histogram_metric
                .clone()
                .with_value(MetricValue::Distribution {
                    statistic: StatisticKind::Histogram,
                    samples: samples!(7.0 => 2, 9.9 => 1),
                }),
            base_histogram_metric
                .clone()
                .with_value(MetricValue::Distribution {
                    statistic: StatisticKind::Histogram,
                    samples: samples!(7.0 => 4, 10.2 => 1),
                }),
        ];

        // Figure out what the merged distributions should add up to.
        let mut merged_summary = base_summary_metric.clone();
        assert!(merged_summary.update(&metrics[1]));
        assert!(merged_summary.update(&metrics[2]));
        let expected_summary = distribution_to_agg_histogram(merged_summary, &buckets)
            .expect("input summary metric should have been distribution")
            .into_absolute();

        let mut merged_histogram = base_histogram_metric.clone();
        assert!(merged_histogram.update(&metrics[4]));
        assert!(merged_histogram.update(&metrics[5]));
        let expected_histogram = distribution_to_agg_histogram(merged_histogram, &buckets)
            .expect("input histogram metric should have been distribution")
            .into_absolute();

        // TODO: make a new metric based on merged_distrib_histogram, with expected_histogram_value,
        // so that the discriminant matches and our lookup in the indexmap can actually find it

        // Now run the events through the sink and see what ends up in the internal metric map.
        let metrics_handle = Arc::clone(&sink.metrics);

        let events = metrics
            .iter()
            .cloned()
            .map(Event::Metric)
            .collect::<Vec<_>>();
        sink.run(Box::pin(futures::stream::iter(events)))
            .await
            .unwrap();

        let metrics_after = metrics_handle.read().unwrap();

        // Both metrics should be present, and both should be aggregated histograms.
        assert_eq!(metrics_after.len(), 2);

        let actual_summary = metrics_after
            .get(&MetricRef::from_metric(&expected_summary))
            .expect("summary metric should exist");
        assert_eq!(actual_summary.0.value(), expected_summary.value());

        let actual_histogram = metrics_after
            .get(&MetricRef::from_metric(&expected_histogram))
            .expect("histogram metric should exist");
        assert_eq!(actual_histogram.0.value(), expected_histogram.value());
    }

    #[tokio::test]
    async fn sink_distributions_as_summaries() {
        // When we get summary distributions, unless we've been configured to actually emit
        // summaries for distributions, we just forcefully turn them into histograms.  This is
        // simpler and uses less memory, as aggregated histograms are better supported by Prometheus
        // since they can actually be aggregated anywhere in the pipeline -- so long as the buckets
        // are the same -- without loss of accuracy.

        // This assumes that when we turn on `distributions_as_summaries`, we'll get aggregated
        // summaries from distributions.  This is technically true, but the way this test works is
        // that we check the internal metric data, which, when in this mode, will actually be a
        // sketch (so that we can merge without loss of accuracy).
        //
        // The render code is actually what will end up rrendering those sketches as aggregated
        // summaries in the scrape output.
        let config = PrometheusExporterConfig {
            address: next_addr(), // Not actually bound, just needed to fill config
            tls: None,
            distributions_as_summaries: true,
            ..Default::default()
        };
        let cx = SinkContext::new_test();

        let sink = Box::new(PrometheusExporter::new(config, cx.acker()));

        // Define a series of incremental distribution updates.
        let base_summary_metric = Metric::new(
            "distrib_summary",
            MetricKind::Incremental,
            MetricValue::Distribution {
                statistic: StatisticKind::Summary,
                samples: samples!(1.0 => 1, 3.0 => 2),
            },
        );

        let base_histogram_metric = Metric::new(
            "distrib_histo",
            MetricKind::Incremental,
            MetricValue::Distribution {
                statistic: StatisticKind::Histogram,
                samples: samples!(7.0 => 1, 9.0 => 2),
            },
        );

        let metrics = vec![
            base_summary_metric.clone(),
            base_summary_metric
                .clone()
                .with_value(MetricValue::Distribution {
                    statistic: StatisticKind::Summary,
                    samples: samples!(1.0 => 2, 2.9 => 1),
                }),
            base_summary_metric
                .clone()
                .with_value(MetricValue::Distribution {
                    statistic: StatisticKind::Summary,
                    samples: samples!(1.0 => 4, 3.2 => 1),
                }),
            base_histogram_metric.clone(),
            base_histogram_metric
                .clone()
                .with_value(MetricValue::Distribution {
                    statistic: StatisticKind::Histogram,
                    samples: samples!(7.0 => 2, 9.9 => 1),
                }),
            base_histogram_metric
                .clone()
                .with_value(MetricValue::Distribution {
                    statistic: StatisticKind::Histogram,
                    samples: samples!(7.0 => 4, 10.2 => 1),
                }),
        ];

        // Figure out what the merged distributions should add up to.
        let mut merged_summary = base_summary_metric.clone();
        assert!(merged_summary.update(&metrics[1]));
        assert!(merged_summary.update(&metrics[2]));
        let expected_summary = distribution_to_ddsketch(merged_summary)
            .expect("input summary metric should have been distribution")
            .into_absolute();

        let mut merged_histogram = base_histogram_metric.clone();
        assert!(merged_histogram.update(&metrics[4]));
        assert!(merged_histogram.update(&metrics[5]));
        let expected_histogram = distribution_to_ddsketch(merged_histogram)
            .expect("input histogram metric should have been distribution")
            .into_absolute();

        // Now run the events through the sink and see what ends up in the internal metric map.
        let metrics_handle = Arc::clone(&sink.metrics);

        let events = metrics
            .iter()
            .cloned()
            .map(Event::Metric)
            .collect::<Vec<_>>();
        sink.run(Box::pin(futures::stream::iter(events)))
            .await
            .unwrap();

        let metrics_after = metrics_handle.read().unwrap();

        // Both metrics should be present, and both should be aggregated histograms.
        assert_eq!(metrics_after.len(), 2);

        let actual_summary = metrics_after
            .get(&MetricRef::from_metric(&expected_summary))
            .expect("summary metric should exist");
        assert_eq!(actual_summary.0.value(), expected_summary.value());

        let actual_histogram = metrics_after
            .get(&MetricRef::from_metric(&expected_histogram))
            .expect("histogram metric should exist");
        assert_eq!(actual_histogram.0.value(), expected_histogram.value());
    }
}

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests {
    #![allow(clippy::print_stdout)] // tests
    #![allow(clippy::print_stderr)] // tests
    #![allow(clippy::dbg_macro)] // tests

    use chrono::Utc;
    use serde_json::Value;
    use tokio::{sync::mpsc, time};
    use tokio_stream::wrappers::UnboundedReceiverStream;

    use super::*;
    use crate::{config::ProxyConfig, http::HttpClient, test_util::trace_init};

    fn sink_exporter_address() -> String {
        std::env::var("SINK_EXPORTER_ADDRESS").unwrap_or_else(|_| "127.0.0.1:9101".into())
    }

    fn prometheus_address() -> String {
        std::env::var("PROMETHEUS_ADDRESS").unwrap_or_else(|_| "localhost:9090".into())
    }

    async fn fetch_exporter_body() -> String {
        let url = format!("http://{}/metrics", sink_exporter_address());
        let request = Request::get(url)
            .body(Body::empty())
            .expect("Error creating request.");
        let proxy = ProxyConfig::default();
        let result = HttpClient::new(None, &proxy)
            .unwrap()
            .send(request)
            .await
            .expect("Could not send request");
        let result = hyper::body::to_bytes(result.into_body())
            .await
            .expect("Error fetching body");
        String::from_utf8_lossy(&result).to_string()
    }

    async fn prometheus_query(query: &str) -> Value {
        let url = format!(
            "http://{}/api/v1/query?query={}",
            prometheus_address(),
            query
        );
        let request = Request::post(url)
            .body(Body::empty())
            .expect("Error creating request.");
        let proxy = ProxyConfig::default();
        let result = HttpClient::new(None, &proxy)
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

    #[tokio::test]
    async fn prometheus_metrics() {
        trace_init();

        prometheus_scrapes_metrics().await;
        time::sleep(time::Duration::from_millis(500)).await;
        reset_on_flush_period().await;
        expire_on_flush_period().await;
    }

    async fn prometheus_scrapes_metrics() {
        let start = Utc::now().timestamp();

        let config = PrometheusExporterConfig {
            address: sink_exporter_address().parse().unwrap(),
            flush_period_secs: Duration::from_secs(2),
            ..Default::default()
        };
        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(sink.run(Box::pin(UnboundedReceiverStream::new(rx))));

        let (name, event) = tests::create_metric_gauge(None, 123.4);
        tx.send(event.into()).expect("Failed to send.");

        // Wait a bit for the prometheus server to scrape the metrics
        time::sleep(time::Duration::from_secs(2)).await;

        // Now try to download them from prometheus
        let result = prometheus_query(&name).await;

        let data = &result["data"]["result"][0];
        assert_eq!(data["metric"]["__name__"], Value::String(name));
        assert_eq!(
            data["metric"]["instance"],
            Value::String(sink_exporter_address())
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
            address: sink_exporter_address().parse().unwrap(),
            flush_period_secs: Duration::from_secs(3),
            ..Default::default()
        };
        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(sink.run(Box::pin(UnboundedReceiverStream::new(rx))));

        // Create two sets with different names but the same size.
        let (name1, event) = tests::create_metric_set(None, vec!["0", "1", "2"]);
        tx.send(event.into()).expect("Failed to send.");
        let (name2, event) = tests::create_metric_set(None, vec!["3", "4", "5"]);
        tx.send(event.into()).expect("Failed to send.");

        // Wait for the Prometheus server to scrape them, and then query it to ensure both metrics
        // have their correct set size value.
        time::sleep(time::Duration::from_secs(2)).await;

        // Now query Prometheus to make sure we see them there.
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

        // Wait a few more seconds to ensure that the two original sets have logically expired.
        // We'll update `name2` but not `name1`, which should lead to both being expired, but
        // `name2` being recreated with two values only, while `name1` is entirely gone.
        time::sleep(time::Duration::from_secs(3)).await;

        let (name2, event) = tests::create_metric_set(Some(name2), vec!["8", "9"]);
        tx.send(event.into()).expect("Failed to send.");

        // Again, wait for the Prometheus server to scrape the metrics, and then query it again.
        time::sleep(time::Duration::from_secs(2)).await;
        let result = prometheus_query(&name1).await;
        assert_eq!(result["data"]["result"][0]["value"][1], Value::Null);
        let result = prometheus_query(&name2).await;
        assert_eq!(
            result["data"]["result"][0]["value"][1],
            Value::String("2".into())
        );
    }

    async fn expire_on_flush_period() {
        let config = PrometheusExporterConfig {
            address: sink_exporter_address().parse().unwrap(),
            flush_period_secs: Duration::from_secs(3),
            ..Default::default()
        };
        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(sink.run(Box::pin(UnboundedReceiverStream::new(rx))));

        // metrics that will not be updated for a full flush period and therefore should expire
        let (name1, event) = tests::create_metric_set(None, vec!["42"]);
        tx.send(event.into()).expect("Failed to send.");
        let (name2, event) = tests::create_metric_gauge(None, 100.0);
        tx.send(event.into()).expect("Failed to send.");

        // Wait a bit for the sink to process the events
        time::sleep(time::Duration::from_secs(1)).await;

        // Exporter should present both metrics at first
        let body = fetch_exporter_body().await;
        assert!(body.contains(&name1));
        assert!(body.contains(&name2));

        // Wait long enough to put us past flush_period_secs for the metric that wasn't updated
        for _ in 0..7 {
            // Update the first metric, ensuring it doesn't expire
            let (_, event) = tests::create_metric_set(Some(name1.clone()), vec!["43"]);
            tx.send(event.into()).expect("Failed to send.");

            // Wait a bit for time to pass
            time::sleep(time::Duration::from_secs(1)).await;
        }

        // Exporter should present only the one that got updated
        let body = fetch_exporter_body().await;
        assert!(body.contains(&name1));
        assert!(!body.contains(&name2));
    }
}

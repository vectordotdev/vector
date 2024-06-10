use std::{
    convert::Infallible,
    hash::Hash,
    mem::{discriminant, Discriminant},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use base64::prelude::{Engine as _, BASE64_STANDARD};
use futures::{future, stream::BoxStream, FutureExt, StreamExt};
use hyper::{
    body::HttpBody,
    header::HeaderValue,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use indexmap::{map::Entry, IndexMap};
use serde_with::serde_as;
use snafu::Snafu;
use stream_cancel::{Trigger, Tripwire};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tracing::{Instrument, Span};
use vector_lib::configurable::configurable_component;
use vector_lib::{
    internal_event::{
        ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle as _, Output, Protocol,
        Registered,
    },
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
};

use super::collector::{MetricCollector, StringCollector};
use crate::{
    config::{AcknowledgementsConfig, GenerateConfig, Input, Resource, SinkConfig, SinkContext},
    event::{
        metric::{Metric, MetricData, MetricKind, MetricSeries, MetricValue},
        Event, EventStatus, Finalizable,
    },
    http::{build_http_trace_layer, Auth},
    internal_events::PrometheusNormalizationError,
    sinks::{
        util::{statistic::validate_quantiles, StreamSink},
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

const MIN_FLUSH_PERIOD_SECS: u64 = 1;

const LOCK_FAILED: &str = "Prometheus exporter data lock is poisoned";

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Flush period for sets must be greater or equal to {} secs", min))]
    FlushPeriodTooShort { min: u64 },
}

/// Configuration for the `prometheus_exporter` sink.
#[serde_as]
#[configurable_component(sink(
    "prometheus_exporter",
    "Expose metric events on a Prometheus compatible endpoint."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PrometheusExporterConfig {
    /// The default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with an underscore (`_`).
    ///
    /// It should follow the Prometheus [naming conventions][prom_naming_docs].
    ///
    /// [prom_naming_docs]: https://prometheus.io/docs/practices/naming/#metric-names
    #[serde(alias = "namespace")]
    #[configurable(metadata(docs::advanced))]
    pub default_namespace: Option<String>,

    /// The address to expose for scraping.
    ///
    /// The metrics are exposed at the typical Prometheus exporter path, `/metrics`.
    #[serde(default = "default_address")]
    #[configurable(metadata(docs::examples = "192.160.0.10:9598"))]
    pub address: SocketAddr,

    #[configurable(derived)]
    pub auth: Option<Auth>,

    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    /// Default buckets to use for aggregating [distribution][dist_metric_docs] metrics into histograms.
    ///
    /// [dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
    #[serde(default = "super::default_histogram_buckets")]
    #[configurable(metadata(docs::advanced))]
    pub buckets: Vec<f64>,

    /// Quantiles to use for aggregating [distribution][dist_metric_docs] metrics into a summary.
    ///
    /// [dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
    #[serde(default = "super::default_summary_quantiles")]
    #[configurable(metadata(docs::advanced))]
    pub quantiles: Vec<f64>,

    /// Whether or not to render [distributions][dist_metric_docs] as an [aggregated histogram][prom_agg_hist_docs] or  [aggregated summary][prom_agg_summ_docs].
    ///
    /// While distributions as a lossless way to represent a set of samples for a
    /// metric is supported, Prometheus clients (the application being scraped, which is this sink) must
    /// aggregate locally into either an aggregated histogram or aggregated summary.
    ///
    /// [dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
    /// [prom_agg_hist_docs]: https://prometheus.io/docs/concepts/metric_types/#histogram
    /// [prom_agg_summ_docs]: https://prometheus.io/docs/concepts/metric_types/#summary
    #[serde(default = "default_distributions_as_summaries")]
    #[configurable(metadata(docs::advanced))]
    pub distributions_as_summaries: bool,

    /// The interval, in seconds, on which metrics are flushed.
    ///
    /// On the flush interval, if a metric has not been seen since the last flush interval, it is
    /// considered expired and is removed.
    ///
    /// Be sure to configure this value higher than your client’s scrape interval.
    #[serde(default = "default_flush_period_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::human_name = "Flush Interval"))]
    pub flush_period_secs: Duration,

    /// Suppresses timestamps on the Prometheus output.
    ///
    /// This can sometimes be useful when the source of metrics leads to their timestamps being too
    /// far in the past for Prometheus to allow them, such as when aggregating metrics over long
    /// time periods, or when replaying old metrics from a disk buffer.
    #[serde(default)]
    #[configurable(metadata(docs::advanced))]
    pub suppress_timestamp: bool,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl Default for PrometheusExporterConfig {
    fn default() -> Self {
        Self {
            default_namespace: None,
            address: default_address(),
            auth: None,
            tls: None,
            buckets: super::default_histogram_buckets(),
            quantiles: super::default_summary_quantiles(),
            distributions_as_summaries: default_distributions_as_summaries(),
            flush_period_secs: default_flush_period_secs(),
            suppress_timestamp: default_suppress_timestamp(),
            acknowledgements: Default::default(),
        }
    }
}

const fn default_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 9598)
}

const fn default_distributions_as_summaries() -> bool {
    false
}

const fn default_flush_period_secs() -> Duration {
    Duration::from_secs(60)
}

const fn default_suppress_timestamp() -> bool {
    false
}

impl GenerateConfig for PrometheusExporterConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_exporter")]
impl SinkConfig for PrometheusExporterConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        if self.flush_period_secs.as_secs() < MIN_FLUSH_PERIOD_SECS {
            return Err(Box::new(BuildError::FlushPeriodTooShort {
                min: MIN_FLUSH_PERIOD_SECS,
            }));
        }

        validate_quantiles(&self.quantiles)?;

        let sink = PrometheusExporter::new(self.clone());
        let healthcheck = future::ok(()).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

struct PrometheusExporter {
    server_shutdown_trigger: Option<Trigger>,
    config: PrometheusExporterConfig,
    metrics: Arc<RwLock<IndexMap<MetricRef, (Metric, MetricMetadata)>>>,
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
        now >= self.expires_at
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
            value: discriminant(metric.value()),
            bounds,
        }
    }
}

impl PartialEq for MetricRef {
    fn eq(&self, other: &Self) -> bool {
        self.series == other.series && self.value == other.value && self.bounds == other.bounds
    }
}

impl Eq for MetricRef {}

impl Hash for MetricRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.series.hash(state);
        self.value.hash(state);
        if let Some(bounds) = &self.bounds {
            for bound in bounds {
                bound.to_bits().hash(state);
            }
        }
    }
}

fn authorized<T: HttpBody>(req: &Request<T>, auth: &Option<Auth>) -> bool {
    if let Some(auth) = auth {
        let headers = req.headers();
        if let Some(auth_header) = headers.get(hyper::header::AUTHORIZATION) {
            let encoded_credentials = match auth {
                Auth::Basic { user, password } => HeaderValue::from_str(
                    format!(
                        "Basic {}",
                        BASE64_STANDARD.encode(format!("{}:{}", user, password.inner()))
                    )
                    .as_str(),
                ),
                Auth::Bearer { token } => {
                    HeaderValue::from_str(format!("Bearer {}", token.inner()).as_str())
                }
            };

            if let Ok(encoded_credentials) = encoded_credentials {
                if auth_header == encoded_credentials {
                    return true;
                }
            }
        }
    } else {
        return true;
    }

    false
}

#[derive(Clone)]
struct Handler {
    auth: Option<Auth>,
    default_namespace: Option<String>,
    buckets: Box<[f64]>,
    quantiles: Box<[f64]>,
    bytes_sent: Registered<BytesSent>,
    events_sent: Registered<EventsSent>,
}

impl Handler {
    fn handle<T: HttpBody>(
        &self,
        req: Request<T>,
        metrics: &RwLock<IndexMap<MetricRef, (Metric, MetricMetadata)>>,
    ) -> Response<Body> {
        let mut response = Response::new(Body::empty());

        match (authorized(&req, &self.auth), req.method(), req.uri().path()) {
            (false, _, _) => {
                *response.status_mut() = StatusCode::UNAUTHORIZED;
                response.headers_mut().insert(
                    http::header::WWW_AUTHENTICATE,
                    HeaderValue::from_static("Basic, Bearer"),
                );
            }

            (true, &Method::GET, "/metrics") => {
                let metrics = metrics.read().expect(LOCK_FAILED);

                let count = metrics.len();
                let byte_size = metrics
                    .iter()
                    .map(|(_, (metric, _))| metric.estimated_json_encoded_size_of())
                    .sum();

                let mut collector = StringCollector::new();

                for (_, (metric, _)) in metrics.iter() {
                    collector.encode_metric(
                        self.default_namespace.as_deref(),
                        &self.buckets,
                        &self.quantiles,
                        metric,
                    );
                }

                drop(metrics);

                let body = collector.finish();
                let body_size = body.size_of();

                *response.body_mut() = body.into();

                response.headers_mut().insert(
                    "Content-Type",
                    HeaderValue::from_static("text/plain; version=0.0.4"),
                );

                self.events_sent.emit(CountByteSize(count, byte_size));
                self.bytes_sent.emit(ByteSize(body_size));
            }

            (true, _, _) => {
                *response.status_mut() = StatusCode::NOT_FOUND;
            }
        }

        response
    }
}

impl PrometheusExporter {
    fn new(config: PrometheusExporterConfig) -> Self {
        Self {
            server_shutdown_trigger: None,
            config,
            metrics: Arc::new(RwLock::new(IndexMap::new())),
        }
    }

    async fn start_server_if_needed(&mut self) -> crate::Result<()> {
        if self.server_shutdown_trigger.is_some() {
            return Ok(());
        }

        let handler = Handler {
            bytes_sent: register!(BytesSent::from(Protocol::HTTP)),
            events_sent: register!(EventsSent::from(Output(None))),
            default_namespace: self.config.default_namespace.clone(),
            buckets: self.config.buckets.clone().into(),
            quantiles: self.config.quantiles.clone().into(),
            auth: self.config.auth.clone(),
        };

        let span = Span::current();
        let metrics = Arc::clone(&self.metrics);

        let new_service = make_service_fn(move |_| {
            let span = Span::current();
            let metrics = Arc::clone(&metrics);
            let handler = handler.clone();

            let inner = service_fn(move |req| {
                let response = handler.handle(req, &metrics);

                future::ok::<_, Infallible>(response)
            });

            let service = ServiceBuilder::new()
                .layer(build_http_trace_layer(span.clone()))
                .layer(CompressionLayer::new())
                .service(inner);

            async move { Ok::<_, Infallible>(service) }
        });

        let (trigger, tripwire) = Tripwire::new();

        let tls = self.config.tls.clone();
        let address = self.config.address;

        let tls = MaybeTlsSettings::from_config(&tls, true)?;
        let listener = tls.bind(&address).await?;

        tokio::spawn(async move {
            info!(message = "Building HTTP server.", address = %address);

            Server::builder(hyper::server::accept::from_stream(listener.accept_stream()))
                .serve(new_service)
                .with_graceful_shutdown(tripwire.then(crate::shutdown::tripwire_handler))
                .instrument(span)
                .await
                .map_err(|error| error!("Server error: {}.", error))?;

            Ok::<(), ()>(())
        });

        self.server_shutdown_trigger = Some(trigger);
        Ok(())
    }

    fn normalize(&mut self, metric: Metric) -> Option<Metric> {
        let new_metric = match metric.value() {
            MetricValue::Distribution { .. } => {
                // Convert the distribution as-is, and then absolute-ify it.
                let (series, data, metadata) = metric.into_parts();
                let (time, kind, value) = data.into_parts();

                let new_value = if self.config.distributions_as_summaries {
                    // We use a sketch when in summary mode because they're actually able to be
                    // merged and provide correct output, unlike the aggregated summaries that
                    // we handle from _sources_ like Prometheus.  The collector code itself
                    // will render sketches as aggregated summaries, so we have continuity there.
                    value
                        .distribution_to_sketch()
                        .expect("value should be distribution already")
                } else {
                    value
                        .distribution_to_agg_histogram(&self.config.buckets)
                        .expect("value should be distribution already")
                };

                let data = MetricData::from_parts(time, kind, new_value);
                Metric::from_parts(series, data, metadata)
            }
            _ => metric,
        };

        match new_metric.kind() {
            MetricKind::Absolute => Some(new_metric),
            MetricKind::Incremental => {
                let metrics = self.metrics.read().expect(LOCK_FAILED);
                let metric_ref = MetricRef::from_metric(&new_metric);

                if let Some(existing) = metrics.get(&metric_ref) {
                    let mut current = existing.0.value().clone();
                    if current.add(new_metric.value()) {
                        // If we were able to add to the existing value (i.e. they were compatible),
                        // return the result as an absolute metric.
                        return Some(new_metric.with_value(current).into_absolute());
                    }
                }

                // Otherwise, if we didn't have an existing value or we did and it was not
                // compatible with the new value, simply return the new value as absolute.
                Some(new_metric.into_absolute())
            }
        }
    }
}

#[async_trait]
impl StreamSink<Event> for PrometheusExporter {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.start_server_if_needed()
            .await
            .map_err(|error| error!("Failed to start Prometheus exporter: {}.", error))?;

        let mut last_flush = Instant::now();
        let flush_period = self.config.flush_period_secs;

        while let Some(event) = input.next().await {
            // If we've exceed our flush interval, go through all of the metrics we're currently
            // tracking and remove any which have exceeded the flush interval in terms of not
            // having been updated within that long of a time.
            //
            // TODO: Can we be smarter about this? As is, we might wait up to 2x the flush period to
            // remove an expired metric depending on how things line up.  It'd be cool to _check_
            // for expired metrics more often, but we also don't want to check _way_ too often, like
            // every second, since then we're constantly iterating through every metric, etc etc.
            if last_flush.elapsed() > self.config.flush_period_secs {
                last_flush = Instant::now();

                let mut metrics = self.metrics.write().expect(LOCK_FAILED);

                metrics.retain(|_metric_ref, (_, metadata)| !metadata.has_expired(last_flush));
            }

            // Now process the metric we got.
            let mut metric = event.into_metric();
            let finalizers = metric.take_finalizers();

            if let Some(normalized) = self.normalize(metric) {
                let normalized = if self.config.suppress_timestamp {
                    normalized.with_timestamp(None)
                } else {
                    normalized
                };

                // We have a normalized metric, in absolute form.  If we're already aware of this
                // metric, update its expiration deadline, otherwise, start tracking it.
                let mut metrics = self.metrics.write().expect(LOCK_FAILED);

                match metrics.entry(MetricRef::from_metric(&normalized)) {
                    Entry::Occupied(mut entry) => {
                        let (data, metadata) = entry.get_mut();
                        *data = normalized;
                        metadata.refresh();
                    }
                    Entry::Vacant(entry) => {
                        entry.insert((normalized, MetricMetadata::new(flush_period)));
                    }
                }
                finalizers.update_status(EventStatus::Delivered);
            } else {
                emit!(PrometheusNormalizationError {});
                finalizers.update_status(EventStatus::Errored);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use flate2::read::GzDecoder;
    use futures::stream;
    use indoc::indoc;
    use similar_asserts::assert_eq;
    use std::io::Read;
    use tokio::{sync::oneshot::error::TryRecvError, time};
    use vector_lib::{
        event::{MetricTags, StatisticKind},
        finalization::{BatchNotifier, BatchStatus},
        metric_tags, samples,
        sensitive_string::SensitiveString,
    };

    use super::*;
    use crate::{
        config::ProxyConfig,
        event::metric::{Metric, MetricValue},
        http::HttpClient,
        sinks::prometheus::{distribution_to_agg_histogram, distribution_to_ddsketch},
        test_util::{
            components::{run_and_assert_sink_compliance, SINK_TAGS},
            next_addr, random_string, trace_init,
        },
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
        let mut tls_config = TlsEnableableConfig::test_config();
        tls_config.options.verify_hostname = Some(false);
        export_and_fetch_simple(Some(tls_config)).await;
    }

    #[tokio::test]
    async fn prometheus_noauth() {
        let (name1, event1) = create_metric_gauge(None, 123.4);
        let (name2, event2) = tests::create_metric_set(None, vec!["0", "1", "2"]);
        let events = vec![event1, event2];

        let response_result = export_and_fetch_with_auth(None, None, events, false).await;

        assert!(response_result.is_ok());

        let body = response_result.expect("Cannot extract body from the response");

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

    #[tokio::test]
    async fn prometheus_successful_basic_auth() {
        let (name1, event1) = create_metric_gauge(None, 123.4);
        let (name2, event2) = tests::create_metric_set(None, vec!["0", "1", "2"]);
        let events = vec![event1, event2];

        let auth_config = Auth::Basic {
            user: "user".to_string(),
            password: SensitiveString::from("password".to_string()),
        };

        let response_result =
            export_and_fetch_with_auth(Some(auth_config.clone()), Some(auth_config), events, false)
                .await;

        assert!(response_result.is_ok());

        let body = response_result.expect("Cannot extract body from the response");

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

    #[tokio::test]
    async fn prometheus_successful_token_auth() {
        let (name1, event1) = create_metric_gauge(None, 123.4);
        let (name2, event2) = tests::create_metric_set(None, vec!["0", "1", "2"]);
        let events = vec![event1, event2];

        let auth_config = Auth::Bearer {
            token: SensitiveString::from("token".to_string()),
        };

        let response_result =
            export_and_fetch_with_auth(Some(auth_config.clone()), Some(auth_config), events, false)
                .await;

        assert!(response_result.is_ok());

        let body = response_result.expect("Cannot extract body from the response");

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

    #[tokio::test]
    async fn prometheus_missing_auth() {
        let (_, event1) = create_metric_gauge(None, 123.4);
        let (_, event2) = tests::create_metric_set(None, vec!["0", "1", "2"]);
        let events = vec![event1, event2];

        let server_auth_config = Auth::Bearer {
            token: SensitiveString::from("token".to_string()),
        };

        let response_result =
            export_and_fetch_with_auth(Some(server_auth_config), None, events, false).await;

        assert!(response_result.is_err());
        assert_eq!(response_result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn prometheus_wrong_auth() {
        let (_, event1) = create_metric_gauge(None, 123.4);
        let (_, event2) = tests::create_metric_set(None, vec!["0", "1", "2"]);
        let events = vec![event1, event2];

        let server_auth_config = Auth::Bearer {
            token: SensitiveString::from("token".to_string()),
        };

        let client_auth_config = Auth::Basic {
            user: "user".to_string(),
            password: SensitiveString::from("password".to_string()),
        };

        let response_result = export_and_fetch_with_auth(
            Some(server_auth_config),
            Some(client_auth_config),
            events,
            false,
        )
        .await;

        assert!(response_result.is_err());
        assert_eq!(response_result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn encoding_gzip() {
        let (name1, event1) = create_metric_gauge(None, 123.4);
        let events = vec![event1];

        let body_raw = export_and_fetch_raw(None, events, false, Some(String::from("gzip"))).await;
        let expected = format!(
            indoc! {r#"
                # HELP {name} {name}
                # TYPE {name} gauge
                {name}{{some_tag="some_value"}} 123.4
            "#},
            name = name1,
        );

        let mut gz = GzDecoder::new(&body_raw[..]);
        let mut body_decoded = String::new();
        let _ = gz.read_to_string(&mut body_decoded);

        assert!(body_raw.len() < expected.len());
        assert_eq!(body_decoded, expected);
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

        let body = export_and_fetch(None, events, false).await;
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

    #[tokio::test]
    async fn suppress_timestamp() {
        let timestamp = Utc::now();
        let (name, event) = create_metric_gauge(None, 123.4);
        let event = Event::from(event.into_metric().with_timestamp(Some(timestamp)));
        let events = vec![event];

        let body = export_and_fetch(None, events, true).await;
        assert_eq!(
            body,
            format!(
                indoc! {r#"
                    # HELP {name} {name}
                    # TYPE {name} gauge
                    {name}{{some_tag="some_value"}} 123.4
                "#},
                name = name,
            )
        );
    }

    /// According to the [spec](https://github.com/OpenObservability/OpenMetrics/blob/main/specification/OpenMetrics.md?plain=1#L115)
    /// > Label names MUST be unique within a LabelSet.
    /// Prometheus itself will reject the metric with an error. Largely to remain backward compatible with older versions of Vector,
    /// we only publish the last tag in the list.
    #[tokio::test]
    async fn prometheus_duplicate_labels() {
        let (name, event) = create_metric_with_tags(
            None,
            MetricValue::Gauge { value: 123.4 },
            Some(metric_tags!("code" => "200", "code" => "success")),
        );
        let events = vec![event];

        let response_result = export_and_fetch_with_auth(None, None, events, false).await;

        assert!(response_result.is_ok());

        let body = response_result.expect("Cannot extract body from the response");

        assert!(body.contains(&format!(
            indoc! {r#"
               # HELP {name} {name}
               # TYPE {name} gauge
               {name}{{code="success"}} 123.4
            "# },
            name = name
        )));
    }

    async fn export_and_fetch_raw(
        tls_config: Option<TlsEnableableConfig>,
        mut events: Vec<Event>,
        suppress_timestamp: bool,
        encoding: Option<String>,
    ) -> hyper::body::Bytes {
        trace_init();

        let client_settings = MaybeTlsSettings::from_config(&tls_config, false).unwrap();
        let proto = client_settings.http_protocol_name();

        let address = next_addr();
        let config = PrometheusExporterConfig {
            address,
            tls: tls_config,
            suppress_timestamp,
            ..Default::default()
        };

        // Set up acknowledgement notification
        let mut receiver = BatchNotifier::apply_to(&mut events[..]);
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));

        let (sink, _) = config.build(SinkContext::default()).await.unwrap();
        let (_, delayed_event) = create_metric_gauge(Some("delayed".to_string()), 123.4);
        let sink_handle = tokio::spawn(run_and_assert_sink_compliance(
            sink,
            stream::iter(events).chain(stream::once(async move {
                // Wait a bit to have time to scrape metrics
                time::sleep(time::Duration::from_millis(500)).await;
                delayed_event
            })),
            &SINK_TAGS,
        ));

        time::sleep(time::Duration::from_millis(100)).await;

        // Events are marked as delivered as soon as they are aggregated.
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let mut request = Request::get(format!("{}://{}/metrics", proto, address))
            .body(Body::empty())
            .expect("Error creating request.");

        if let Some(ref encoding) = encoding {
            request.headers_mut().insert(
                http::header::ACCEPT_ENCODING,
                HeaderValue::from_str(encoding.as_str()).unwrap(),
            );
        }

        let proxy = ProxyConfig::default();
        let result = HttpClient::new(client_settings, &proxy)
            .unwrap()
            .send(request)
            .await
            .expect("Could not fetch query");

        assert!(result.status().is_success());

        if encoding.is_some() {
            assert!(result
                .headers()
                .contains_key(http::header::CONTENT_ENCODING));
        }

        let body = result.into_body();
        let bytes = hyper::body::to_bytes(body)
            .await
            .expect("Reading body failed");

        sink_handle.await.unwrap();

        bytes
    }

    async fn export_and_fetch(
        tls_config: Option<TlsEnableableConfig>,
        events: Vec<Event>,
        suppress_timestamp: bool,
    ) -> String {
        let bytes = export_and_fetch_raw(tls_config, events, suppress_timestamp, None);
        String::from_utf8(bytes.await.to_vec()).unwrap()
    }

    async fn export_and_fetch_with_auth(
        server_auth_config: Option<Auth>,
        client_auth_config: Option<Auth>,
        mut events: Vec<Event>,
        suppress_timestamp: bool,
    ) -> Result<String, http::status::StatusCode> {
        trace_init();

        let client_settings = MaybeTlsSettings::from_config(&None, false).unwrap();
        let proto = client_settings.http_protocol_name();

        let address = next_addr();
        let config = PrometheusExporterConfig {
            address,
            auth: server_auth_config,
            tls: None,
            suppress_timestamp,
            ..Default::default()
        };

        // Set up acknowledgement notification
        let mut receiver = BatchNotifier::apply_to(&mut events[..]);
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));

        let (sink, _) = config.build(SinkContext::default()).await.unwrap();
        let (_, delayed_event) = create_metric_gauge(Some("delayed".to_string()), 123.4);
        let sink_handle = tokio::spawn(run_and_assert_sink_compliance(
            sink,
            stream::iter(events).chain(stream::once(async move {
                // Wait a bit to have time to scrape metrics
                time::sleep(time::Duration::from_millis(500)).await;
                delayed_event
            })),
            &SINK_TAGS,
        ));

        time::sleep(time::Duration::from_millis(100)).await;

        // Events are marked as delivered as soon as they are aggregated.
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let mut request = Request::get(format!("{}://{}/metrics", proto, address))
            .body(Body::empty())
            .expect("Error creating request.");

        if let Some(client_auth_config) = client_auth_config {
            client_auth_config.apply(&mut request);
        }

        let proxy = ProxyConfig::default();
        let result = HttpClient::new(client_settings, &proxy)
            .unwrap()
            .send(request)
            .await
            .expect("Could not fetch query");

        if !result.status().is_success() {
            return Err(result.status());
        }

        let body = result.into_body();
        let bytes = hyper::body::to_bytes(body)
            .await
            .expect("Reading body failed");
        let result = String::from_utf8(bytes.to_vec()).unwrap();

        sink_handle.await.unwrap();

        Ok(result)
    }

    async fn export_and_fetch_simple(tls_config: Option<TlsEnableableConfig>) {
        let (name1, event1) = create_metric_gauge(None, 123.4);
        let (name2, event2) = tests::create_metric_set(None, vec!["0", "1", "2"]);
        let events = vec![event1, event2];

        let body = export_and_fetch(tls_config, events, false).await;

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

    fn create_metric(name: Option<String>, value: MetricValue) -> (String, Event) {
        create_metric_with_tags(name, value, Some(metric_tags!("some_tag" => "some_value")))
    }

    fn create_metric_with_tags(
        name: Option<String>,
        value: MetricValue,
        tags: Option<MetricTags>,
    ) -> (String, Event) {
        let name = name.unwrap_or_else(|| format!("vector_set_{}", random_string(16)));
        let event = Metric::new(name.clone(), MetricKind::Incremental, value)
            .with_tags(tags)
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

        let sink = PrometheusExporter::new(config);

        let m1 = Metric::new(
            "absolute",
            MetricKind::Absolute,
            MetricValue::Counter { value: 32. },
        )
        .with_tags(Some(metric_tags!("tag1" => "value1")));

        let m2 = m1.clone().with_tags(Some(metric_tags!("tag1" => "value2")));

        let events = vec![
            Event::Metric(m1.clone().with_value(MetricValue::Counter { value: 32. })),
            Event::Metric(m2.clone().with_value(MetricValue::Counter { value: 33. })),
            Event::Metric(m1.clone().with_value(MetricValue::Counter { value: 40. })),
        ];

        let metrics_handle = Arc::clone(&sink.metrics);

        let sink = VectorSink::from_event_streamsink(sink);
        let input_events = stream::iter(events).map(Into::into);
        sink.run(input_events).await.unwrap();

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

        let sink = PrometheusExporter::new(config);

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

        let sink = VectorSink::from_event_streamsink(sink);
        let input_events = stream::iter(events).map(Into::into);
        sink.run(input_events).await.unwrap();

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
        // The render code is actually what will end up rendering those sketches as aggregated
        // summaries in the scrape output.
        let config = PrometheusExporterConfig {
            address: next_addr(), // Not actually bound, just needed to fill config
            tls: None,
            distributions_as_summaries: true,
            ..Default::default()
        };

        let sink = PrometheusExporter::new(config);

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

        let sink = VectorSink::from_event_streamsink(sink);
        let input_events = stream::iter(events).map(Into::into);
        sink.run(input_events).await.unwrap();

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
    async fn sink_gauge_incremental_absolute_mix() {
        // Because Prometheus does not, itself, have the concept of an Incremental metric, the
        // Exporter must apply a normalization function that converts all metrics to Absolute ones
        // before handling them.

        // This test ensures that this normalization works correctly when applied to a mix of both
        // Incremental and Absolute inputs.
        let config = PrometheusExporterConfig {
            address: next_addr(), // Not actually bound, just needed to fill config
            tls: None,
            ..Default::default()
        };

        let sink = PrometheusExporter::new(config);

        let base_absolute_gauge_metric = Metric::new(
            "gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 100.0 },
        );

        let base_incremental_gauge_metric = Metric::new(
            "gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value: -10.0 },
        );

        let metrics = vec![
            base_absolute_gauge_metric.clone(),
            base_absolute_gauge_metric
                .clone()
                .with_value(MetricValue::Gauge { value: 333.0 }),
            base_incremental_gauge_metric.clone(),
            base_incremental_gauge_metric
                .clone()
                .with_value(MetricValue::Gauge { value: 4.0 }),
        ];

        // Now run the events through the sink and see what ends up in the internal metric map.
        let metrics_handle = Arc::clone(&sink.metrics);

        let events = metrics
            .iter()
            .cloned()
            .map(Event::Metric)
            .collect::<Vec<_>>();

        let sink = VectorSink::from_event_streamsink(sink);
        let input_events = stream::iter(events).map(Into::into);
        sink.run(input_events).await.unwrap();

        let metrics_after = metrics_handle.read().unwrap();

        // The gauge metric should be present.
        assert_eq!(metrics_after.len(), 1);

        let expected_gauge = Metric::new(
            "gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 327.0 },
        );

        let actual_gauge = metrics_after
            .get(&MetricRef::from_metric(&expected_gauge))
            .expect("gauge metric should exist");
        assert_eq!(actual_gauge.0.value(), expected_gauge.value());
    }
}

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests {
    #![allow(clippy::print_stdout)] // tests
    #![allow(clippy::print_stderr)] // tests
    #![allow(clippy::dbg_macro)] // tests

    use chrono::Utc;
    use futures::{future::ready, stream};
    use serde_json::Value;
    use tokio::{sync::mpsc, time};
    use tokio_stream::wrappers::UnboundedReceiverStream;

    use super::*;
    use crate::{
        config::ProxyConfig,
        http::HttpClient,
        test_util::{
            components::{run_and_assert_sink_compliance, SINK_TAGS},
            trace_init,
        },
    };

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
        let (sink, _) = config.build(SinkContext::default()).await.unwrap();
        let (name, event) = tests::create_metric_gauge(None, 123.4);
        let (_, delayed_event) = tests::create_metric_gauge(Some("delayed".to_string()), 123.4);

        run_and_assert_sink_compliance(
            sink,
            stream::once(ready(event)).chain(stream::once(async move {
                // Wait a bit for the prometheus server to scrape the metrics
                time::sleep(time::Duration::from_secs(2)).await;
                delayed_event
            })),
            &SINK_TAGS,
        )
        .await;

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
        let (sink, _) = config.build(SinkContext::default()).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        let input_events = UnboundedReceiverStream::new(rx);

        let input_events = input_events.map(Into::into);
        let sink_handle = tokio::spawn(async move { sink.run(input_events).await.unwrap() });

        // Create two sets with different names but the same size.
        let (name1, event) = tests::create_metric_set(None, vec!["0", "1", "2"]);
        tx.send(event).expect("Failed to send.");
        let (name2, event) = tests::create_metric_set(None, vec!["3", "4", "5"]);
        tx.send(event).expect("Failed to send.");

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
        tx.send(event).expect("Failed to send.");

        // Again, wait for the Prometheus server to scrape the metrics, and then query it again.
        time::sleep(time::Duration::from_secs(2)).await;
        let result = prometheus_query(&name1).await;
        assert_eq!(result["data"]["result"][0]["value"][1], Value::Null);
        let result = prometheus_query(&name2).await;
        assert_eq!(
            result["data"]["result"][0]["value"][1],
            Value::String("2".into())
        );

        drop(tx);
        sink_handle.await.unwrap();
    }

    async fn expire_on_flush_period() {
        let config = PrometheusExporterConfig {
            address: sink_exporter_address().parse().unwrap(),
            flush_period_secs: Duration::from_secs(3),
            ..Default::default()
        };
        let (sink, _) = config.build(SinkContext::default()).await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        let input_events = UnboundedReceiverStream::new(rx);

        let input_events = input_events.map(Into::into);
        let sink_handle = tokio::spawn(async move { sink.run(input_events).await.unwrap() });

        // metrics that will not be updated for a full flush period and therefore should expire
        let (name1, event) = tests::create_metric_set(None, vec!["42"]);
        tx.send(event).expect("Failed to send.");
        let (name2, event) = tests::create_metric_gauge(None, 100.0);
        tx.send(event).expect("Failed to send.");

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
            tx.send(event).expect("Failed to send.");

            // Wait a bit for time to pass
            time::sleep(time::Duration::from_secs(1)).await;
        }

        // Exporter should present only the one that got updated
        let body = fetch_exporter_body().await;
        assert!(body.contains(&name1));
        assert!(!body.contains(&name2));

        drop(tx);
        sink_handle.await.unwrap();
    }
}

use std::sync::Arc;
use std::task;

use aws_types::credentials::SharedCredentialsProvider;
use aws_types::region::Region;
use bytes::{Bytes, BytesMut};
use futures::{future::BoxFuture, stream, FutureExt, SinkExt};
use http::{Request, Uri};
use prost::Message;
use snafu::{ResultExt, Snafu};
use tower::Service;
use vector_config::configurable_component;
use vector_core::ByteSizeOf;

use super::collector::{self, MetricCollector as _};
use crate::{
    aws::RegionOrEndpoint,
    config::{self, AcknowledgementsConfig, Input, SinkConfig},
    event::{Event, Metric},
    http::{Auth, HttpClient},
    internal_events::{EndpointBytesSent, TemplateRenderingError},
    sinks::{
        self,
        prometheus::PrometheusRemoteWriteAuth,
        util::{
            batch::BatchConfig,
            buffer::metrics::{MetricNormalize, MetricNormalizer, MetricSet, MetricsBuffer},
            http::HttpRetryLogic,
            uri, EncodedEvent, PartitionBuffer, PartitionInnerBuffer, SinkBatchSettings,
            TowerRequestConfig,
        },
    },
    template::Template,
    tls::{TlsConfig, TlsSettings},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct PrometheusRemoteWriteDefaultBatchSettings;

impl SinkBatchSettings for PrometheusRemoteWriteDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1_000);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Debug, Snafu)]
enum Errors {
    #[snafu(display(r#"Prometheus remote_write sink cannot accept "set" metrics"#))]
    SetMetricInvalid,
    #[snafu(display("aws.region required when AWS authentication is in use"))]
    AwsRegionRequired,
}

/// Configuration for the `prometheus_remote_write` sink.
#[configurable_component(sink("prometheus_remote_write"))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct RemoteWriteConfig {
    /// The endpoint to send data to.
    ///
    /// The endpoint should include the scheme and the path to write to.
    #[configurable(metadata(docs::examples = "https://localhost:8087/api/v1/write"))]
    pub endpoint: String,

    /// The default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with an underscore (`_`).
    ///
    /// It should follow the Prometheus [naming conventions][prom_naming_docs].
    ///
    /// [prom_naming_docs]: https://prometheus.io/docs/practices/naming/#metric-names
    #[configurable(metadata(docs::examples = "service"))]
    #[configurable(metadata(docs::advanced))]
    pub default_namespace: Option<String>,

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

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<PrometheusRemoteWriteDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    /// The tenant ID to send.
    ///
    /// If set, a header named `X-Scope-OrgID` is added to outgoing requests with the value of this setting.
    ///
    /// This may be used by Cortex or other remote services to identify the tenant making the request.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "my-domain"))]
    #[configurable(metadata(docs::advanced))]
    pub tenant_id: Option<Template>,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    pub auth: Option<PrometheusRemoteWriteAuth>,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    pub aws: Option<RegionOrEndpoint>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl_generate_config_from_default!(RemoteWriteConfig);

#[async_trait::async_trait]
impl SinkConfig for RemoteWriteConfig {
    async fn build(
        &self,
        cx: config::SinkContext,
    ) -> crate::Result<(sinks::VectorSink, sinks::Healthcheck)> {
        let endpoint = self.endpoint.parse::<Uri>().context(sinks::UriParseSnafu)?;
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let batch = self.batch.into_batch_settings()?;
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let buckets = self.buckets.clone();
        let quantiles = self.quantiles.clone();

        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let tenant_id = self.tenant_id.clone();

        let (http_auth, credentials_provider, aws_region) = match &self.auth {
            Some(PrometheusRemoteWriteAuth::Basic { user, password }) => (
                Some(Auth::Basic {
                    user: user.clone(),
                    password: password.clone().into(),
                }),
                None,
                None,
            ),
            Some(PrometheusRemoteWriteAuth::Bearer { token }) => (
                Some(Auth::Bearer {
                    token: token.clone(),
                }),
                None,
                None,
            ),
            Some(PrometheusRemoteWriteAuth::Aws(aws_auth)) => {
                let region = self
                    .aws
                    .as_ref()
                    .map(|config| config.region())
                    .ok_or(Errors::AwsRegionRequired)?
                    .ok_or(Errors::AwsRegionRequired)?;

                (
                    None,
                    Some(aws_auth.credentials_provider(region.clone()).await?),
                    Some(region),
                )
            }
            None => (None, None, None),
        };

        let http_request_builder = Arc::new(HttpRequestBuilder {
            endpoint: endpoint.clone(),
            aws_region,
            credentials_provider,
            http_auth,
        });

        let healthcheck = healthcheck(client.clone(), Arc::clone(&http_request_builder)).boxed();
        let service = RemoteWriteService {
            default_namespace: self.default_namespace.clone(),
            client,
            buckets,
            quantiles,
            http_request_builder,
        };

        let sink = {
            let buffer = PartitionBuffer::new(MetricsBuffer::new(batch.size));
            let mut normalizer = MetricNormalizer::<PrometheusMetricNormalize>::default();

            request_settings
                .partition_sink(HttpRetryLogic, service, buffer, batch.timeout)
                .with_flat_map(move |event: Event| {
                    let byte_size = event.size_of();
                    stream::iter(normalizer.normalize(event.into_metric()).map(|event| {
                        let tenant_id = tenant_id.as_ref().and_then(|template| {
                            template
                                .render_string(&event)
                                .map_err(|error| {
                                    emit!(TemplateRenderingError {
                                        error,
                                        field: Some("tenant_id"),
                                        drop_event: true,
                                    })
                                })
                                .ok()
                        });
                        let key = PartitionKey { tenant_id };
                        Ok(EncodedEvent::new(
                            PartitionInnerBuffer::new(event, key),
                            byte_size,
                        ))
                    }))
                })
                .sink_map_err(
                    |error| error!(message = "Prometheus remote_write sink error.", %error),
                )
        };

        Ok((sinks::VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct PartitionKey {
    tenant_id: Option<String>,
}

async fn healthcheck(
    client: HttpClient,
    http_request_builder: Arc<HttpRequestBuilder>,
) -> crate::Result<()> {
    let body = bytes::Bytes::new();
    let request = http_request_builder
        .build_request(http::Method::GET, body.into(), None)
        .await?;
    let response = client.send(request).await?;

    match response.status() {
        http::StatusCode::OK => Ok(()),
        other => Err(sinks::HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}

#[derive(Default)]
pub struct PrometheusMetricNormalize;

impl MetricNormalize for PrometheusMetricNormalize {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        state.make_absolute(metric)
    }
}

#[derive(Clone)]
struct RemoteWriteService {
    default_namespace: Option<String>,
    client: HttpClient,
    buckets: Vec<f64>,
    quantiles: Vec<f64>,
    http_request_builder: Arc<HttpRequestBuilder>,
}

impl RemoteWriteService {
    fn encode_events(&self, metrics: Vec<Metric>) -> Bytes {
        let mut time_series = collector::TimeSeries::new();
        for metric in metrics {
            time_series.encode_metric(
                self.default_namespace.as_deref(),
                &self.buckets,
                &self.quantiles,
                &metric,
            );
        }
        let request = time_series.finish();

        let mut out = BytesMut::with_capacity(request.encoded_len());
        request.encode(&mut out).expect("Out of memory");
        out.freeze()
    }
}

impl Service<PartitionInnerBuffer<Vec<Metric>, PartitionKey>> for RemoteWriteService {
    type Response = http::Response<Bytes>;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _task: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        task::Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, buffer: PartitionInnerBuffer<Vec<Metric>, PartitionKey>) -> Self::Future {
        let (events, key) = buffer.into_parts();
        let body = self.encode_events(events);
        let body = snap_block(body);

        let client = self.client.clone();
        let request_builder = Arc::clone(&self.http_request_builder);

        Box::pin(async move {
            let request = request_builder
                .build_request(http::Method::POST, body, key.tenant_id)
                .await?;

            let (protocol, endpoint) = uri::protocol_endpoint(request.uri().clone());

            let response = client.send(request).await?;
            let (parts, body) = response.into_parts();
            let body = hyper::body::to_bytes(body).await?;

            emit!(EndpointBytesSent {
                byte_size: body.len(),
                protocol: &protocol,
                endpoint: &endpoint
            });

            Ok(hyper::Response::from_parts(parts, body))
        })
    }
}

pub struct HttpRequestBuilder {
    pub endpoint: Uri,
    pub aws_region: Option<Region>,
    pub http_auth: Option<Auth>,
    pub credentials_provider: Option<SharedCredentialsProvider>,
}

impl HttpRequestBuilder {
    pub async fn build_request(
        &self,
        method: http::Method,
        body: Vec<u8>,
        tenant_id: Option<String>,
    ) -> Result<Request<hyper::Body>, crate::Error> {
        let mut builder = http::Request::builder()
            .method(method)
            .uri(self.endpoint.clone())
            .header("X-Prometheus-Remote-Write-Version", "0.1.0")
            .header("Content-Encoding", "snappy")
            .header("Content-Type", "application/x-protobuf");

        if let Some(tenant_id) = &tenant_id {
            builder = builder.header("X-Scope-OrgID", tenant_id);
        }

        let mut request = builder.body(body.into()).unwrap();
        if let Some(http_auth) = &self.http_auth {
            http_auth.apply(&mut request);
        }

        if let Some(credentials_provider) = &self.credentials_provider {
            sign_request(&mut request, credentials_provider, &self.aws_region).await?;
        }

        let (parts, body) = request.into_parts();
        let request: Request<hyper::Body> = hyper::Request::from_parts(parts, body.into());

        Ok(request)
    }
}

fn snap_block(data: Bytes) -> Vec<u8> {
    snap::raw::Encoder::new()
        .compress_vec(&data)
        .expect("Out of memory")
}

async fn sign_request(
    request: &mut http::Request<Bytes>,
    credentials_provider: &SharedCredentialsProvider,
    region: &Option<Region>,
) -> crate::Result<()> {
    crate::aws::sign_request("aps", request, credentials_provider, region).await
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use http::HeaderMap;
    use indoc::indoc;
    use prometheus_parser::proto;
    use vector_core::metric_tags;

    use super::*;
    use crate::{
        config::SinkContext,
        event::{MetricKind, MetricValue},
        sinks::util::test::build_test_server,
        test_util::{
            self,
            components::{assert_sink_compliance, HTTP_SINK_TAGS},
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RemoteWriteConfig>();
    }

    macro_rules! labels {
        ( $( $name:expr => $value:expr ),* ) => {
            vec![ $( proto::Label {
                name: $name.to_string(),
                value: $value.to_string()
            }, )* ]
        }
    }

    #[tokio::test]
    async fn sends_request() {
        let outputs = send_request("", vec![create_event("gauge-2".into(), 32.0)]).await;
        assert_eq!(outputs.len(), 1);
        let (headers, req) = &outputs[0];

        assert!(!headers.contains_key("x-scope-orgid"));

        assert_eq!(req.timeseries.len(), 1);
        assert_eq!(
            req.timeseries[0].labels,
            labels!("__name__" => "gauge-2", "production" => "true", "region" => "us-west-1")
        );
        assert_eq!(req.timeseries[0].samples.len(), 1);
        assert_eq!(req.timeseries[0].samples[0].value, 32.0);
        assert_eq!(req.metadata.len(), 1);
        assert_eq!(req.metadata[0].r#type, proto::MetricType::Gauge as i32);
        assert_eq!(req.metadata[0].metric_family_name, "gauge-2");
    }

    #[tokio::test]
    async fn sends_authenticated_request() {
        let outputs = send_request(
            indoc! {r#"
                tenant_id = "tenant-%Y"
                [auth]
                strategy = "basic"
                user = "user"
                password = "password"
            "#},
            vec![create_event("gauge-2".into(), 32.0)],
        )
        .await;

        assert_eq!(outputs.len(), 1);
        let (_headers, req) = &outputs[0];

        assert_eq!(req.timeseries.len(), 1);
        assert_eq!(
            req.timeseries[0].labels,
            labels!("__name__" => "gauge-2", "production" => "true", "region" => "us-west-1")
        );
        assert_eq!(req.timeseries[0].samples.len(), 1);
        assert_eq!(req.timeseries[0].samples[0].value, 32.0);
        assert_eq!(req.metadata.len(), 1);
        assert_eq!(req.metadata[0].r#type, proto::MetricType::Gauge as i32);
        assert_eq!(req.metadata[0].metric_family_name, "gauge-2");
    }

    #[tokio::test]
    async fn sends_authenticated_aws_request() {
        let outputs = send_request(
            indoc! {r#"
                tenant_id = "tenant-%Y"
                [aws]
                region = "foo"
                [auth]
                strategy = "aws"
                access_key_id = "foo"
                secret_access_key = "bar"
            "#},
            vec![create_event("gauge-2".into(), 32.0)],
        )
        .await;

        assert_eq!(outputs.len(), 1);
        let (headers, _req) = &outputs[0];

        let auth = headers["authorization"]
            .to_str()
            .expect("Missing AWS authorization header");
        assert!(auth.starts_with("AWS4-HMAC-SHA256"));
    }

    #[tokio::test]
    async fn sends_x_scope_orgid_header() {
        let outputs = send_request(
            r#"tenant_id = "tenant""#,
            vec![create_event("gauge-3".into(), 12.0)],
        )
        .await;

        assert_eq!(outputs.len(), 1);
        let (headers, _) = &outputs[0];
        assert_eq!(headers["x-scope-orgid"], "tenant");
    }

    #[tokio::test]
    async fn sends_templated_x_scope_orgid_header() {
        let outputs = send_request(
            r#"tenant_id = "tenant-%Y""#,
            vec![create_event("gauge-3".into(), 12.0)],
        )
        .await;

        assert_eq!(outputs.len(), 1);
        let (headers, _) = &outputs[0];
        let orgid = headers["x-scope-orgid"]
            .to_str()
            .expect("Missing x-scope-orgid header");
        assert!(orgid.starts_with("tenant-20"));
        assert_eq!(orgid.len(), 11);
    }

    #[tokio::test]
    async fn retains_state_between_requests() {
        // This sink converts all incremental events to absolute, and
        // should accumulate their totals between batches.
        let outputs = send_request(
            r#"batch.max_events = 1"#,
            vec![
                create_inc_event("counter-1".into(), 12.0),
                create_inc_event("counter-2".into(), 13.0),
                create_inc_event("counter-1".into(), 14.0),
            ],
        )
        .await;

        assert_eq!(outputs.len(), 3);

        let check_output = |index: usize, name: &str, value: f64| {
            let (_, req) = &outputs[index];
            assert_eq!(req.timeseries.len(), 1);
            assert_eq!(req.timeseries[0].labels, labels!("__name__" => name));
            assert_eq!(req.timeseries[0].samples.len(), 1);
            assert_eq!(req.timeseries[0].samples[0].value, value);
        };
        check_output(0, "counter-1", 12.0);
        check_output(1, "counter-2", 13.0);
        check_output(2, "counter-1", 26.0);
    }

    async fn send_request(
        config: &str,
        events: Vec<Event>,
    ) -> Vec<(HeaderMap, proto::WriteRequest)> {
        assert_sink_compliance(&HTTP_SINK_TAGS, async {
            let addr = test_util::next_addr();
            let (rx, trigger, server) = build_test_server(addr);
            tokio::spawn(server);

            let config = format!("endpoint = \"http://{}/write\"\n{}", addr, config);
            let config: RemoteWriteConfig = toml::from_str(&config).unwrap();
            let cx = SinkContext::new_test();

            let (sink, _) = config.build(cx).await.unwrap();
            sink.run_events(events).await.unwrap();

            drop(trigger);

            rx.map(|(parts, body)| {
                assert_eq!(parts.method, "POST");
                assert_eq!(parts.uri.path(), "/write");
                let headers = parts.headers;
                assert_eq!(headers["x-prometheus-remote-write-version"], "0.1.0");
                assert_eq!(headers["content-encoding"], "snappy");
                assert_eq!(headers["content-type"], "application/x-protobuf");

                if config.auth.is_some() {
                    assert!(headers.contains_key("authorization"));
                }

                let decoded = snap::raw::Decoder::new()
                    .decompress_vec(&body)
                    .expect("Invalid snappy compressed data");
                let request =
                    proto::WriteRequest::decode(Bytes::from(decoded)).expect("Invalid protobuf");
                (headers, request)
            })
            .collect::<Vec<_>>()
            .await
        })
        .await
    }

    pub(super) fn create_event(name: String, value: f64) -> Event {
        Metric::new(name, MetricKind::Absolute, MetricValue::Gauge { value })
            .with_tags(Some(metric_tags!(
                "region" => "us-west-1",
                "production" => "true",
            )))
            .with_timestamp(Some(chrono::Utc::now()))
            .into()
    }

    fn create_inc_event(name: String, value: f64) -> Event {
        Metric::new(
            name,
            MetricKind::Incremental,
            MetricValue::Counter { value },
        )
        .with_timestamp(Some(chrono::Utc::now()))
        .into()
    }
}

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests {
    use std::{collections::HashMap, ops::Range};

    use serde_json::Value;

    use super::{tests::*, *};
    use crate::{
        config::{SinkConfig, SinkContext},
        event::{metric::MetricValue, Event},
        sinks::influxdb::test_util::{cleanup_v1, format_timestamp, onboarding_v1, query_v1},
        test_util::components::{assert_sink_compliance, HTTP_SINK_TAGS},
        tls::{self, TlsConfig},
    };

    const HTTP_URL: &str = "http://influxdb-v1:8086";
    const HTTPS_URL: &str = "https://influxdb-v1-tls:8087";

    #[tokio::test]
    async fn insert_metrics_over_http() {
        insert_metrics(HTTP_URL).await;
    }

    #[tokio::test]
    async fn insert_metrics_over_https() {
        insert_metrics(HTTPS_URL).await;
    }

    async fn insert_metrics(url: &str) {
        assert_sink_compliance(&HTTP_SINK_TAGS, async {
            let database = onboarding_v1(url).await;

            let cx = SinkContext::new_test();

            let config = RemoteWriteConfig {
                endpoint: format!("{}/api/v1/prom/write?db={}", url, database),
                tls: Some(TlsConfig {
                    ca_file: Some(tls::TEST_PEM_CA_PATH.into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let events = create_events(0..5, |n| n * 11.0);

            let (sink, _) = config.build(cx).await.expect("error building config");
            sink.run_events(events.clone()).await.unwrap();

            let result = query(url, &format!("show series on {}", database)).await;

            let values = &result["results"][0]["series"][0]["values"];
            assert_eq!(values.as_array().unwrap().len(), 5);

            for event in events {
                let metric = event.into_metric();
                let result = query(
                    url,
                    &format!(r#"SELECT * FROM "{}".."{}""#, database, metric.name()),
                )
                .await;

                let metrics = decode_metrics(&result["results"][0]["series"][0]);
                assert_eq!(metrics.len(), 1);
                let output = &metrics[0];

                match metric.value() {
                    MetricValue::Gauge { value } => {
                        assert_eq!(output["value"], Value::Number((*value as u32).into()))
                    }
                    _ => panic!("Unhandled metric value, fix the test"),
                }
                for (tag, value) in metric.tags().unwrap().iter_single() {
                    assert_eq!(output[tag], Value::String(value.to_string()));
                }
                let timestamp =
                    format_timestamp(metric.timestamp().unwrap(), chrono::SecondsFormat::Millis);
                assert_eq!(output["time"], Value::String(timestamp));
            }

            cleanup_v1(url, &database).await;
        })
        .await
    }

    async fn query(url: &str, query: &str) -> Value {
        let result = query_v1(url, query).await;
        let text = result.text().await.unwrap();
        serde_json::from_str(&text).expect("error when parsing InfluxDB response JSON")
    }

    fn decode_metrics(data: &Value) -> Vec<HashMap<String, Value>> {
        let data = data.as_object().expect("Data is not an object");
        let columns = data["columns"].as_array().expect("Columns is not an array");
        data["values"]
            .as_array()
            .expect("Values is not an array")
            .iter()
            .map(|values| {
                columns
                    .iter()
                    .zip(values.as_array().unwrap().iter())
                    .map(|(column, value)| (column.as_str().unwrap().to_owned(), value.clone()))
                    .collect()
            })
            .collect()
    }

    fn create_events(name_range: Range<i32>, value: impl Fn(f64) -> f64) -> Vec<Event> {
        name_range
            .map(move |num| create_event(format!("metric_{}", num), value(num as f64)))
            .collect()
    }
}

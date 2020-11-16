use super::{
    collector::{self, MetricCollector as _},
    proto,
};
use crate::{
    config::{self, SinkConfig, SinkDescription},
    event::Metric,
    http::HttpClient,
    sinks::{
        self,
        util::{
            http::HttpRetryLogic, BatchConfig, BatchSettings, MetricBuffer, TowerRequestConfig,
        },
    },
    tls::{TlsOptions, TlsSettings},
};
use bytes::{Bytes, BytesMut};
use futures::{future::BoxFuture, FutureExt, SinkExt};
use http::Uri;
use prost::Message;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::task;

#[derive(Debug, Snafu)]
enum Errors {
    #[snafu(display(r#"Prometheus remote_write sink cannot accept "set" metrics"#))]
    SetMetricInvalid,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct RemoteWriteConfig {
    endpoint: String,

    default_namespace: Option<String>,

    #[serde(default = "super::default_histogram_buckets")]
    buckets: Vec<f64>,
    #[serde(default = "super::default_summary_quantiles")]
    quantiles: Vec<f64>,

    #[serde(default)]
    batch: BatchConfig,
    #[serde(default)]
    request: TowerRequestConfig,

    tls: Option<TlsOptions>,
}

inventory::submit! {
    SinkDescription::new::<RemoteWriteConfig>("prometheus_remote_write")
}

lazy_static::lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = Default::default();
}

impl_generate_config_from_default!(RemoteWriteConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_remote_write")]
impl SinkConfig for RemoteWriteConfig {
    async fn build(
        &self,
        cx: config::SinkContext,
    ) -> crate::Result<(sinks::VectorSink, sinks::Healthcheck)> {
        let endpoint = self.endpoint.parse::<Uri>().context(sinks::UriParseError)?;
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let batch = BatchSettings::default()
            .events(1_000)
            .timeout(1)
            .parse_config(self.batch)?;
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);
        let buckets = self.buckets.clone();
        let quantiles = self.quantiles.clone();

        let client = HttpClient::new(tls_settings)?;
        let healthcheck = healthcheck(endpoint.clone(), client.clone()).boxed();
        let service = RemoteWriteService {
            endpoint,
            default_namespace: self.default_namespace.clone(),
            client,
            buckets,
            quantiles,
        };
        let sink = request
            .batch_sink(
                HttpRetryLogic,
                service,
                MetricBuffer::new(batch.size),
                batch.timeout,
                cx.acker(),
            )
            .sink_map_err(|error| error!(message = "Prometheus remote_write sink error.", %error));

        Ok((sinks::VectorSink::Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> crate::config::DataType {
        config::DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "prometheus_remote_write"
    }
}

async fn healthcheck(endpoint: Uri, client: HttpClient) -> crate::Result<()> {
    let request = http::Request::get(endpoint)
        .body(hyper::Body::empty())
        .unwrap();

    let response = client.send(request).await?;

    match response.status() {
        http::StatusCode::OK => Ok(()),
        other => Err(sinks::HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}

#[derive(Clone)]
struct RemoteWriteService {
    endpoint: Uri,
    default_namespace: Option<String>,
    client: HttpClient,
    buckets: Vec<f64>,
    quantiles: Vec<f64>,
}

impl RemoteWriteService {
    fn encode_events(&self, metrics: Vec<Metric>) -> Bytes {
        let mut time_series = collector::TimeSeries::new();
        for metric in metrics {
            time_series.encode_metric(
                self.default_namespace.as_deref(),
                &self.buckets,
                &self.quantiles,
                false,
                &metric,
            );
        }
        let timeseries = time_series.finish();

        let request = proto::WriteRequest { timeseries };
        let mut out = BytesMut::with_capacity(request.encoded_len());
        request.encode(&mut out).expect("Out of memory");
        out.freeze()
    }
}

impl tower::Service<Vec<Metric>> for RemoteWriteService {
    type Response = http::Response<Bytes>;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _task: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, events: Vec<Metric>) -> Self::Future {
        let body = self.encode_events(events);
        let body = snap_block(body);

        let request = http::Request::post(self.endpoint.clone())
            .header("X-Prometheus-Remote-Write-Version", "0.1.0")
            .header("Content-Encoding", "snappy")
            .header("Content-Type", "application/x-protobuf")
            .body(body.into())
            .unwrap();
        let client = self.client.clone();

        Box::pin(async move {
            let response = client.send(request).await?;
            let (parts, body) = response.into_parts();
            let body = hyper::body::to_bytes(body).await?;
            Ok(hyper::Response::from_parts(parts, body))
        })
    }
}

fn snap_block(data: Bytes) -> Vec<u8> {
    snap::raw::Encoder::new()
        .compress_vec(&data)
        .expect("Out of memory")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RemoteWriteConfig>();
    }
}

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests {
    use super::*;
    use crate::{
        config::{SinkConfig, SinkContext},
        event::metric::{Metric, MetricKind, MetricValue},
        sinks::influxdb::test_util::{cleanup_v1, onboarding_v1, query_v1},
        tls::TlsOptions,
        Event,
    };
    use futures::stream;
    use std::ops::Range;

    const HTTP_URL: &str = "http://localhost:8086";
    const HTTPS_URL: &str = "https://localhost:8087";

    #[tokio::test]
    async fn insert_metrics_over_http() {
        insert_metrics(HTTP_URL).await;
    }

    #[tokio::test]
    async fn insert_metrics_over_https() {
        insert_metrics(HTTPS_URL).await;
    }

    async fn insert_metrics(url: &str) {
        let database = onboarding_v1(url).await;

        let cx = SinkContext::new_test();

        let config = RemoteWriteConfig {
            endpoint: format!("{}/api/v1/prom/write?db={}", url, database),
            tls: Some(TlsOptions {
                ca_file: Some("tests/data/Vector_CA.crt".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let events = create_events(0..10);

        let (sink, _) = config.build(cx).await.expect("error building config");
        sink.run(stream::iter(events)).await.unwrap();

        let result = query_v1(url, &format!("show series on {}", database)).await;
        let text = result.text().await.unwrap();
        let result: serde_json::Value =
            serde_json::from_str(&text).expect("error when parsing InfluxDB response JSON");

        let values = &result["results"][0]["series"][0]["values"];
        assert_eq!(values.as_array().unwrap().len(), 10);

        cleanup_v1(url, &database).await;
    }

    fn create_events(range: Range<i32>) -> Vec<Event> {
        range.map(create_event).collect()
    }

    fn create_event(i: i32) -> Event {
        Event::Metric(Metric {
            name: format!("gauge-{}", i),
            namespace: None,
            timestamp: Some(chrono::Utc::now()),
            tags: Some(
                vec![
                    ("region".to_owned(), "us-west-1".to_owned()),
                    ("production".to_owned(), "true".to_owned()),
                ]
                .into_iter()
                .collect(),
            ),
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge { value: i as f64 },
        })
    }
}

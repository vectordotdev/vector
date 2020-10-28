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
use futures::{future::BoxFuture, FutureExt as _};
use futures01::Sink as _;
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
            .events(1_000_000)
            .parse_config(self.batch)?;
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);
        let buckets = self.buckets.clone();
        let quantiles = self.quantiles.clone();

        let client = HttpClient::new(tls_settings)?;
        let healthcheck = healthcheck(endpoint.clone(), client.clone()).boxed();
        let service = RemoteWriteService {
            endpoint,
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
            .sink_map_err(|error| error!("Prometheus remote_write sink error: {}", error));

        Ok((
            sinks::VectorSink::Futures01Sink(Box::new(sink)),
            healthcheck,
        ))
    }

    fn input_type(&self) -> crate::config::DataType {
        config::DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "prometheus_remote_write"
    }
}

async fn healthcheck(endpoint: Uri, mut client: HttpClient) -> crate::Result<()> {
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
    client: HttpClient,
    buckets: Vec<f64>,
    quantiles: Vec<f64>,
}

impl RemoteWriteService {
    fn encode_events(&self, metrics: Vec<Metric>) -> crate::Result<Bytes> {
        let mut time_series = collector::TimeSeries::new();
        for metric in metrics {
            time_series.encode_metric(None, &self.buckets, &self.quantiles, false, &metric);
        }
        let timeseries = time_series.finish();

        let request = proto::WriteRequest { timeseries };
        let mut out = BytesMut::with_capacity(request.encoded_len());
        request
            .encode(&mut out)
            .map(move |_| out.freeze())
            .map_err(Into::into)
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
        let body = self.encode_events(events).unwrap();
        let body = snap_block(body).unwrap();

        let request = http::Request::post(self.endpoint.clone())
            .header("X-Prometheus-Remote-Write-Version", "0.1.0")
            .header("Content-Encoding", "snappy")
            .header("Content-Type", "application/x-protobuf")
            .body(body.into())
            .unwrap();
        let mut client = self.client.clone();

        Box::pin(async move {
            let response = client.call(request).await?;
            let (parts, body) = response.into_parts();
            let body = hyper::body::to_bytes(body).await?;
            Ok(hyper::Response::from_parts(parts, body))
        })
    }
}

fn snap_block(data: Bytes) -> crate::Result<Vec<u8>> {
    snap::raw::Encoder::new()
        .compress_vec(&data)
        .map_err(Into::into)
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
}

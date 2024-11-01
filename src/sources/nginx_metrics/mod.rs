use std::{
    convert::TryFrom,
    time::{Duration, Instant},
};

use bytes::Bytes;
use chrono::Utc;
use futures::{future::join_all, StreamExt, TryFutureExt};
use http::{Request, StatusCode};
use hyper::{body::to_bytes as body_to_bytes, Body, Uri};
use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::configurable::configurable_component;
use vector_lib::{metric_tags, EstimatedJsonEncodedSizeOf};

use crate::{
    config::{SourceConfig, SourceContext, SourceOutput},
    event::metric::{Metric, MetricKind, MetricTags, MetricValue},
    http::{Auth, HttpClient},
    internal_events::{
        CollectionCompleted, EndpointBytesReceived, NginxMetricsEventsReceived,
        NginxMetricsRequestError, NginxMetricsStubStatusParseError, StreamClosedError,
    },
    tls::{TlsConfig, TlsSettings},
};

pub mod parser;
use parser::NginxStubStatus;
use vector_lib::config::LogNamespace;

macro_rules! counter {
    ($value:expr) => {
        MetricValue::Counter {
            value: $value as f64,
        }
    };
}

macro_rules! gauge {
    ($value:expr) => {
        MetricValue::Gauge {
            value: $value as f64,
        }
    };
}

#[derive(Debug, Snafu)]
enum NginxBuildError {
    #[snafu(display("Failed to parse endpoint: {}", source))]
    HostInvalidUri { source: http::uri::InvalidUri },
}

#[derive(Debug, Snafu)]
enum NginxError {
    #[snafu(display("Invalid response status: {}", status))]
    InvalidResponseStatus { status: StatusCode },
}

/// Configuration for the `nginx_metrics` source.
#[serde_as]
#[configurable_component(source("nginx_metrics", "Collect metrics from NGINX."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct NginxMetricsConfig {
    /// A list of NGINX instances to scrape.
    ///
    /// Each endpoint must be a valid HTTP/HTTPS URI pointing to an NGINX instance that has the
    /// `ngx_http_stub_status_module` module enabled.
    #[configurable(metadata(docs::examples = "http://localhost:8000/basic_status"))]
    endpoints: Vec<String>,

    /// The interval between scrapes.
    #[serde(default = "default_scrape_interval_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    scrape_interval_secs: Duration,

    /// Overrides the default namespace for the metrics emitted by the source.
    ///
    /// If set to an empty string, no namespace is added to the metrics.
    ///
    /// By default, `nginx` is used.
    #[serde(default = "default_namespace")]
    namespace: String,

    #[configurable(derived)]
    tls: Option<TlsConfig>,

    #[configurable(derived)]
    auth: Option<Auth>,
}

pub(super) const fn default_scrape_interval_secs() -> Duration {
    Duration::from_secs(15)
}

pub fn default_namespace() -> String {
    "nginx".to_string()
}

impl_generate_config_from_default!(NginxMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "nginx_metrics")]
impl SourceConfig for NginxMetricsConfig {
    async fn build(&self, mut cx: SourceContext) -> crate::Result<super::Source> {
        let tls = TlsSettings::from_options(&self.tls)?;
        let http_client = HttpClient::new(tls, &cx.proxy)?;

        let namespace = Some(self.namespace.clone()).filter(|namespace| !namespace.is_empty());
        let mut sources = Vec::with_capacity(self.endpoints.len());
        for endpoint in self.endpoints.iter() {
            sources.push(NginxMetrics::new(
                http_client.clone(),
                endpoint.clone(),
                self.auth.clone(),
                namespace.clone(),
            )?);
        }

        let duration = self.scrape_interval_secs;
        let shutdown = cx.shutdown;
        Ok(Box::pin(async move {
            let mut interval = IntervalStream::new(time::interval(duration)).take_until(shutdown);
            while interval.next().await.is_some() {
                let start = Instant::now();
                let metrics = join_all(sources.iter().map(|nginx| nginx.collect())).await;
                emit!(CollectionCompleted {
                    start,
                    end: Instant::now()
                });

                let metrics: Vec<Metric> = metrics.into_iter().flatten().collect();
                let count = metrics.len();

                if (cx.out.send_batch(metrics).await).is_err() {
                    emit!(StreamClosedError { count });
                    return Err(());
                }
            }

            Ok(())
        }))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[derive(Debug)]
struct NginxMetrics {
    http_client: HttpClient,
    endpoint: String,
    auth: Option<Auth>,
    namespace: Option<String>,
    tags: MetricTags,
}

impl NginxMetrics {
    fn new(
        http_client: HttpClient,
        endpoint: String,
        auth: Option<Auth>,
        namespace: Option<String>,
    ) -> crate::Result<Self> {
        let tags = metric_tags!(
            "endpoint" => endpoint.clone(),
            "host" => Self::get_endpoint_host(&endpoint)?,
        );

        Ok(Self {
            http_client,
            endpoint,
            auth,
            namespace,
            tags,
        })
    }

    fn get_endpoint_host(endpoint: &str) -> crate::Result<String> {
        let uri: Uri = endpoint.parse().context(HostInvalidUriSnafu)?;
        Ok(match (uri.host().unwrap_or(""), uri.port()) {
            (host, None) => host.to_owned(),
            (host, Some(port)) => format!("{}:{}", host, port),
        })
    }

    async fn collect(&self) -> Vec<Metric> {
        let (up_value, mut metrics) = match self.collect_metrics().await {
            Ok(metrics) => (1.0, metrics),
            Err(()) => (0.0, vec![]),
        };

        let byte_size = metrics.estimated_json_encoded_size_of();

        metrics.push(self.create_metric("up", gauge!(up_value)));

        emit!(NginxMetricsEventsReceived {
            count: metrics.len(),
            byte_size,
            endpoint: &self.endpoint
        });

        metrics
    }

    async fn collect_metrics(&self) -> Result<Vec<Metric>, ()> {
        let response = self.get_nginx_response().await.map_err(|error| {
            emit!(NginxMetricsRequestError {
                error,
                endpoint: &self.endpoint,
            })
        })?;
        emit!(EndpointBytesReceived {
            byte_size: response.len(),
            protocol: "http",
            endpoint: &self.endpoint,
        });

        let status = NginxStubStatus::try_from(String::from_utf8_lossy(&response).as_ref())
            .map_err(|error| {
                emit!(NginxMetricsStubStatusParseError {
                    error,
                    endpoint: &self.endpoint,
                })
            })?;

        Ok(vec![
            self.create_metric("connections_active", gauge!(status.active)),
            self.create_metric("connections_accepted_total", counter!(status.accepts)),
            self.create_metric("connections_handled_total", counter!(status.handled)),
            self.create_metric("http_requests_total", counter!(status.requests)),
            self.create_metric("connections_reading", gauge!(status.reading)),
            self.create_metric("connections_writing", gauge!(status.writing)),
            self.create_metric("connections_waiting", gauge!(status.waiting)),
        ])
    }

    async fn get_nginx_response(&self) -> crate::Result<Bytes> {
        let mut request = Request::get(&self.endpoint).body(Body::empty())?;
        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        let response = self.http_client.send(request).await?;
        let (parts, body) = response.into_parts();
        match parts.status {
            StatusCode::OK => body_to_bytes(body).err_into().await,
            status => Err(Box::new(NginxError::InvalidResponseStatus { status })),
        }
    }

    fn create_metric(&self, name: &str, value: MetricValue) -> Metric {
        Metric::new(name, MetricKind::Absolute, value)
            .with_namespace(self.namespace.clone())
            .with_tags(Some(self.tags.clone()))
            .with_timestamp(Some(Utc::now()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<NginxMetricsConfig>();
    }
}

#[cfg(all(test, feature = "nginx-integration-tests"))]
mod integration_tests {
    use super::*;
    use crate::{
        config::ProxyConfig,
        test_util::components::{run_and_assert_source_compliance_advanced, HTTP_PULL_SOURCE_TAGS},
    };
    use tokio::time::Duration;

    fn nginx_proxy_address() -> String {
        std::env::var("NGINX_PROXY_ADDRESS").unwrap_or_else(|_| "http://nginx-proxy:8000".into())
    }

    fn nginx_address() -> String {
        std::env::var("NGINX_ADDRESS").unwrap_or_else(|_| "http://localhost:8000".into())
    }

    fn squid_address() -> String {
        std::env::var("SQUID_ADDRESS").unwrap_or_else(|_| "http://localhost:3128".into())
    }

    async fn test_nginx(endpoint: String, auth: Option<Auth>, proxy: ProxyConfig) {
        let config = NginxMetricsConfig {
            endpoints: vec![endpoint],
            scrape_interval_secs: Duration::from_secs(15),
            namespace: "vector_nginx".to_owned(),
            tls: None,
            auth,
        };

        let events = run_and_assert_source_compliance_advanced(
            config,
            move |context: &mut SourceContext| {
                context.proxy = proxy;
            },
            Some(Duration::from_secs(3)),
            None,
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert_eq!(events.len(), 8);
    }

    #[tokio::test]
    async fn test_stub_status() {
        let url = format!("{}/basic_status", nginx_address());
        test_nginx(url, None, ProxyConfig::default()).await
    }

    #[tokio::test]
    async fn test_stub_status_auth() {
        let url = format!("{}/basic_status_auth", nginx_address());
        test_nginx(
            url,
            Some(Auth::Basic {
                user: "vector".to_owned(),
                password: "vector".to_owned().into(),
            }),
            ProxyConfig::default(),
        )
        .await
    }

    // This integration test verifies that proxy support is wired up correctly in Vector
    // It is the only test of its kind
    #[tokio::test]
    async fn test_stub_status_with_proxy() {
        let url = format!("{}/basic_status", nginx_proxy_address());
        test_nginx(
            url,
            None,
            ProxyConfig {
                http: Some(squid_address()),
                ..Default::default()
            },
        )
        .await
    }
}

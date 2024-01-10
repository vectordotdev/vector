use std::{future::ready, time::Duration};

use chrono::Utc;
use futures::{stream, FutureExt, StreamExt, TryFutureExt};
use http::uri::Scheme;
use hyper::{Body, Request};
use serde_with::serde_as;
use snafu::ResultExt;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::configurable::configurable_component;
use vector_lib::{metric_tags, EstimatedJsonEncodedSizeOf};

use crate::{
    config::{GenerateConfig, ProxyConfig, SourceConfig, SourceContext, SourceOutput},
    event::metric::{Metric, MetricKind, MetricValue},
    http::HttpClient,
    internal_events::{
        ApacheMetricsEventsReceived, ApacheMetricsParseError, EndpointBytesReceived,
        HttpClientHttpError, HttpClientHttpResponseError, StreamClosedError,
    },
    shutdown::ShutdownSignal,
    SourceSender,
};

mod parser;

pub use parser::ParseError;
use vector_lib::config::LogNamespace;

/// Configuration for the `apache_metrics` source.
#[serde_as]
#[configurable_component(source("apache_metrics", "Collect metrics from Apache's HTTPD server."))]
#[derive(Clone, Debug)]
pub struct ApacheMetricsConfig {
    /// The list of `mod_status` endpoints to scrape metrics from.
    #[configurable(metadata(docs::examples = "http://localhost:8080/server-status/?auto"))]
    endpoints: Vec<String>,

    /// The interval between scrapes.
    #[serde(default = "default_scrape_interval_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    scrape_interval_secs: Duration,

    /// The namespace of the metric.
    ///
    /// Disabled if empty.
    #[serde(default = "default_namespace")]
    namespace: String,
}

pub const fn default_scrape_interval_secs() -> Duration {
    Duration::from_secs(15)
}

pub fn default_namespace() -> String {
    "apache".to_string()
}

impl GenerateConfig for ApacheMetricsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            endpoints: vec!["http://localhost:8080/server-status/?auto".to_owned()],
            scrape_interval_secs: default_scrape_interval_secs(),
            namespace: default_namespace(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "apache_metrics")]
impl SourceConfig for ApacheMetricsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let urls = self
            .endpoints
            .iter()
            .map(|endpoint| endpoint.parse::<http::Uri>())
            .collect::<Result<Vec<_>, _>>()
            .context(super::UriParseSnafu)?;

        let namespace = Some(self.namespace.clone()).filter(|namespace| !namespace.is_empty());

        Ok(apache_metrics(
            urls,
            self.scrape_interval_secs,
            namespace,
            cx.shutdown,
            cx.out,
            cx.proxy,
        ))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

trait UriExt {
    fn to_sanitized_string(&self) -> String;

    fn sanitized_authority(&self) -> String;
}

impl UriExt for http::Uri {
    fn to_sanitized_string(&self) -> String {
        let mut s = String::new();

        if let Some(scheme) = self.scheme() {
            s.push_str(scheme.as_str());
            s.push_str("://");
        }

        s.push_str(&self.sanitized_authority());

        s.push_str(self.path());

        if let Some(query) = self.query() {
            s.push_str(query);
        }

        s
    }

    fn sanitized_authority(&self) -> String {
        let mut s = String::new();

        if let Some(host) = self.host() {
            s.push_str(host);
        }

        if let Some(port) = self.port() {
            s.push(':');
            s.push_str(port.as_str());
        }

        s
    }
}

fn apache_metrics(
    urls: Vec<http::Uri>,
    interval: Duration,
    namespace: Option<String>,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
    proxy: ProxyConfig,
) -> super::Source {
    Box::pin(async move {
        let mut stream = IntervalStream::new(tokio::time::interval(interval))
            .take_until(shutdown)
            .map(move |_| stream::iter(urls.clone()))
            .flatten()
            .map(move |url| {
                let client = HttpClient::new(None, &proxy).expect("HTTPS initialization failed");
                let sanitized_url = url.to_sanitized_string();

                let request = Request::get(&url)
                    .body(Body::empty())
                    .expect("error creating request");

                let tags = metric_tags! {
                    "endpoint" => sanitized_url.to_string(),
                    "host" => url.sanitized_authority(),
                };

                let namespace = namespace.clone();
                client
                    .send(request)
                    .map_err(crate::Error::from)
                    .and_then(|response| async {
                        let (header, body) = response.into_parts();
                        let body = hyper::body::to_bytes(body).await?;
                        Ok((header, body))
                    })
                    .into_stream()
                    .filter_map(move |response| {
                        ready(match response {
                            Ok((header, body)) if header.status == hyper::StatusCode::OK => {
                                let byte_size = body.len();
                                let body = String::from_utf8_lossy(&body);
                                emit!(EndpointBytesReceived {
                                    byte_size,
                                    protocol: url.scheme().unwrap_or(&Scheme::HTTP).as_str(),
                                    endpoint: &sanitized_url,
                                });

                                let results = parser::parse(
                                    &body,
                                    namespace.as_deref(),
                                    Utc::now(),
                                    Some(&tags),
                                )
                                .chain(vec![Ok(Metric::new(
                                    "up",
                                    MetricKind::Absolute,
                                    MetricValue::Gauge { value: 1.0 },
                                )
                                .with_namespace(namespace.clone())
                                .with_tags(Some(tags.clone()))
                                .with_timestamp(Some(Utc::now())))]);

                                let metrics = results
                                    .filter_map(|res| match res {
                                        Ok(metric) => Some(metric),
                                        Err(e) => {
                                            emit!(ApacheMetricsParseError {
                                                error: e,
                                                endpoint: &sanitized_url,
                                            });
                                            None
                                        }
                                    })
                                    .collect::<Vec<_>>();

                                emit!(ApacheMetricsEventsReceived {
                                    byte_size: metrics.estimated_json_encoded_size_of(),
                                    count: metrics.len(),
                                    endpoint: &sanitized_url,
                                });
                                Some(stream::iter(metrics))
                            }
                            Ok((header, _)) => {
                                emit!(HttpClientHttpResponseError {
                                    code: header.status,
                                    url: sanitized_url.to_owned(),
                                });
                                Some(stream::iter(vec![Metric::new(
                                    "up",
                                    MetricKind::Absolute,
                                    MetricValue::Gauge { value: 1.0 },
                                )
                                .with_namespace(namespace.clone())
                                .with_tags(Some(tags.clone()))
                                .with_timestamp(Some(Utc::now()))]))
                            }
                            Err(error) => {
                                emit!(HttpClientHttpError {
                                    error,
                                    url: sanitized_url.to_owned(),
                                });
                                Some(stream::iter(vec![Metric::new(
                                    "up",
                                    MetricKind::Absolute,
                                    MetricValue::Gauge { value: 0.0 },
                                )
                                .with_namespace(namespace.clone())
                                .with_tags(Some(tags.clone()))
                                .with_timestamp(Some(Utc::now()))]))
                            }
                        })
                    })
                    .flatten()
            })
            .flatten()
            .boxed();

        match out.send_event_stream(&mut stream).await {
            Ok(()) => {
                debug!("Finished sending.");
                Ok(())
            }
            Err(_) => {
                let (count, _) = stream.size_hint();
                emit!(StreamClosedError { count });
                Err(())
            }
        }
    })
}

#[cfg(test)]
mod test {
    use hyper::{
        service::{make_service_fn, service_fn},
        Body, Response, Server,
    };
    use similar_asserts::assert_eq;
    use tokio::time::{sleep, Duration};

    use super::*;
    use crate::{
        config::SourceConfig,
        test_util::{
            collect_ready,
            components::{run_and_assert_source_compliance, HTTP_PULL_SOURCE_TAGS},
            next_addr, wait_for_tcp,
        },
        Error,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ApacheMetricsConfig>();
    }

    #[tokio::test]
    async fn test_apache_up() {
        let in_addr = next_addr();

        let make_svc = make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(|_| async {
                Ok::<_, Error>(Response::new(Body::from(
                    r##"
localhost
ServerVersion: Apache/2.4.46 (Unix)
ServerMPM: event
Server Built: Aug  5 2020 23:20:17
CurrentTime: Friday, 21-Aug-2020 18:41:34 UTC
RestartTime: Friday, 21-Aug-2020 18:41:08 UTC
ParentServerConfigGeneration: 1
ParentServerMPMGeneration: 0
ServerUptimeSeconds: 26
ServerUptime: 26 seconds
Load1: 0.00
Load5: 0.03
Load15: 0.03
Total Accesses: 30
Total kBytes: 217
Total Duration: 11
CPUUser: .2
CPUSystem: .02
CPUChildrenUser: 0
CPUChildrenSystem: 0
CPULoad: .846154
Uptime: 26
ReqPerSec: 1.15385
BytesPerSec: 8546.46
BytesPerReq: 7406.93
DurationPerReq: .366667
BusyWorkers: 1
IdleWorkers: 74
Processes: 3
Stopping: 0
BusyWorkers: 1
IdleWorkers: 74
ConnsTotal: 1
ConnsAsyncWriting: 0
ConnsAsyncKeepAlive: 0
ConnsAsyncClosing: 0
Scoreboard: ____S_____I______R____I_______KK___D__C__G_L____________W__________________.....................................................................................................................................................................................................................................................................................................................................
                    "##,
                )))
            }))
        });

        tokio::spawn(async move {
            if let Err(error) = Server::bind(&in_addr).serve(make_svc).await {
                error!(message = "Server error.", %error);
            }
        });
        wait_for_tcp(in_addr).await;

        let config = ApacheMetricsConfig {
            endpoints: vec![format!("http://foo:bar@{}/metrics", in_addr)],
            scrape_interval_secs: Duration::from_secs(1),
            namespace: "custom".to_string(),
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(1),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        let metrics = events
            .into_iter()
            .map(|e| e.into_metric())
            .collect::<Vec<_>>();

        match metrics.iter().find(|m| m.name() == "up") {
            Some(m) => {
                assert_eq!(m.value(), &MetricValue::Gauge { value: 1.0 });

                match m.tags() {
                    Some(tags) => {
                        assert_eq!(
                            tags.get("endpoint"),
                            Some(&format!("http://{}/metrics", in_addr)[..])
                        );
                        assert_eq!(tags.get("host"), Some(&in_addr.to_string()[..]));
                    }
                    None => error!(message = "No tags for metric.", metric = ?m),
                }
            }
            None => error!(message = "Could not find up metric in.", metrics = ?metrics),
        }
    }

    #[tokio::test]
    async fn test_apache_error() {
        let in_addr = next_addr();

        let make_svc = make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(|_| async {
                Ok::<_, Error>(
                    Response::builder()
                        .status(404)
                        .body(Body::from("not found"))
                        .unwrap(),
                )
            }))
        });

        tokio::spawn(async move {
            if let Err(error) = Server::bind(&in_addr).serve(make_svc).await {
                error!(message = "Server error.", %error);
            }
        });
        wait_for_tcp(in_addr).await;

        let (tx, rx) = SourceSender::new_test();

        let source = ApacheMetricsConfig {
            endpoints: vec![format!("http://{}", in_addr)],
            scrape_interval_secs: Duration::from_secs(1),
            namespace: "apache".to_string(),
        }
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
        tokio::spawn(source);

        sleep(Duration::from_secs(1)).await;

        let metrics = collect_ready(rx)
            .await
            .into_iter()
            .map(|e| e.into_metric())
            .collect::<Vec<_>>();

        // we still publish `up=1` for bad status codes following the pattern of the Prometheus exporter:
        //
        // https://github.com/Lusitaniae/apache_exporter/blob/712a6796fb84f741ef3cd562dc11418f2ee8b741/apache_exporter.go#L200
        match metrics.iter().find(|m| m.name() == "up") {
            Some(m) => assert_eq!(m.value(), &MetricValue::Gauge { value: 1.0 }),
            None => error!(message = "Could not find up metric in.", metrics = ?metrics),
        }
    }

    #[tokio::test]
    async fn test_apache_down() {
        // will have nothing bound
        let in_addr = next_addr();

        let (tx, rx) = SourceSender::new_test();

        let source = ApacheMetricsConfig {
            endpoints: vec![format!("http://{}", in_addr)],
            scrape_interval_secs: Duration::from_secs(1),
            namespace: "custom".to_string(),
        }
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
        tokio::spawn(source);

        sleep(Duration::from_secs(1)).await;

        let metrics = collect_ready(rx)
            .await
            .into_iter()
            .map(|e| e.into_metric())
            .collect::<Vec<_>>();

        match metrics.iter().find(|m| m.name() == "up") {
            Some(m) => assert_eq!(m.value(), &MetricValue::Gauge { value: 0.0 }),
            None => error!(message = "Could not find up metric in.", metrics = ?metrics),
        }
    }
}

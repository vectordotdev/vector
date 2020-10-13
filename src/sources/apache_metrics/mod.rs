use crate::{
    config::{self, GenerateConfig, GlobalOptions, SourceConfig, SourceDescription},
    event::metric::{Metric, MetricKind, MetricValue},
    internal_events::{
        ApacheMetricsErrorResponse, ApacheMetricsEventReceived, ApacheMetricsHttpError,
        ApacheMetricsParseError, ApacheMetricsRequestCompleted,
    },
    shutdown::ShutdownSignal,
    Event, Pipeline,
};
use chrono::Utc;
use futures::{
    compat::{Future01CompatExt, Sink01CompatExt},
    future, stream, FutureExt, StreamExt, TryFutureExt,
};
use futures01::Sink;
use hyper::{Body, Client, Request};
use hyper_openssl::HttpsConnector;
use parser::encode_namespace;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

mod parser;

pub use parser::ParseError;

#[derive(Deserialize, Serialize, Clone, Debug)]
struct ApacheMetricsConfig {
    endpoints: Vec<String>,
    #[serde(default = "default_scrape_interval_secs")]
    scrape_interval_secs: u64,
    #[serde(default = "default_namespace")]
    namespace: String,
}

pub fn default_scrape_interval_secs() -> u64 {
    15
}

pub fn default_namespace() -> String {
    "apache".to_string()
}

inventory::submit! {
    SourceDescription::new::<ApacheMetricsConfig>("apache_metrics")
}

impl GenerateConfig for ApacheMetricsConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "apache_metrics")]
impl SourceConfig for ApacheMetricsConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        let urls = self
            .endpoints
            .iter()
            .map(|endpoint| endpoint.parse::<http::Uri>())
            .collect::<Result<Vec<_>, _>>()
            .context(super::UriParseError)?;

        Ok(apache_metrics(
            urls,
            self.scrape_interval_secs,
            self.namespace.clone(),
            shutdown,
            out,
        ))
    }

    fn output_type(&self) -> crate::config::DataType {
        config::DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "apache_metrics"
    }
}

fn apache_metrics(
    urls: Vec<http::Uri>,
    interval: u64,
    namespace: String,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> super::Source {
    let out = out
        .sink_map_err(|e| error!("error sending metric: {:?}", e))
        .sink_compat();
    let task = tokio::time::interval(Duration::from_secs(interval))
        .take_until(shutdown.compat())
        .map(move |_| stream::iter(urls.clone()))
        .flatten()
        .map(move |url| {
            let https = HttpsConnector::new().expect("TLS initialization failed");
            let client = Client::builder().build(https);

            let request = Request::get(&url)
                .body(Body::empty())
                .expect("error creating request");

            let mut tags: BTreeMap<String, String> = BTreeMap::new();
            tags.insert("endpoint".into(), url.to_string());
            if let Some(host) = url.host() {
                tags.insert("host".into(), host.into());
            }

            let start = Instant::now();
            let namespace = namespace.clone();
            client
                .request(request)
                .and_then(|response| async {
                    let (header, body) = response.into_parts();
                    let body = hyper::body::to_bytes(body).await?;
                    Ok((header, body))
                })
                .into_stream()
                .filter_map(move |response| {
                    future::ready(match response {
                        Ok((header, body)) if header.status == hyper::StatusCode::OK => {
                            emit!(ApacheMetricsRequestCompleted {
                                start,
                                end: Instant::now()
                            });

                            let byte_size = body.len();
                            let body = String::from_utf8_lossy(&body);

                            let results = parser::parse(&body, &namespace, Utc::now(), Some(&tags))
                                .chain(vec![Ok(Metric {
                                    name: encode_namespace(&namespace, "up"),
                                    timestamp: Some(Utc::now()),
                                    tags: Some(tags.clone()),
                                    kind: MetricKind::Absolute,
                                    value: MetricValue::Gauge { value: 1.0 },
                                })]);

                            let metrics = results
                                .filter_map(|res| match res {
                                    Ok(metric) => Some(metric),
                                    Err(e) => {
                                        emit!(ApacheMetricsParseError {
                                            error: e,
                                            url: url.clone(),
                                        });
                                        None
                                    }
                                })
                                .collect::<Vec<_>>();

                            emit!(ApacheMetricsEventReceived {
                                byte_size,
                                count: metrics.len(),
                            });
                            Some(stream::iter(metrics).map(Event::Metric).map(Ok))
                        }
                        Ok((header, _)) => {
                            emit!(ApacheMetricsErrorResponse {
                                code: header.status,
                                url: url.clone(),
                            });
                            Some(
                                stream::iter(vec![Metric {
                                    name: encode_namespace(&namespace, "up"),
                                    timestamp: Some(Utc::now()),
                                    tags: Some(tags.clone()),
                                    kind: MetricKind::Absolute,
                                    value: MetricValue::Gauge { value: 1.0 },
                                }])
                                .map(Event::Metric)
                                .map(Ok),
                            )
                        }
                        Err(error) => {
                            emit!(ApacheMetricsHttpError {
                                error,
                                url: url.clone()
                            });
                            Some(
                                stream::iter(vec![Metric {
                                    name: encode_namespace(&namespace, "up"),
                                    timestamp: Some(Utc::now()),
                                    tags: Some(tags.clone()),
                                    kind: MetricKind::Absolute,
                                    value: MetricValue::Gauge { value: 0.0 },
                                }])
                                .map(Event::Metric)
                                .map(Ok),
                            )
                        }
                    })
                })
                .flatten()
        })
        .flatten()
        .forward(out)
        .inspect(|_| info!("finished sending"));

    Box::new(task.boxed().compat())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        config::{GlobalOptions, SourceConfig},
        test_util::{collect_ready, next_addr, wait_for_tcp},
        Error,
    };
    use futures::compat::Future01CompatExt;
    use hyper::{
        service::{make_service_fn, service_fn},
        {Body, Response, Server},
    };
    use pretty_assertions::assert_eq;
    use tokio::time::{delay_for, Duration};

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
            if let Err(e) = Server::bind(&in_addr).serve(make_svc).await {
                error!("server error: {:?}", e);
            }
        });
        wait_for_tcp(in_addr).await;

        let (tx, rx) = Pipeline::new_test();

        let source = ApacheMetricsConfig {
            endpoints: vec![format!("http://{}", in_addr)],
            scrape_interval_secs: 1,
            namespace: "custom".to_string(),
        }
        .build(
            "default",
            &GlobalOptions::default(),
            ShutdownSignal::noop(),
            tx,
        )
        .await
        .unwrap()
        .compat();
        tokio::spawn(source);

        delay_for(Duration::from_secs(1)).await;

        let metrics = collect_ready(rx)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.into_metric())
            .collect::<Vec<_>>();

        match metrics.iter().find(|m| m.name == "custom_up") {
            Some(m) => assert_eq!(m.value, MetricValue::Gauge { value: 1.0 }),
            None => error!("could not find apache_up metric in {:?}", metrics),
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
            if let Err(e) = Server::bind(&in_addr).serve(make_svc).await {
                error!("server error: {:?}", e);
            }
        });
        wait_for_tcp(in_addr).await;

        let (tx, rx) = Pipeline::new_test();

        let source = ApacheMetricsConfig {
            endpoints: vec![format!("http://{}", in_addr)],
            scrape_interval_secs: 1,
            namespace: "apache".to_string(),
        }
        .build(
            "default",
            &GlobalOptions::default(),
            ShutdownSignal::noop(),
            tx,
        )
        .await
        .unwrap()
        .compat();
        tokio::spawn(source);

        delay_for(Duration::from_secs(1)).await;

        let metrics = collect_ready(rx)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.into_metric())
            .collect::<Vec<_>>();

        // we still publish `apache_up=1` for bad status codes following the pattern of the Prometheus exporter:
        //
        // https://github.com/Lusitaniae/apache_exporter/blob/712a6796fb84f741ef3cd562dc11418f2ee8b741/apache_exporter.go#L200
        match metrics.iter().find(|m| m.name == "apache_up") {
            Some(m) => assert_eq!(m.value, MetricValue::Gauge { value: 1.0 }),
            None => error!("could not find apache_up metric in {:?}", metrics),
        }
    }

    #[tokio::test]
    async fn test_apache_down() {
        // will have nothing bound
        let in_addr = next_addr();

        let (tx, rx) = Pipeline::new_test();

        let source = ApacheMetricsConfig {
            endpoints: vec![format!("http://{}", in_addr)],
            scrape_interval_secs: 1,
            namespace: "custom".to_string(),
        }
        .build(
            "default",
            &GlobalOptions::default(),
            ShutdownSignal::noop(),
            tx,
        )
        .await
        .unwrap()
        .compat();
        tokio::spawn(source);

        delay_for(Duration::from_secs(1)).await;

        let metrics = collect_ready(rx)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.into_metric())
            .collect::<Vec<_>>();

        match metrics.iter().find(|m| m.name == "custom_up") {
            Some(m) => assert_eq!(m.value, MetricValue::Gauge { value: 0.0 }),
            None => error!("could not find apache_up metric in {:?}", metrics),
        }
    }
}

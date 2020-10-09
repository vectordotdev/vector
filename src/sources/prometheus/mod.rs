use crate::{
    config::{self, GenerateConfig, GlobalOptions, SourceConfig, SourceDescription},
    internal_events::{
        PrometheusErrorResponse, PrometheusEventReceived, PrometheusHttpError,
        PrometheusParseError, PrometheusRequestCompleted,
    },
    shutdown::ShutdownSignal,
    Event, Pipeline,
};
use futures::{
    compat::{Future01CompatExt, Sink01CompatExt},
    future, stream, FutureExt, StreamExt, TryFutureExt,
};
use futures01::Sink;
use hyper::{Body, Client, Request};
use hyper_openssl::HttpsConnector;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::time::{Duration, Instant};

pub mod parser;

#[derive(Deserialize, Serialize, Clone, Debug)]
struct PrometheusConfig {
    // Deprecated name
    #[serde(alias = "hosts")]
    endpoints: Vec<String>,
    #[serde(default = "default_scrape_interval_secs")]
    scrape_interval_secs: u64,
}

pub fn default_scrape_interval_secs() -> u64 {
    15
}

inventory::submit! {
    SourceDescription::new::<PrometheusConfig>("prometheus")
}

impl GenerateConfig for PrometheusConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus")]
impl SourceConfig for PrometheusConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        let mut urls = Vec::new();
        for host in self.endpoints.iter() {
            let base_uri = host.parse::<http::Uri>().context(super::UriParseError)?;
            urls.push(format!("{}metrics", base_uri));
        }
        Ok(prometheus(urls, self.scrape_interval_secs, shutdown, out))
    }

    fn output_type(&self) -> crate::config::DataType {
        config::DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "prometheus"
    }
}

fn prometheus(
    urls: Vec<String>,
    interval: u64,
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

            let start = Instant::now();
            client
                .request(request)
                .and_then(|response| async move {
                    let (header, body) = response.into_parts();
                    let body = hyper::body::to_bytes(body).await?;
                    Ok((header, body))
                })
                .into_stream()
                .filter_map(move |response| {
                    future::ready(match response {
                        Ok((header, body)) if header.status == hyper::StatusCode::OK => {
                            emit!(PrometheusRequestCompleted {
                                start,
                                end: Instant::now()
                            });

                            let byte_size = body.len();
                            let body = String::from_utf8_lossy(&body);

                            match parser::parse(&body) {
                                Ok(metrics) => {
                                    emit!(PrometheusEventReceived {
                                        byte_size,
                                        count: metrics.len(),
                                    });
                                    Some(stream::iter(metrics).map(Event::Metric).map(Ok))
                                }
                                Err(error) => {
                                    emit!(PrometheusParseError {
                                        error,
                                        url: url.clone(),
                                        body,
                                    });
                                    None
                                }
                            }
                        }
                        Ok((header, _)) => {
                            emit!(PrometheusErrorResponse {
                                code: header.status,
                                url: url.clone(),
                            });
                            None
                        }
                        Err(error) => {
                            emit!(PrometheusHttpError {
                                error,
                                url: url.clone(),
                            });
                            None
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

#[cfg(feature = "sinks-prometheus")]
#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        config,
        sinks::prometheus::PrometheusSinkConfig,
        test_util::{next_addr, start_topology},
        Error,
    };
    use futures::compat::Future01CompatExt;
    use hyper::{
        service::{make_service_fn, service_fn},
        {Body, Client, Response, Server},
    };
    use pretty_assertions::assert_eq;
    use tokio::time::{delay_for, Duration};

    #[tokio::test]
    async fn test_prometheus_routing() {
        let in_addr = next_addr();
        let out_addr = next_addr();

        let make_svc = make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(|_| async {
                Ok::<_, Error>(Response::new(Body::from(
                    r##"
                    # HELP promhttp_metric_handler_requests_total Total number of scrapes by HTTP status code.
                    # TYPE promhttp_metric_handler_requests_total counter
                    promhttp_metric_handler_requests_total{code="200"} 100
                    promhttp_metric_handler_requests_total{code="404"} 7
                    prometheus_remote_storage_samples_in_total 57011636
                    # A histogram, which has a pretty complex representation in the text format:
                    # HELP http_request_duration_seconds A histogram of the request duration.
                    # TYPE http_request_duration_seconds histogram
                    http_request_duration_seconds_bucket{le="0.05"} 24054
                    http_request_duration_seconds_bucket{le="0.1"} 33444
                    http_request_duration_seconds_bucket{le="0.2"} 100392
                    http_request_duration_seconds_bucket{le="0.5"} 129389
                    http_request_duration_seconds_bucket{le="1"} 133988
                    http_request_duration_seconds_bucket{le="+Inf"} 144320
                    http_request_duration_seconds_sum 53423
                    http_request_duration_seconds_count 144320
                    # Finally a summary, which has a complex representation, too:
                    # HELP rpc_duration_seconds A summary of the RPC duration in seconds.
                    # TYPE rpc_duration_seconds summary
                    rpc_duration_seconds{code="200",quantile="0.01"} 3102
                    rpc_duration_seconds{code="200",quantile="0.05"} 3272
                    rpc_duration_seconds{code="200",quantile="0.5"} 4773
                    rpc_duration_seconds{code="200",quantile="0.9"} 9001
                    rpc_duration_seconds{code="200",quantile="0.99"} 76656
                    rpc_duration_seconds_sum{code="200"} 1.7560473e+07
                    rpc_duration_seconds_count{code="200"} 2693
                    "##,
                )))
            }))
        });

        tokio::spawn(async move {
            if let Err(e) = Server::bind(&in_addr).serve(make_svc).await {
                error!("server error: {:?}", e);
            }
        });

        let mut config = config::Config::builder();
        config.add_source(
            "in",
            PrometheusConfig {
                endpoints: vec![format!("http://{}", in_addr)],
                scrape_interval_secs: 1,
            },
        );
        config.add_sink(
            "out",
            &["in"],
            PrometheusSinkConfig {
                address: out_addr,
                namespace: Some("vector".into()),
                buckets: vec![1.0, 2.0, 4.0],
                flush_period_secs: 1,
            },
        );

        let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
        delay_for(Duration::from_secs(1)).await;

        let response = Client::new()
            .get(format!("http://{}/metrics", out_addr).parse().unwrap())
            .await
            .unwrap();
        assert!(response.status().is_success());

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let lines = std::str::from_utf8(&body)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();

        assert_eq!(lines, vec![
            "# HELP vector_promhttp_metric_handler_requests_total promhttp_metric_handler_requests_total",
            "# TYPE vector_promhttp_metric_handler_requests_total counter",
            "vector_promhttp_metric_handler_requests_total{code=\"200\"} 100",
            "vector_promhttp_metric_handler_requests_total{code=\"404\"} 7",
            "# HELP vector_prometheus_remote_storage_samples_in_total prometheus_remote_storage_samples_in_total",
            "# TYPE vector_prometheus_remote_storage_samples_in_total gauge",
            "vector_prometheus_remote_storage_samples_in_total 57011636",
            "# HELP vector_http_request_duration_seconds http_request_duration_seconds",
            "# TYPE vector_http_request_duration_seconds histogram",
            "vector_http_request_duration_seconds_bucket{le=\"0.05\"} 24054",
            "vector_http_request_duration_seconds_bucket{le=\"0.1\"} 33444",
            "vector_http_request_duration_seconds_bucket{le=\"0.2\"} 100392",
            "vector_http_request_duration_seconds_bucket{le=\"0.5\"} 129389",
            "vector_http_request_duration_seconds_bucket{le=\"1\"} 133988",
            "vector_http_request_duration_seconds_bucket{le=\"+Inf\"} 144320",
            "vector_http_request_duration_seconds_sum 53423",
            "vector_http_request_duration_seconds_count 144320",
            "# HELP vector_rpc_duration_seconds rpc_duration_seconds",
            "# TYPE vector_rpc_duration_seconds summary",
            "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.01\"} 3102",
            "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.05\"} 3272",
            "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.5\"} 4773",
            "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.9\"} 9001",
            "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.99\"} 76656",
            "vector_rpc_duration_seconds_sum{code=\"200\"} 17560473",
            "vector_rpc_duration_seconds_count{code=\"200\"} 2693",
            ],
        );

        topology.stop().compat().await.unwrap();
    }
}

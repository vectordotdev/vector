use crate::{topology::config::GlobalOptions, Event};
use futures::{sync::mpsc, Future, Sink, Stream};
use http::Uri;
use hyper;
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::timer::Interval;

pub mod parser;

#[derive(Deserialize, Serialize, Clone, Debug)]
struct PrometheusConfig {
    host: String,
    #[serde(default = "default_scrape_interval_secs")]
    scrape_interval_secs: u64,
}

pub fn default_scrape_interval_secs() -> u64 {
    15
}

#[typetag::serde(name = "prometheus")]
impl crate::topology::config::SourceConfig for PrometheusConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        Ok(prometheus(self.clone(), out))
    }

    fn output_type(&self) -> crate::topology::config::DataType {
        crate::topology::config::DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "prometheus"
    }
}

fn prometheus(config: PrometheusConfig, out: mpsc::Sender<Event>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending metric: {:?}", e));

    let task = Interval::new(
        Instant::now(),
        Duration::from_secs(config.scrape_interval_secs),
    )
    .map_err(|e| error!("timer error: {:?}", e))
    .map(move |_| {
        let uri = format!("{}/metrics", config.host).parse::<Uri>().unwrap();
        let request = hyper::Request::get(uri).body(hyper::Body::empty()).unwrap();

        let https = HttpsConnector::new(4).expect("TLS initialization failed");
        let client = hyper::Client::builder().build(https);

        client
            .request(request)
            .and_then(|response| response.into_body().concat2())
            .map(|body| {
                let packet = String::from_utf8_lossy(&body);
                let metrics = parser::parse(&packet)
                    .map_err(|e| error!("parsing error: {:?}", e))
                    .unwrap_or_default()
                    .into_iter()
                    .map(Event::Metric);
                futures::stream::iter_ok(metrics)
            })
            .flatten_stream()
            .map_err(|e| error!("http request processing error: {:?}", e))
    })
    .flatten()
    .forward(out)
    .map(|_| info!("finished sending"));

    Box::new(task)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        sinks::prometheus::PrometheusSinkConfig,
        test_util::{block_on, next_addr, runtime},
        topology::{self, config},
    };
    use hyper::service::{make_service_fn, service_fn_ok};
    use hyper::{Body, Response, Server};
    use pretty_assertions::assert_eq;
    use std::{thread, time::Duration};

    #[test]
    fn test_prometheus_routing() {
        let mut rt = runtime();
        let in_addr = next_addr();
        let out_addr = next_addr();

        let make_svc = make_service_fn(|_| {
            service_fn_ok(move |_| {
                Response::new(Body::from(
                    r##"
                    # HELP promhttp_metric_handler_requests_total Total number of scrapes by HTTP status code.
                    # TYPE promhttp_metric_handler_requests_total counter
                    promhttp_metric_handler_requests_total{code="200"} 100
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
                    "##
                ))
            })
        });

        let server = Server::bind(&in_addr).serve(make_svc);
        rt.spawn(server.map_err(|e| {
            error!("server error: {:?}", e);
        }));

        let mut config = config::Config::empty();
        config.add_source(
            "in",
            PrometheusConfig {
                host: format!("http://{}", in_addr),
                scrape_interval_secs: 1,
            },
        );
        config.add_sink(
            "out",
            &["in"],
            PrometheusSinkConfig {
                address: out_addr,
                namespace: "vector".into(),
                buckets: vec![1.0, 2.0, 4.0],
                flush_period_sec: 0,
            },
        );

        let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
        thread::sleep(Duration::from_secs(1));

        let client = hyper::Client::new();
        let response =
            block_on(client.get(format!("http://{}/metrics", out_addr).parse().unwrap())).unwrap();
        assert!(response.status().is_success());

        let body = block_on(response.into_body().concat2()).unwrap();
        let lines = std::str::from_utf8(&body)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();

        assert_eq!(lines, vec![
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
            "# HELP vector_prometheus_remote_storage_samples_in_total prometheus_remote_storage_samples_in_total",
            "# TYPE vector_prometheus_remote_storage_samples_in_total gauge",
            "vector_prometheus_remote_storage_samples_in_total 57011636",
            "# HELP vector_promhttp_metric_handler_requests_total promhttp_metric_handler_requests_total",
            "# TYPE vector_promhttp_metric_handler_requests_total counter",
            "vector_promhttp_metric_handler_requests_total{code=\"200\"} 100",
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

        block_on(topology.stop()).unwrap();
    }
}

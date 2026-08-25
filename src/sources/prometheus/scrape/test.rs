use http_body::Body as _;
use hyper::{
    Body, Client, Response, Server,
    service::{make_service_fn, service_fn},
};
use similar_asserts::assert_eq;
use tokio::time::{Duration, sleep};
use warp::Filter;

use super::*;
use crate::{
    Error, config,
    http::{ParameterValue, QueryParameterValue},
    sinks::prometheus::exporter::PrometheusExporterConfig,
    test_util::{
        addr::next_addr,
        components::{HTTP_PULL_SOURCE_TAGS, run_and_assert_source_compliance},
        start_topology, trace_init, wait_for_tcp,
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<PrometheusScrapeConfig>();
}

#[tokio::test]
async fn test_prometheus_sets_headers() {
    let (_guard, in_addr) = next_addr();

    let dummy_endpoint = warp::path!("metrics").and(warp::header::exact("Accept", "text/plain")).map(|| {
        r#"
                promhttp_metric_handler_requests_total{endpoint="http://example.com", instance="localhost:9999", code="200"} 100 1612411516789
                "#
    });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    let config = PrometheusScrapeConfig {
        endpoints: vec![format!("http://{}/metrics", in_addr)],
        interval: Duration::from_secs(1),
        timeout: default_timeout(),
        instance_tag: Some("instance".to_string()),
        endpoint_tag: Some("endpoint".to_string()),
        honor_labels: true,
        query: HashMap::new(),
        auth: None,
        tls: None,
    };

    let events =
        run_and_assert_source_compliance(config, Duration::from_secs(3), &HTTP_PULL_SOURCE_TAGS)
            .await;
    assert!(!events.is_empty());
}

#[tokio::test]
async fn test_prometheus_honor_labels() {
    let (_guard, in_addr) = next_addr();

    let dummy_endpoint = warp::path!("metrics").map(|| {
            r#"
                promhttp_metric_handler_requests_total{endpoint="http://example.com", instance="localhost:9999", code="200"} 100 1612411516789
                "#
    });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    let config = PrometheusScrapeConfig {
        endpoints: vec![format!("http://{}/metrics", in_addr)],
        interval: Duration::from_secs(1),
        timeout: default_timeout(),
        instance_tag: Some("instance".to_string()),
        endpoint_tag: Some("endpoint".to_string()),
        honor_labels: true,
        query: HashMap::new(),
        auth: None,
        tls: None,
    };

    let events =
        run_and_assert_source_compliance(config, Duration::from_secs(3), &HTTP_PULL_SOURCE_TAGS)
            .await;
    assert!(!events.is_empty());

    let metrics: Vec<_> = events
        .into_iter()
        .map(|event| event.into_metric())
        .collect();

    for metric in metrics {
        assert_eq!(
            metric.tag_value("instance"),
            Some(String::from("localhost:9999"))
        );
        assert_eq!(
            metric.tag_value("endpoint"),
            Some(String::from("http://example.com"))
        );
        assert_eq!(metric.tag_value("exported_instance"), None,);
        assert_eq!(metric.tag_value("exported_endpoint"), None,);
    }
}

#[tokio::test]
async fn test_prometheus_do_not_honor_labels() {
    let (_guard, in_addr) = next_addr();

    let dummy_endpoint = warp::path!("metrics").map(|| {
            r#"
                promhttp_metric_handler_requests_total{endpoint="http://example.com", instance="localhost:9999", code="200"} 100 1612411516789
            "#
    });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    let config = PrometheusScrapeConfig {
        endpoints: vec![format!("http://{}/metrics", in_addr)],
        interval: Duration::from_secs(1),
        timeout: default_timeout(),
        instance_tag: Some("instance".to_string()),
        endpoint_tag: Some("endpoint".to_string()),
        honor_labels: false,
        query: HashMap::new(),
        auth: None,
        tls: None,
    };

    let events =
        run_and_assert_source_compliance(config, Duration::from_secs(3), &HTTP_PULL_SOURCE_TAGS)
            .await;
    assert!(!events.is_empty());

    let metrics: Vec<_> = events
        .into_iter()
        .map(|event| event.into_metric())
        .collect();

    for metric in metrics {
        assert_eq!(
            metric.tag_value("instance"),
            Some(format!("{}:{}", in_addr.ip(), in_addr.port()))
        );
        assert_eq!(
            metric.tag_value("endpoint"),
            Some(format!(
                "http://{}:{}/metrics",
                in_addr.ip(),
                in_addr.port()
            ))
        );
        assert_eq!(
            metric.tag_value("exported_instance"),
            Some(String::from("localhost:9999"))
        );
        assert_eq!(
            metric.tag_value("exported_endpoint"),
            Some(String::from("http://example.com"))
        );
    }
}

/// According to the [spec](https://github.com/OpenObservability/OpenMetrics/blob/main/specification/OpenMetrics.md?plain=1#L115)
/// > Label names MUST be unique within a LabelSet.
/// Prometheus itself will reject the metric with an error. Largely to remain backward compatible with older versions of Vector,
/// we accept the metric, but take the last label in the list.
#[tokio::test]
async fn test_prometheus_duplicate_tags() {
    let (_guard, in_addr) = next_addr();

    let dummy_endpoint = warp::path!("metrics").map(|| {
        r#"
                metric_label{code="200",code="success"} 100 1612411516789
        "#
    });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    let config = PrometheusScrapeConfig {
        endpoints: vec![format!("http://{}/metrics", in_addr)],
        interval: Duration::from_secs(1),
        timeout: default_timeout(),
        instance_tag: Some("instance".to_string()),
        endpoint_tag: Some("endpoint".to_string()),
        honor_labels: true,
        query: HashMap::new(),
        auth: None,
        tls: None,
    };

    let events =
        run_and_assert_source_compliance(config, Duration::from_secs(3), &HTTP_PULL_SOURCE_TAGS)
            .await;
    assert!(!events.is_empty());

    let metrics: Vec<vector_lib::event::Metric> = events
        .into_iter()
        .map(|event| event.into_metric())
        .collect();
    let metric = &metrics[0];

    assert_eq!(metric.name(), "metric_label");

    let code_tag = metric
        .tags()
        .unwrap()
        .iter_all()
        .filter(|(name, _value)| *name == "code")
        .map(|(_name, value)| value)
        .collect::<Vec<_>>();

    assert_eq!(1, code_tag.len());
    assert_eq!("success", code_tag[0].unwrap());
}

#[tokio::test]
async fn test_prometheus_request_query() {
    let (_guard, in_addr) = next_addr();

    let dummy_endpoint = warp::path!("metrics").and(warp::query::raw()).map(|query| {
        format!(
            r#"
                promhttp_metric_handler_requests_total{{query="{query}"}} 100 1612411516789
            "#
        )
    });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    let config = PrometheusScrapeConfig {
        endpoints: vec![format!("http://{}/metrics?key1=val1", in_addr)],
        interval: Duration::from_secs(1),
        timeout: default_timeout(),
        instance_tag: Some("instance".to_string()),
        endpoint_tag: Some("endpoint".to_string()),
        honor_labels: false,
        query: HashMap::from([
            (
                "key1".to_string(),
                QueryParameterValue::MultiParams(vec![ParameterValue::String("val2".to_string())]),
            ),
            (
                "key2".to_string(),
                QueryParameterValue::MultiParams(vec![
                    ParameterValue::String("val1".to_string()),
                    ParameterValue::String("val2".to_string()),
                ]),
            ),
        ]),
        auth: None,
        tls: None,
    };

    let events =
        run_and_assert_source_compliance(config, Duration::from_secs(3), &HTTP_PULL_SOURCE_TAGS)
            .await;
    assert!(!events.is_empty());

    let metrics: Vec<_> = events
        .into_iter()
        .map(|event| event.into_metric())
        .collect();

    let expected = HashMap::from([
        (
            "key1".to_string(),
            vec!["val1".to_string(), "val2".to_string()],
        ),
        (
            "key2".to_string(),
            vec!["val1".to_string(), "val2".to_string()],
        ),
    ]);

    for metric in metrics {
        let query = metric.tag_value("query").expect("query must be tagged");
        let mut got: HashMap<String, Vec<String>> = HashMap::new();
        for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
            got.entry(k.to_string()).or_default().push(v.to_string());
        }
        for v in got.values_mut() {
            v.sort();
        }
        assert_eq!(got, expected);
    }
}

// Intentionally not using assert_source_compliance here because this is a round-trip test which
// means source and sink will both emit `EventsSent` , triggering multi-emission check.
#[tokio::test]
async fn test_prometheus_routing() {
    trace_init();
    let (_in_guard, in_addr) = next_addr();
    let (_out_guard, out_addr) = next_addr();

    let make_svc = make_service_fn(|_| async {
        Ok::<_, Error>(service_fn(|_| async {
            Ok::<_, Error>(Response::new(Body::from(
                r#"
                # HELP promhttp_metric_handler_requests_total Total number of scrapes by HTTP status code.
                # TYPE promhttp_metric_handler_requests_total counter
                promhttp_metric_handler_requests_total{code="200"} 100 1612411516789
                promhttp_metric_handler_requests_total{code="404"} 7 1612411516789
                prometheus_remote_storage_samples_in_total 57011636 1612411516789
                # A histogram, which has a pretty complex representation in the text format:
                # HELP http_request_duration_seconds A histogram of the request duration.
                # TYPE http_request_duration_seconds histogram
                http_request_duration_seconds_bucket{le="0.05"} 24054 1612411516789
                http_request_duration_seconds_bucket{le="0.1"} 33444 1612411516789
                http_request_duration_seconds_bucket{le="0.2"} 100392 1612411516789
                http_request_duration_seconds_bucket{le="0.5"} 129389 1612411516789
                http_request_duration_seconds_bucket{le="1"} 133988 1612411516789
                http_request_duration_seconds_bucket{le="+Inf"} 144320 1612411516789
                http_request_duration_seconds_sum 53423 1612411516789
                http_request_duration_seconds_count 144320 1612411516789
                # Finally a summary, which has a complex representation, too:
                # HELP rpc_duration_seconds A summary of the RPC duration in seconds.
                # TYPE rpc_duration_seconds summary
                rpc_duration_seconds{code="200",quantile="0.01"} 3102 1612411516789
                rpc_duration_seconds{code="200",quantile="0.05"} 3272 1612411516789
                rpc_duration_seconds{code="200",quantile="0.5"} 4773 1612411516789
                rpc_duration_seconds{code="200",quantile="0.9"} 9001 1612411516789
                rpc_duration_seconds{code="200",quantile="0.99"} 76656 1612411516789
                rpc_duration_seconds_sum{code="200"} 1.7560473e+07 1612411516789
                rpc_duration_seconds_count{code="200"} 2693 1612411516789
                "#,
            )))
        }))
    });

    tokio::spawn(async move {
        if let Err(error) = Server::bind(&in_addr).serve(make_svc).await {
            error!(message = "Server error.", %error);
        }
    });
    wait_for_tcp(in_addr).await;

    let mut config = config::Config::builder();
    config.add_source(
        "in",
        PrometheusScrapeConfig {
            endpoints: vec![format!("http://{}", in_addr)],
            instance_tag: None,
            endpoint_tag: None,
            honor_labels: false,
            query: HashMap::new(),
            interval: Duration::from_secs(1),
            timeout: default_timeout(),
            tls: None,
            auth: None,
        },
    );
    config.add_sink(
        "out",
        &["in"],
        PrometheusExporterConfig {
            address: out_addr,
            auth: None,
            tls: None,
            default_namespace: Some("vector".into()),
            buckets: vec![1.0, 2.0, 4.0],
            quantiles: vec![],
            distributions_as_summaries: false,
            flush_period_secs: Duration::from_secs(3),
            suppress_timestamp: false,
            acknowledgements: Default::default(),
        },
    );

    let (topology, _) = start_topology(config.build().unwrap(), false).await;
    sleep(Duration::from_secs(1)).await;

    let response = Client::new()
        .get(format!("http://{out_addr}/metrics").parse().unwrap())
        .await
        .unwrap();

    assert!(response.status().is_success());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let lines = std::str::from_utf8(&body)
        .unwrap()
        .lines()
        .collect::<Vec<_>>();

    assert_eq!(
        lines,
        vec![
            "# HELP vector_http_request_duration_seconds http_request_duration_seconds",
            "# TYPE vector_http_request_duration_seconds histogram",
            "vector_http_request_duration_seconds_bucket{le=\"0.05\"} 24054 1612411516789",
            "vector_http_request_duration_seconds_bucket{le=\"0.1\"} 33444 1612411516789",
            "vector_http_request_duration_seconds_bucket{le=\"0.2\"} 100392 1612411516789",
            "vector_http_request_duration_seconds_bucket{le=\"0.5\"} 129389 1612411516789",
            "vector_http_request_duration_seconds_bucket{le=\"1\"} 133988 1612411516789",
            "vector_http_request_duration_seconds_bucket{le=\"+Inf\"} 144320 1612411516789",
            "vector_http_request_duration_seconds_sum 53423 1612411516789",
            "vector_http_request_duration_seconds_count 144320 1612411516789",
            "# HELP vector_prometheus_remote_storage_samples_in_total prometheus_remote_storage_samples_in_total",
            "# TYPE vector_prometheus_remote_storage_samples_in_total gauge",
            "vector_prometheus_remote_storage_samples_in_total 57011636 1612411516789",
            "# HELP vector_promhttp_metric_handler_requests_total promhttp_metric_handler_requests_total",
            "# TYPE vector_promhttp_metric_handler_requests_total counter",
            "vector_promhttp_metric_handler_requests_total{code=\"200\"} 100 1612411516789",
            "vector_promhttp_metric_handler_requests_total{code=\"404\"} 7 1612411516789",
            "# HELP vector_rpc_duration_seconds rpc_duration_seconds",
            "# TYPE vector_rpc_duration_seconds summary",
            "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.01\"} 3102 1612411516789",
            "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.05\"} 3272 1612411516789",
            "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.5\"} 4773 1612411516789",
            "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.9\"} 9001 1612411516789",
            "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.99\"} 76656 1612411516789",
            "vector_rpc_duration_seconds_sum{code=\"200\"} 17560473 1612411516789",
            "vector_rpc_duration_seconds_count{code=\"200\"} 2693 1612411516789",
        ],
    );

    topology.stop().await;
}

use tokio::time::Duration;

use super::*;
use crate::{
    event::{MetricKind, MetricValue},
    test_util::components::{HTTP_PULL_SOURCE_TAGS, run_and_assert_source_compliance},
};

#[tokio::test]
async fn scrapes_metrics() {
    let config = PrometheusScrapeConfig {
        endpoints: vec!["http://prometheus:9090/metrics".into()],
        interval: Duration::from_secs(1),
        timeout: Duration::from_secs(1),
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

    let find_metric = |name: &str| {
        metrics
            .iter()
            .find(|metric| metric.name() == name)
            .unwrap_or_else(|| panic!("Missing metric {name:?}"))
    };

    // Sample some well-known metrics
    let build = find_metric("prometheus_build_info");
    assert!(matches!(build.kind(), MetricKind::Absolute));
    assert!(matches!(build.value(), &MetricValue::Gauge { .. }));
    assert!(build.tags().unwrap().contains_key("branch"));
    assert!(build.tags().unwrap().contains_key("version"));
    assert_eq!(
        build.tag_value("instance"),
        Some("prometheus:9090".to_string())
    );
    assert_eq!(
        build.tag_value("endpoint"),
        Some("http://prometheus:9090/metrics".to_string())
    );

    let queries = find_metric("prometheus_engine_queries");
    assert!(matches!(queries.kind(), MetricKind::Absolute));
    assert!(matches!(queries.value(), &MetricValue::Gauge { .. }));
    assert_eq!(
        queries.tag_value("instance"),
        Some("prometheus:9090".to_string())
    );
    assert_eq!(
        queries.tag_value("endpoint"),
        Some("http://prometheus:9090/metrics".to_string())
    );

    let go_info = find_metric("go_info");
    assert!(matches!(go_info.kind(), MetricKind::Absolute));
    assert!(matches!(go_info.value(), &MetricValue::Gauge { .. }));
    assert!(go_info.tags().unwrap().contains_key("version"));
    assert_eq!(
        go_info.tag_value("instance"),
        Some("prometheus:9090".to_string())
    );
    assert_eq!(
        go_info.tag_value("endpoint"),
        Some("http://prometheus:9090/metrics".to_string())
    );
}

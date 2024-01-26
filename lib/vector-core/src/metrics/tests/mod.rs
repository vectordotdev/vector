use metrics::counter;
use tracing::{span, Level};

use crate::event::Event;

#[ignore]
#[test]
fn test_labels_injection() {
    _ = super::init();

    let span = span!(
        Level::ERROR,
        "my span",
        component_id = "my_component_id",
        component_type = "my_component_type",
        component_kind = "my_component_kind",
        some_other_label = "qwerty"
    );
    // See https://github.com/tokio-rs/tracing/issues/978
    if span.is_disabled() {
        panic!("test is not configured properly, set TEST_LOG=info env var")
    }
    let _enter = span.enter();

    counter!("labels_injected_total", 1);

    let metric = super::Controller::get()
        .unwrap()
        .capture_metrics()
        .map(|e| e.into_metric())
        .find(|metric| metric.name() == "labels_injected_total")
        .unwrap();

    let expected_tags = Some(
        vec![
            ("component_id".to_owned(), "my_component_id".to_owned()),
            ("component_type".to_owned(), "my_component_type".to_owned()),
            ("component_kind".to_owned(), "my_component_kind".to_owned()),
        ]
        .into_iter()
        .collect(),
    );

    assert_eq!(metric.tags(), expected_tags.as_ref());
}

#[test]
fn test_cardinality_metric() {
    _ = super::init();

    let capture_value = || {
        let metric = super::Controller::get()
            .unwrap()
            .capture_metrics()
            .map(Event::into_metric)
            .find(|metric| metric.name() == super::CARDINALITY_KEY_NAME)
            .unwrap();
        match metric.data.value {
            crate::event::MetricValue::Counter { value } => value,
            _ => panic!("invalid metric value type, expected counter, got something else"),
        }
    };

    let initial_value = capture_value();

    counter!("cardinality_test_metric_1", 1);
    assert!(capture_value() >= initial_value + 1.0);

    counter!("cardinality_test_metric_1", 1);
    assert!(capture_value() >= initial_value + 1.0);

    counter!("cardinality_test_metric_2", 1);
    counter!("cardinality_test_metric_3", 1);
    assert!(capture_value() >= initial_value + 3.0);

    // Other tests could possibly increase the cardinality, so just
    // try adding the same test metrics a few times and fail only if
    // it keeps increasing.
    for count in 1..=10 {
        let start_value = capture_value();
        counter!("cardinality_test_metric_1", 1);
        counter!("cardinality_test_metric_2", 1);
        counter!("cardinality_test_metric_3", 1);
        let end_value = capture_value();
        assert!(end_value >= start_value);
        if start_value == end_value {
            break;
        }
        if count == 10 {
            panic!("Cardinality count still increasing after 10 loops!");
        }
    }
}

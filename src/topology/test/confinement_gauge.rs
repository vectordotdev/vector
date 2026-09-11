//! Tests for the topology-owned `security_confinement_disabled` gauge.
//!
//! The gauge reports whether a sink has opted out of template confinement
//! (`dangerously_allow_unconfined_template_resolution`). The topology owns it
//! for each sink's lifetime so it does not expire while the sink runs, and
//! reconciles it on every (re)load. These tests exercise the observable parts:
//! per-sink identity (no cross-sink clobbering) and value updates on reload.

use vector_lib::metrics::Controller;

use crate::{
    config::Config,
    event::{Metric, MetricValue},
    test_util::{
        mock::{basic_sink, basic_sink_with_data, basic_source},
        start_topology, trace_init,
    },
};

const GAUGE: &str = "security_confinement_disabled";

/// Find the confinement gauge for a specific sink `component_id` in a captured
/// metrics snapshot. Filtering by `component_id` keeps the assertion isolated
/// from other tests sharing the process-wide metrics registry.
fn confinement_gauge<'a>(metrics: &'a [Metric], component_id: &str) -> Option<&'a Metric> {
    metrics.iter().find(|m| {
        m.name() == GAUGE
            && m.tags().and_then(|tags| tags.get("component_id")) == Some(component_id)
    })
}

#[track_caller]
fn assert_gauge(metrics: &[Metric], component_id: &str, expected: f64) {
    let metric = confinement_gauge(metrics, component_id).unwrap_or_else(|| {
        panic!(
            "{GAUGE} not found for sink '{component_id}'; present series: {:?}",
            metrics
                .iter()
                .filter(|m| m.name() == GAUGE)
                .filter_map(|m| m
                    .tags()
                    .and_then(|t| t.get("component_id").map(String::from)))
                .collect::<Vec<_>>(),
        )
    });

    let tags = metric.tags().expect("gauge must carry component tags");
    assert_eq!(tags.get("component_kind"), Some("sink"));
    assert_eq!(tags.get("component_type"), Some("test_basic"));

    match metric.value() {
        MetricValue::Gauge { value } => assert_eq!(
            *value, expected,
            "unexpected {GAUGE} value for '{component_id}'"
        ),
        other => panic!("expected Gauge, got {other:?}"),
    }
}

/// At startup the gauge is emitted per sink, keyed by `component_id`, so two
/// sinks of the same type with different confinement settings produce two
/// distinct series rather than clobbering a single type-keyed series.
#[tokio::test]
async fn per_sink_series_at_startup() {
    trace_init();
    let controller = Controller::get().expect("metrics controller");

    let source_id = "conf_gauge_startup_source";
    let disabled_id = "conf_gauge_startup_disabled";
    let enabled_id = "conf_gauge_startup_enabled";

    let (_src_tx, source_config) = basic_source();
    let (_rx_disabled, disabled_sink) = basic_sink(1);
    let (_rx_enabled, enabled_sink) = basic_sink(1);

    let mut config = Config::builder();
    config.add_source(source_id, source_config);
    config.add_sink(
        disabled_id,
        &[source_id],
        disabled_sink.with_confinement(true),
    );
    config.add_sink(
        enabled_id,
        &[source_id],
        enabled_sink.with_confinement(false),
    );

    let (topology, _shutdown) = start_topology(config.build().unwrap(), false).await;

    let metrics = controller.capture_metrics();
    assert_gauge(&metrics, disabled_id, 1.0);
    assert_gauge(&metrics, enabled_id, 0.0);

    topology.stop().await;
}

/// A sink that does not participate in confinement (the default mock) emits no
/// gauge — matching the pre-existing behavior where only confinement-aware
/// sinks reported.
#[tokio::test]
async fn no_gauge_for_non_confinement_sink() {
    trace_init();
    let controller = Controller::get().expect("metrics controller");

    let source_id = "conf_gauge_absent_source";
    let sink_id = "conf_gauge_absent_sink";

    let (_src_tx, source_config) = basic_source();
    let (_rx, sink) = basic_sink(1);

    let mut config = Config::builder();
    config.add_source(source_id, source_config);
    config.add_sink(sink_id, &[source_id], sink);

    let (topology, _shutdown) = start_topology(config.build().unwrap(), false).await;

    let metrics = controller.capture_metrics();
    assert!(
        confinement_gauge(&metrics, sink_id).is_none(),
        "sink without confinement config should emit no {GAUGE} gauge"
    );

    topology.stop().await;
}

/// On reload the gauge value is updated to reflect the sink's current
/// confinement setting.
#[tokio::test]
async fn value_updated_on_reload() {
    trace_init();
    let controller = Controller::get().expect("metrics controller");

    let source_id = "conf_gauge_reload_source";
    let sink_id = "conf_gauge_reload_sink";

    let (_src_tx, source_config) = basic_source();
    // `data` differs across the two revisions so the reload detects the sink as
    // changed and re-spawns it, mirroring a real config edit.
    let (_rx1, sink_v1) = basic_sink_with_data(1, "v1");

    let mut config = Config::builder();
    config.add_source(source_id, source_config);
    config.add_sink(sink_id, &[source_id], sink_v1.with_confinement(true));

    let (mut topology, _shutdown) = start_topology(config.build().unwrap(), false).await;

    assert_gauge(&controller.capture_metrics(), sink_id, 1.0);

    // Reload with confinement disabled flipped off.
    let (_src_tx2, source_config2) = basic_source();
    let (_rx2, sink_v2) = basic_sink_with_data(1, "v2");
    let mut new_config = Config::builder();
    new_config.add_source(source_id, source_config2);
    new_config.add_sink(sink_id, &[source_id], sink_v2.with_confinement(false));

    topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .expect("reload should succeed");

    assert_gauge(&controller.capture_metrics(), sink_id, 0.0);

    topology.stop().await;
}

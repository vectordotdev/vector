use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use futures::{Stream, StreamExt, future};
use tokio::time::timeout;
use vector_lib::{extra_context::ExtraContext, source_sender::SourceSenderItem};

use crate::{
    config::Config,
    event::EventContainer,
    sources::internal_metrics::InternalMetricsConfig,
    test_util::{
        mock::{basic_sink, basic_source, basic_transform},
        start_topology, trace_init,
    },
};

fn internal_metrics(scrape_interval_secs: f64) -> InternalMetricsConfig {
    InternalMetricsConfig {
        scrape_interval_secs: Duration::from_secs_f64(scrape_interval_secs),
        ..Default::default()
    }
}

/// Consumes a sink's output in the background so that a full channel never applies backpressure
/// that would stall the upstream sources and, with them, reloads and shutdown.
fn drain(stream: impl Stream<Item = SourceSenderItem> + Send + 'static) {
    tokio::spawn(stream.for_each(|_| future::ready(())));
}

fn edges(expected: &[&str]) -> BTreeSet<String> {
    expected.iter().map(ToString::to_string).collect()
}

/// Reads scrapes from an `internal_metrics` output until one contains exactly the `expected`
/// `component_connections` edges, formatted as `kind:type:id[.output] -> kind:type:id`.
///
/// All metrics from a single scrape share a timestamp, which is used to group them, since a scrape
/// can be split across several batches.
async fn wait_for_connections(
    stream: &mut (impl Stream<Item = SourceSenderItem> + Unpin),
    expected: &BTreeSet<String>,
) {
    let mut scrapes: HashMap<_, BTreeSet<String>> = HashMap::new();
    let result = timeout(Duration::from_secs(10), async {
        while let Some(item) = stream.next().await {
            for event in item.events.into_events() {
                let metric = event.into_metric();
                if metric.name() != "component_connections" {
                    continue;
                }
                let tags = metric.tags().expect("connection metric has tags");
                let tag = |name: &str| tags.get(name).unwrap_or("?").to_owned();
                let output = tags
                    .get("from_output")
                    .map(|output| format!(".{output}"))
                    .unwrap_or_default();
                let edge = format!(
                    "{}:{}:{}{output} -> {}:{}:{}",
                    tag("from_component_kind"),
                    tag("from_component_type"),
                    tag("from_component_id"),
                    tag("to_component_kind"),
                    tag("to_component_type"),
                    tag("to_component_id"),
                );
                let scrape = scrapes.entry(metric.timestamp()).or_default();
                scrape.insert(edge);
                if scrape == expected {
                    return;
                }
            }
        }
        panic!("internal_metrics output closed");
    })
    .await;
    assert!(
        result.is_ok(),
        "expected connections {expected:#?} never observed, got {scrapes:#?}"
    );
}

#[tokio::test]
async fn topology_connections_follow_reloads() {
    trace_init();

    let (in1, source1) = basic_source();
    let (out1, sink1) = basic_sink(10);
    let (mut metrics1, metrics_sink1) = basic_sink(100);
    let (metrics2, metrics_sink2) = basic_sink(100);
    drain(out1);
    drain(metrics2);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], basic_transform("", 0.0));
    config.add_sink("out1", &["t1"], sink1);
    config.add_source("im1", internal_metrics(0.1));
    config.add_sink("m1", &["im1"], metrics_sink1);
    config.add_source("im2", internal_metrics(0.1));
    config.add_sink("m2", &["im2"], metrics_sink2);

    let (mut topology, _) = start_topology(config.build().unwrap(), false).await;

    wait_for_connections(
        &mut metrics1,
        &edges(&[
            "source:test_basic:in1 -> transform:test_basic:t1",
            "transform:test_basic:t1 -> sink:test_basic:out1",
            "source:internal_metrics:im1 -> sink:test_basic:m1",
            "source:internal_metrics:im2 -> sink:test_basic:m2",
        ]),
    )
    .await;

    // Remove `out1`, add `out2`, and change `im2` so that it is rebuilt. `im1` is unchanged and must
    // still observe the new topology, including the component type of the newly added sink.
    let mut config = Config::builder();
    config.add_source("in1", basic_source().1);
    config.add_transform("t1", &["in1"], basic_transform("", 0.0));
    let (out2, sink2) = basic_sink(10);
    drain(out2);
    config.add_sink("out2", &["in1"], sink2);
    config.add_source("im1", internal_metrics(0.1));
    // `m1` and `m2` are unchanged, so the running sinks, and their outputs, are kept.
    config.add_sink("m1", &["im1"], basic_sink(100).1);
    config.add_source("im2", internal_metrics(0.2));
    config.add_sink("m2", &["im2"], basic_sink(100).1);

    timeout(
        Duration::from_secs(30),
        topology.reload_config_and_respawn(config.build().unwrap(), ExtraContext::default()),
    )
    .await
    .expect("reload timed out")
    .unwrap();

    wait_for_connections(
        &mut metrics1,
        &edges(&[
            "source:test_basic:in1 -> transform:test_basic:t1",
            "source:test_basic:in1 -> sink:test_basic:out2",
            "source:internal_metrics:im1 -> sink:test_basic:m1",
            "source:internal_metrics:im2 -> sink:test_basic:m2",
        ]),
    )
    .await;

    drain(metrics1);
    drop(in1);
    timeout(Duration::from_secs(30), topology.stop())
        .await
        .expect("shutdown timed out");
}

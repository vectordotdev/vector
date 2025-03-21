use std::{
    net::{SocketAddr, TcpListener},
    num::NonZeroU64,
    time::Duration,
};

use futures::StreamExt;
use tokio::time::sleep;
use tokio_stream::wrappers::UnboundedReceiverStream;
use vector_lib::buffers::{BufferConfig, BufferType, WhenFull};
use vector_lib::config::ComponentKey;

use crate::{
    config::Config,
    sinks::prometheus::exporter::PrometheusExporterConfig,
    sources::{
        internal_metrics::InternalMetricsConfig, prometheus::PrometheusRemoteWriteConfig,
        splunk_hec::SplunkConfig,
    },
    test_util::{self, mock::basic_sink, next_addr, start_topology, temp_dir, wait_for_tcp},
};

fn internal_metrics_source() -> InternalMetricsConfig {
    InternalMetricsConfig {
        // TODO: A scrape interval left at the default of 1.0 seconds or less triggers some kind of
        // race condition in the `topology_disk_buffer_conflict` test below, but it is unclear
        // why. All these tests should work regardless of the scrape interval. This warrants further
        // investigation.
        scrape_interval_secs: Duration::from_secs_f64(1.1),
        ..Default::default()
    }
}

fn prom_remote_write_source(addr: SocketAddr) -> PrometheusRemoteWriteConfig {
    PrometheusRemoteWriteConfig::from_address(addr)
}

fn prom_exporter_sink(addr: SocketAddr, flush_period_secs: u64) -> PrometheusExporterConfig {
    PrometheusExporterConfig {
        address: addr,
        flush_period_secs: Duration::from_secs(flush_period_secs),
        ..Default::default()
    }
}

fn splunk_source_config(addr: SocketAddr) -> SplunkConfig {
    let mut config = SplunkConfig::default();
    config.address = addr;
    config
}

#[tokio::test]
async fn topology_reuse_old_port() {
    test_util::trace_init();

    let address = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in1", prom_remote_write_source(address));
    old_config.add_sink("out", &["in1"], basic_sink(1).1);

    let mut new_config = Config::builder();
    new_config.add_source("in2", prom_remote_write_source(address));
    new_config.add_sink("out", &["in2"], basic_sink(1).1);

    let (mut topology, _) = start_topology(old_config.build().unwrap(), false).await;
    assert!(topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_rebuild_old() {
    test_util::trace_init();

    let address_0 = next_addr();
    let address_1 = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", splunk_source_config(address_0));
    old_config.add_sink("out", &["in"], basic_sink(1).1);

    let mut new_config = Config::builder();
    new_config.add_source("in", splunk_source_config(address_1));
    new_config.add_sink("out", &["in"], basic_sink(1).1);

    // Will cause the new_config to fail on build
    let _bind = TcpListener::bind(address_1).unwrap();

    let (mut topology, _) = start_topology(old_config.build().unwrap(), false).await;
    assert!(!topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_old() {
    test_util::trace_init();

    let address = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", prom_remote_write_source(address));
    old_config.add_sink("out", &["in"], basic_sink(1).1);

    let (mut topology, _) = start_topology(old_config.clone().build().unwrap(), false).await;
    assert!(topology
        .reload_config_and_respawn(old_config.build().unwrap(), Default::default())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_reuse_old_port_sink() {
    // TODO: Write a test source that emits only metrics, and a test sink that can bind a TCP listener, so we can
    // replace `internal_metrics` and `prometheus_exporter` here. We additionally need to ensure the metrics subsystem
    // is enabled to use `internal_metrics`, otherwise it throws an error when trying to build the component.
    test_util::trace_init();

    let address = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", internal_metrics_source());
    old_config.add_sink("out", &["in"], prom_exporter_sink(address, 1));

    let mut new_config = Config::builder();
    new_config.add_source("in", internal_metrics_source());
    new_config.add_sink("out", &["in"], prom_exporter_sink(address, 2));

    reload_sink_test(
        old_config.build().unwrap(),
        new_config.build().unwrap(),
        address,
        address,
    )
    .await;
}

#[tokio::test]
async fn topology_reuse_old_port_cross_dependency() {
    // TODO: Write a test source that emits only metrics, and a test sink that can bind a TCP listener, so we can
    // replace `internal_metrics` and `prometheus_exporter` here. We additionally need to ensure the metrics subsystem
    // is enabled to use `internal_metrics`, otherwise it throws an error when trying to build the component.
    test_util::trace_init();

    // Reload with source that uses address of changed sink.
    let address_0 = next_addr();
    let address_1 = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", internal_metrics_source());
    old_config.add_sink("out", &["in"], prom_exporter_sink(address_0, 1));

    let mut new_config = Config::builder();
    new_config.add_source("in", prom_remote_write_source(address_0));
    new_config.add_sink("out", &["in"], prom_exporter_sink(address_1, 1));

    reload_sink_test(
        old_config.build().unwrap(),
        new_config.build().unwrap(),
        address_0,
        address_1,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn topology_disk_buffer_conflict() {
    // TODO: Write a test source that emits only metrics, and a test sink that can bind a TCP listener, so we can
    // replace `internal_metrics` and `prometheus_exporter` here. We additionally need to ensure the metrics subsystem
    // is enabled to use `internal_metrics`, otherwise it throws an error when trying to build the component.
    test_util::trace_init();

    let address_0 = next_addr();
    let address_1 = next_addr();

    let data_dir = temp_dir();
    std::fs::create_dir(&data_dir).unwrap();

    let mut old_config = Config::builder();
    old_config.global.data_dir = Some(data_dir);
    old_config.add_source("in", internal_metrics_source());
    old_config.add_sink("out", &["in"], prom_exporter_sink(address_0, 1));

    let sink_key = ComponentKey::from("out");
    old_config.sinks[&sink_key].buffer = BufferConfig::Single(BufferType::DiskV2 {
        max_size: NonZeroU64::new(268435488).unwrap(),
        when_full: WhenFull::Block,
    });

    let mut new_config = old_config.clone();
    new_config.sinks[&sink_key].inner = prom_exporter_sink(address_1, 1).into();
    new_config.sinks[&sink_key].buffer = BufferConfig::Single(BufferType::DiskV2 {
        max_size: NonZeroU64::new(268435488).unwrap(),
        when_full: WhenFull::Block,
    });

    reload_sink_test(
        old_config.build().unwrap(),
        new_config.build().unwrap(),
        address_0,
        address_1,
    )
    .await;
}

#[tokio::test]
async fn topology_reload_with_new_components() {
    // TODO: Write a test source that emits only metrics, and a test sink that can bind a TCP listener, so we can
    // replace `internal_metrics` and `prometheus_exporter` here. We additionally need to ensure the metrics subsystem
    // is enabled to use `internal_metrics`, otherwise it throws an error when trying to build the component.
    test_util::trace_init();

    // This specifically exercises that we can add new components -- no changed or removed
    // components -- via the reload mechanism and without any issues.
    let address_0 = next_addr();
    let address_1 = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in1", internal_metrics_source());
    old_config.add_sink("out1", &["in1"], prom_exporter_sink(address_0, 1));

    let mut new_config = Config::builder();
    new_config.add_source("in1", internal_metrics_source());
    new_config.add_sink("out1", &["in1"], prom_exporter_sink(address_0, 1));
    new_config.add_source("in2", internal_metrics_source());
    new_config.add_sink("out2", &["in2"], prom_exporter_sink(address_1, 1));

    reload_sink_test(
        old_config.build().unwrap(),
        new_config.build().unwrap(),
        address_0,
        address_1,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn topology_readd_input() {
    // TODO: Write a test source that emits only metrics, and a test sink that can bind a TCP listener, so we can
    // replace `internal_metrics` and `prometheus_exporter` here. We additionally need to ensure the metrics subsystem
    // is enabled to use `internal_metrics`, otherwise it throws an error when trying to build the component.
    test_util::trace_init();

    let address_0 = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in1", internal_metrics_source());
    old_config.add_source("in2", internal_metrics_source());
    old_config.add_sink("out", &["in1", "in2"], prom_exporter_sink(address_0, 1));
    let (mut topology, crash) = start_topology(old_config.build().unwrap(), false).await;

    // remove in2
    let mut new_config = Config::builder();
    new_config.add_source("in1", internal_metrics_source());
    new_config.add_source("in2", internal_metrics_source());
    new_config.add_sink("out", &["in1"], prom_exporter_sink(address_0, 1));
    assert!(topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap());

    // re-add in2
    let mut new_config = Config::builder();
    new_config.add_source("in1", internal_metrics_source());
    new_config.add_source("in2", internal_metrics_source());
    new_config.add_sink("out", &["in1", "in2"], prom_exporter_sink(address_0, 1));
    assert!(topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap());

    sleep(Duration::from_secs(1)).await;
    topology.stop().await;

    // sink should not crash
    assert!(UnboundedReceiverStream::new(crash)
        .collect::<Vec<_>>()
        .await
        .is_empty());
}

async fn reload_sink_test(
    old_config: Config,
    new_config: Config,
    old_address: SocketAddr,
    new_address: SocketAddr,
) {
    // Start a topology from the "old" configuration, which should result in a component listening on `old_address`.
    let (mut topology, crash) = start_topology(old_config, false).await;
    let mut crash_stream = UnboundedReceiverStream::new(crash);

    wait_for_tcp(old_address).await;

    // Make sure the topology is fully running: other components, etc.
    sleep(Duration::from_secs(1)).await;

    // Now reload the topology with the "new" configuration, and make sure that a component is now listening on `new_address`.
    assert!(topology
        .reload_config_and_respawn(new_config, Default::default())
        .await
        .unwrap());

    // Give the old topology configuration a chance to shutdown cleanly, etc.
    sleep(Duration::from_secs(2)).await;

    tokio::select! {
        _ = wait_for_tcp(new_address) => {},
        _ = crash_stream.next() => panic!(),
    }
}

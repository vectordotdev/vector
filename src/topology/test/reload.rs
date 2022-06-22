use std::{
    net::{SocketAddr, TcpListener},
    time::Duration,
};

use futures::StreamExt;
use tokio::time::sleep;
use tokio_stream::wrappers::UnboundedReceiverStream;
use vector_buffers::{BufferConfig, BufferType, WhenFull};
use vector_core::config::ComponentKey;

use crate::{
    config::Config,
    sinks::{blackhole::BlackholeConfig, prometheus::exporter::PrometheusExporterConfig},
    sources::{internal_metrics::InternalMetricsConfig, prometheus::PrometheusRemoteWriteConfig},
    test_util::{next_addr, start_topology, temp_dir, wait_for_tcp},
};

fn internal_metrics_source() -> InternalMetricsConfig {
    InternalMetricsConfig::default()
}

fn prom_remote_write_source(addr: SocketAddr) -> PrometheusRemoteWriteConfig {
    PrometheusRemoteWriteConfig::from_address(addr)
}

fn blackhole_sink() -> BlackholeConfig {
    BlackholeConfig::default()
}

fn prom_exporter_sink(addr: SocketAddr, flush_period_secs: u64) -> PrometheusExporterConfig {
    PrometheusExporterConfig {
        address: addr,
        flush_period_secs: Duration::from_secs(flush_period_secs),
        ..Default::default()
    }
}

#[tokio::test]
async fn topology_reuse_old_port() {
    let address = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in1", prom_remote_write_source(address));
    old_config.add_sink("out", &["in1"], blackhole_sink());

    let mut new_config = Config::builder();
    new_config.add_source("in2", prom_remote_write_source(address));
    new_config.add_sink("out", &["in2"], blackhole_sink());

    let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
    assert!(topology
        .reload_config_and_respawn(new_config.build().unwrap())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_rebuild_old() {
    let address_0 = next_addr();
    let address_1 = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", prom_remote_write_source(address_0));
    old_config.add_sink("out", &["in"], blackhole_sink());

    let mut new_config = Config::builder();
    new_config.add_source("in", prom_remote_write_source(address_1));
    new_config.add_sink("out", &["in"], blackhole_sink());

    // Will cause the new_config to fail on build
    let _bind = TcpListener::bind(address_1).unwrap();

    let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
    assert!(!topology
        .reload_config_and_respawn(new_config.build().unwrap())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_old() {
    let address = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", prom_remote_write_source(address));
    old_config.add_sink("out", &["in"], blackhole_sink());

    let (mut topology, _crash) = start_topology(old_config.clone().build().unwrap(), false).await;
    assert!(topology
        .reload_config_and_respawn(old_config.build().unwrap())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_reuse_old_port_sink() {
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
    let address_0 = next_addr();
    let address_1 = next_addr();

    let data_dir = temp_dir();
    std::fs::create_dir(&data_dir).unwrap();

    let mut old_config = Config::builder();
    old_config.global.data_dir = Some(data_dir);
    old_config.add_source("in", internal_metrics_source());
    old_config.add_sink("out", &["in"], prom_exporter_sink(address_0, 1));

    let sink_key = ComponentKey::from("out");
    old_config.sinks[&sink_key].buffer = BufferConfig {
        stages: vec![BufferType::DiskV1 {
            max_size: std::num::NonZeroU64::new(1024).unwrap(),
            when_full: WhenFull::Block,
        }],
    };

    let mut new_config = old_config.clone();
    new_config.sinks[&sink_key].inner = Box::new(prom_exporter_sink(address_1, 1));
    new_config.sinks[&sink_key].buffer = BufferConfig {
        stages: vec![BufferType::DiskV1 {
            max_size: std::num::NonZeroU64::new(1024).unwrap(),
            when_full: WhenFull::Block,
        }],
    };

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
        .reload_config_and_respawn(new_config)
        .await
        .unwrap());

    // Give the old topology configuration a chance to shutdown cleanly, etc.
    sleep(Duration::from_secs(2)).await;

    tokio::select! {
        _ = wait_for_tcp(new_address) => {},
        _ = crash_stream.next() => panic!(),
    }
}

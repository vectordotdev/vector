use std::{
    collections::HashSet,
    net::{SocketAddr, TcpListener},
    num::{NonZeroU64, NonZeroUsize},
    time::Duration,
};

use futures::StreamExt;
use tokio::time::sleep;
use tokio_stream::wrappers::UnboundedReceiverStream;
use vector_lib::{
    buffers::{BufferConfig, BufferType, MemoryBufferSize, WhenFull},
    config::ComponentKey,
};

use crate::{
    config::Config,
    sinks::prometheus::exporter::PrometheusExporterConfig,
    sources::{
        internal_metrics::InternalMetricsConfig, prometheus::PrometheusRemoteWriteConfig,
        splunk_hec::SplunkConfig,
    },
    test_util::{self, addr::next_addr, mock::basic_sink, start_topology, temp_dir, wait_for_tcp},
    topology::ReloadError::*,
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

    let (_guard, address) = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in1", prom_remote_write_source(address));
    old_config.add_sink("out", &["in1"], basic_sink(1).1);

    let mut new_config = Config::builder();
    new_config.add_source("in2", prom_remote_write_source(address));
    new_config.add_sink("out", &["in2"], basic_sink(1).1);

    let (mut topology, _) = start_topology(old_config.build().unwrap(), false).await;
    topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn topology_rebuild_old() {
    test_util::trace_init();

    let (_guard_0, address_0) = next_addr();
    let (_guard_1, address_1) = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", splunk_source_config(address_0));
    old_config.add_sink("out", &["in"], basic_sink(1).1);

    let mut new_config = Config::builder();
    new_config.add_source("in", splunk_source_config(address_1));
    new_config.add_sink("out", &["in"], basic_sink(1).1);

    // Will cause the new_config to fail on build
    let _bind = TcpListener::bind(address_1).unwrap();

    let (mut topology, _) = start_topology(old_config.build().unwrap(), false).await;
    let result = topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await;

    // Should fail with TopologyBuildFailed error due to port conflict
    assert!(matches!(result, Err(TopologyBuildFailed)));
}

#[tokio::test]
async fn topology_old() {
    test_util::trace_init();

    let (_guard, address) = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", prom_remote_write_source(address));
    old_config.add_sink("out", &["in"], basic_sink(1).1);

    let (mut topology, _) = start_topology(old_config.clone().build().unwrap(), false).await;
    topology
        .reload_config_and_respawn(old_config.build().unwrap(), Default::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn topology_reuse_old_port_sink() {
    // TODO: Write a test source that emits only metrics, and a test sink that can bind a TCP listener, so we can
    // replace `internal_metrics` and `prometheus_exporter` here. We additionally need to ensure the metrics subsystem
    // is enabled to use `internal_metrics`, otherwise it throws an error when trying to build the component.
    test_util::trace_init();

    let (_guard, address) = next_addr();

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
    let (_guard_0, address_0) = next_addr();
    let (_guard_1, address_1) = next_addr();

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

    let (_guard_0, address_0) = next_addr();
    let (_guard_1, address_1) = next_addr();

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
    let (_guard_0, address_0) = next_addr();
    let (_guard_1, address_1) = next_addr();

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

    let (_guard, address_0) = next_addr();

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
    topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap();

    // re-add in2
    let mut new_config = Config::builder();
    new_config.add_source("in1", internal_metrics_source());
    new_config.add_source("in2", internal_metrics_source());
    new_config.add_sink("out", &["in1", "in2"], prom_exporter_sink(address_0, 1));
    topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap();

    sleep(Duration::from_secs(1)).await;
    topology.stop().await;

    // sink should not crash
    assert!(
        UnboundedReceiverStream::new(crash)
            .collect::<Vec<_>>()
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn topology_reload_component() {
    test_util::trace_init();
    let (_guard, address_0) = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in1", internal_metrics_source());
    old_config.add_source("in2", internal_metrics_source());
    old_config.add_sink("out", &["in1", "in2"], prom_exporter_sink(address_0, 1));
    let (mut topology, crash) = start_topology(old_config.clone().build().unwrap(), false).await;
    let mut crash_stream = UnboundedReceiverStream::new(crash);

    topology.extend_reload_set(HashSet::from_iter(vec![ComponentKey::from("out")]));

    topology
        .reload_config_and_respawn(old_config.build().unwrap(), Default::default())
        .await
        .unwrap();

    // TODO: Implement notification to avoid the sleep()
    // Give the old topology configuration a chance to shutdown cleanly, etc.
    sleep(Duration::from_secs(2)).await;

    tokio::select! {
        _ = wait_for_tcp(address_0) => {},
        _ = crash_stream.next() => panic!(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn topology_disk_buffer_config_change_does_not_stall() {
    // Changing a disk buffer's configuration on a running sink (e.g. via in-situ
    // config edit) must not stall the reload. Previously, the detach trigger was
    // only cancelled for sinks whose buffers were being reused, so sinks with
    // changed disk buffer configs would never have their input stream terminated,
    // causing the reload to hang indefinitely.
    test_util::trace_init();

    let (_guard, address) = next_addr();

    let data_dir = temp_dir();
    std::fs::create_dir(&data_dir).unwrap();

    let mut old_config = Config::builder();
    old_config.global.data_dir = Some(data_dir);
    old_config.add_source("in", internal_metrics_source());
    old_config.add_sink("out", &["in"], prom_exporter_sink(address, 1));

    let sink_key = ComponentKey::from("out");
    old_config.sinks[&sink_key].buffer = BufferConfig::Single(BufferType::DiskV2 {
        max_size: NonZeroU64::new(268435488).unwrap(),
        when_full: WhenFull::Block,
    });

    // Change only the disk buffer's max_size.
    let mut new_config = old_config.clone();
    new_config.sinks[&sink_key].buffer = BufferConfig::Single(BufferType::DiskV2 {
        max_size: NonZeroU64::new(536870912).unwrap(),
        when_full: WhenFull::Block,
    });

    let (mut topology, crash) = start_topology(old_config.build().unwrap(), true).await;
    let mut crash_stream = UnboundedReceiverStream::new(crash);

    tokio::select! {
        _ = wait_for_tcp(address) => {},
        _ = crash_stream.next() => panic!("topology crashed before reload"),
    }

    // Simulate an in-situ config edit: the config watcher would put the changed
    // sink into components_to_reload, which excludes it from reuse_buffers.
    topology.extend_reload_set(HashSet::from_iter(vec![sink_key]));

    let reload_result = tokio::time::timeout(
        Duration::from_secs(5),
        topology.reload_config_and_respawn(new_config.build().unwrap(), Default::default()),
    )
    .await;

    assert!(
        reload_result.is_ok(),
        "Reload stalled: changing a disk buffer config should not cause the reload to hang"
    );
    reload_result.unwrap().unwrap();

    // Verify the new sink is running.
    tokio::select! {
        _ = wait_for_tcp(address) => {},
        _ = crash_stream.next() => panic!("topology crashed after reload"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn topology_disk_buffer_config_change_chained_does_not_stall() {
    // Same as above but with a chained memory → disk overflow buffer to verify
    // that the writer-drop notification is collected from overflow stages too.
    test_util::trace_init();

    let (_guard, address) = next_addr();

    let data_dir = temp_dir();
    std::fs::create_dir(&data_dir).unwrap();

    let memory_stage = BufferType::Memory {
        size: MemoryBufferSize::MaxEvents(NonZeroUsize::new(100).unwrap()),
        when_full: WhenFull::Overflow,
    };

    let mut old_config = Config::builder();
    old_config.global.data_dir = Some(data_dir);
    old_config.add_source("in", internal_metrics_source());
    old_config.add_sink("out", &["in"], prom_exporter_sink(address, 1));

    let sink_key = ComponentKey::from("out");
    old_config.sinks[&sink_key].buffer = BufferConfig::Chained(vec![
        memory_stage,
        BufferType::DiskV2 {
            max_size: NonZeroU64::new(268435488).unwrap(),
            when_full: WhenFull::Block,
        },
    ]);

    // Change only the disk overflow stage's max_size.
    let mut new_config = old_config.clone();
    new_config.sinks[&sink_key].buffer = BufferConfig::Chained(vec![
        memory_stage,
        BufferType::DiskV2 {
            max_size: NonZeroU64::new(536870912).unwrap(),
            when_full: WhenFull::Block,
        },
    ]);

    let (mut topology, crash) = start_topology(old_config.build().unwrap(), true).await;
    let mut crash_stream = UnboundedReceiverStream::new(crash);

    tokio::select! {
        _ = wait_for_tcp(address) => {},
        _ = crash_stream.next() => panic!("topology crashed before reload"),
    }

    topology.extend_reload_set(HashSet::from_iter(vec![sink_key]));

    let reload_result = tokio::time::timeout(
        Duration::from_secs(5),
        topology.reload_config_and_respawn(new_config.build().unwrap(), Default::default()),
    )
    .await;

    assert!(
        reload_result.is_ok(),
        "Reload stalled: changing a chained disk buffer config should not cause the reload to hang"
    );
    reload_result.unwrap().unwrap();

    // Verify the new sink is running.
    tokio::select! {
        _ = wait_for_tcp(address) => {},
        _ = crash_stream.next() => panic!("topology crashed after reload"),
    }
}

/// Regression test for https://github.com/vectordotdev/vector/issues/24125
///
/// When a sink with a conflicting resource (e.g., a bound port) is reloaded,
/// the old sink must be waited on before the new one can start. Previously,
/// `remove_inputs` sent `Pause` to the upstream fanout, which blocked the
/// source pump in `wait_for_replacements` — creating a circular dependency
/// with `shutdown_diff`. This test verifies that the reload completes
/// within a reasonable time instead of stalling.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn topology_reload_conflicting_sink_does_not_stall() {
    test_util::trace_init();

    let (_guard, address) = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", internal_metrics_source());
    old_config.add_sink("out", &["in"], prom_exporter_sink(address, 1));

    // Change only the flush period so the sink config differs but the
    // resource (bound address) stays the same, creating a conflict.
    let mut new_config = Config::builder();
    new_config.add_source("in", internal_metrics_source());
    new_config.add_sink("out", &["in"], prom_exporter_sink(address, 2));

    let (mut topology, crash) = start_topology(old_config.build().unwrap(), false).await;
    let mut crash_stream = UnboundedReceiverStream::new(crash);

    tokio::select! {
        _ = wait_for_tcp(address) => {},
        _ = crash_stream.next() => panic!("topology crashed before reload"),
    }

    // Let some events flow so the source pump is active.
    sleep(Duration::from_secs(2)).await;

    let reload_result = tokio::time::timeout(
        Duration::from_secs(10),
        topology.reload_config_and_respawn(new_config.build().unwrap(), Default::default()),
    )
    .await;

    assert!(
        reload_result.is_ok(),
        "Reload stalled: reloading a sink with conflicting resources should not block the source pump"
    );
    reload_result.unwrap().unwrap();

    // Verify the new sink is running.
    tokio::select! {
        _ = wait_for_tcp(address) => {},
        _ = crash_stream.next() => panic!("topology crashed after reload"),
    }
}

/// Similar regression test for the SIGHUP reload path where sinks end up in
/// `reuse_buffers` (buffer config unchanged, no `components_to_reload`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn topology_reload_reuse_buffer_does_not_stall() {
    test_util::trace_init();

    let (_guard_0, address_0) = next_addr();
    let (_guard_1, address_1) = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", internal_metrics_source());
    old_config.add_sink("out", &["in"], prom_exporter_sink(address_0, 1));

    // Change the address so the sink is in to_change, but don't change
    // the buffer config so it lands in reuse_buffers. Also don't use
    // extend_reload_set so the sink is NOT in components_to_reload
    // (simulating a SIGHUP-style reload).
    let mut new_config = Config::builder();
    new_config.add_source("in", internal_metrics_source());
    new_config.add_sink("out", &["in"], prom_exporter_sink(address_1, 1));

    let (mut topology, crash) = start_topology(old_config.build().unwrap(), false).await;
    let mut crash_stream = UnboundedReceiverStream::new(crash);

    tokio::select! {
        _ = wait_for_tcp(address_0) => {},
        _ = crash_stream.next() => panic!("topology crashed before reload"),
    }

    // Let some events flow so the source pump is active.
    sleep(Duration::from_secs(2)).await;

    let reload_result = tokio::time::timeout(
        Duration::from_secs(10),
        topology.reload_config_and_respawn(new_config.build().unwrap(), Default::default()),
    )
    .await;

    assert!(
        reload_result.is_ok(),
        "Reload stalled: reloading a sink with reused buffer should not block the source pump"
    );
    reload_result.unwrap().unwrap();

    // Verify the new sink is running on the new address.
    tokio::select! {
        _ = wait_for_tcp(address_1) => {},
        _ = crash_stream.next() => panic!("topology crashed after reload"),
    }
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
    topology
        .reload_config_and_respawn(new_config, Default::default())
        .await
        .unwrap();

    // Give the old topology configuration a chance to shutdown cleanly, etc.
    sleep(Duration::from_secs(2)).await;

    tokio::select! {
        _ = wait_for_tcp(new_address) => {},
        _ = crash_stream.next() => panic!(),
    }
}

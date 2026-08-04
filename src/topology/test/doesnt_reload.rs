use std::{collections::HashSet, path::{Path, PathBuf}};

use vector_lib::config::ComponentKey;

use crate::{
    SourceSender,
    config::Config,
    sources::prometheus::PrometheusRemoteWriteConfig,
    test_util::{
        addr::next_addr,
        mock::{
            basic_sink, basic_sink_with_data, basic_source, basic_transform, sinks::BasicSinkConfig,
        },
        start_topology, trace_init,
    },
    topology::ReloadError::*,
};

fn prom_remote_write_source(addr: std::net::SocketAddr) -> PrometheusRemoteWriteConfig {
    PrometheusRemoteWriteConfig::from_address(addr)
}

fn basic_sink_failing_healthcheck_with_data(data: &str) -> BasicSinkConfig {
    let (tx, _rx) = SourceSender::new_test_sender_with_options(1, None);
    BasicSinkConfig::new_with_data(tx, false, data)
}

#[tokio::test]
async fn topology_doesnt_reload_new_data_dir() {
    trace_init();

    let mut old_config = Config::builder();
    old_config.add_source("in", basic_source().1);
    old_config.add_sink("out", &["in"], basic_sink(1).1);
    old_config.global.data_dir = Some(Path::new("/asdf").to_path_buf());
    let mut new_config = old_config.clone();

    let (mut topology, _) = start_topology(old_config.build().unwrap(), false).await;

    new_config.global.data_dir = Some(Path::new("/qwerty").to_path_buf());

    let result = topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await;

    // Should fail with GlobalOptionsChanged error
    assert!(matches!(result, Err(GlobalOptionsChanged { .. })));

    assert_eq!(
        topology.config.global.data_dir,
        Some(Path::new("/asdf").to_path_buf())
    );
}

#[tokio::test]
async fn topology_skips_reload_when_config_unchanged() {
    trace_init();

    let mut config = Config::builder();
    config.add_source("in", basic_source().1);
    config.add_sink("out", &["in"], basic_sink(1).1);

    let (mut topology, _) = start_topology(config.clone().build().unwrap(), false).await;

    let reloaded = topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap();

    assert!(
        !reloaded,
        "identical parsed config should skip component reload"
    );
}

#[tokio::test]
async fn topology_reloads_when_sink_added() {
    trace_init();

    let mut old_config = Config::builder();
    old_config.add_source("in", basic_source().1);
    old_config.add_sink("out", &["in"], basic_sink(1).1);

    let mut new_config = old_config.clone();
    new_config.add_sink("out2", &["in"], basic_sink(1).1);

    let (mut topology, _) = start_topology(old_config.build().unwrap(), false).await;

    let reloaded = topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap();

    assert!(reloaded, "adding a sink should rebuild the topology");
    assert!(topology.config().sink(&ComponentKey::from("out2")).is_some());
}

#[tokio::test]
async fn topology_force_reloads_unchanged_config_via_reload_set() {
    trace_init();

    let mut config = Config::builder();
    config.add_source("in", basic_source().1);
    config.add_sink("out", &["in"], basic_sink(1).1);

    let (mut topology, _) = start_topology(config.clone().build().unwrap(), false).await;

    topology.extend_reload_set(HashSet::from([ComponentKey::from("out")]));

    let reloaded = topology
        .reload_config_and_respawn(config.clone().build().unwrap(), Default::default())
        .await
        .unwrap();

    assert!(
        reloaded,
        "explicit reload set must force component restart even when config is unchanged"
    );

    // pending_reload must be consumed; a follow-up identical reload should skip.
    let reloaded_again = topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap();
    assert!(
        !reloaded_again,
        "forced reload keys must not linger and keep forcing subsequent reloads"
    );
}

#[tokio::test]
async fn topology_preserves_reload_set_after_failed_reload() {
    trace_init();

    let (_guard, address) = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", prom_remote_write_source(address));
    old_config.add_sink("out", &["in"], basic_sink_with_data(1, "v1").1);

    // Prometheus remote_write binds when the source task runs, not during build, so a
    // port conflict cannot produce TopologyBuildFailed here. Use a parsed config change
    // plus a required unhealthy healthcheck instead (same pattern as healthcheck reload tests).
    let mut failing_config = Config::builder();
    failing_config.add_source("in", prom_remote_write_source(address));
    failing_config.add_sink(
        "out",
        &["in"],
        basic_sink_failing_healthcheck_with_data("v2"),
    );
    let mut failing_config = failing_config.build().unwrap();
    failing_config.healthchecks.require_healthy = true;

    let (mut topology, _) = start_topology(old_config.clone().build().unwrap(), false).await;

    topology.extend_reload_set(HashSet::from([ComponentKey::from("out")]));

    let failed = topology
        .reload_config_and_respawn(failing_config, Default::default())
        .await;
    assert!(
        matches!(failed, Err(TopologyBuildFailed)),
        "expected TopologyBuildFailed, got {failed:?}"
    );

    // Force intent from the failed attempt must survive so a subsequent identical
    // reload still restarts the component (e.g. TLS material / external file change).
    let reloaded = topology
        .reload_config_and_respawn(old_config.build().unwrap(), Default::default())
        .await
        .unwrap();
    assert!(
        reloaded,
        "pending_reload must be restored after a failed reload so force intent is not lost"
    );
}

#[tokio::test]
async fn topology_reload_from_disk_forces_external_file_transforms() {
    trace_init();

    let external_file = PathBuf::from("/tmp/vector-test-external.vrl");

    let mut config = Config::builder();
    config.add_source("in", basic_source().1);
    config.add_transform(
        "remap_file",
        &["in"],
        basic_transform("", 0.0).with_files_to_watch(vec![external_file]),
    );
    config.add_sink("out", &["remap_file"], basic_sink(1).1);

    let (mut topology, _) = start_topology(config.clone().build().unwrap(), false).await;
    let new_config = config.build().unwrap();

    assert_eq!(
        new_config.transform_keys_with_external_files(),
        HashSet::from([ComponentKey::from("remap_file")])
    );

    // ReloadFromDisk / SIGHUP path (#23898).
    topology.prepare_reload_from_disk(&new_config);

    let reloaded = topology
        .reload_config_and_respawn(new_config, Default::default())
        .await
        .unwrap();
    assert!(
        reloaded,
        "ReloadFromDisk must restart transforms with external files even when parsed config is unchanged"
    );
}

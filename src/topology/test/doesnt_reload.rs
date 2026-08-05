use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use vector_lib::config::ComponentKey;

use crate::{
    SourceSender,
    config::Config,
    enrichment_tables::file::{FileConfig, FileSettings},
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

    assert!(reloaded, "reload set should force restart");

    // Consumed keys should not force the next identical reload.
    let reloaded_again = topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap();
    assert!(!reloaded_again, "reload set should be cleared after success");
}

#[tokio::test]
async fn topology_preserves_reload_set_after_failed_reload() {
    trace_init();

    let (_guard, address) = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in", prom_remote_write_source(address));
    old_config.add_sink("out", &["in"], basic_sink_with_data(1, "v1").1);

    // Fail via required unhealthy healthcheck (bind conflicts won't fail at build).
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

    // Failed reload should restore pending_reload.
    let reloaded = topology
        .reload_config_and_respawn(old_config.build().unwrap(), Default::default())
        .await
        .unwrap();
    assert!(reloaded, "pending_reload should survive failed reload");
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
    assert!(reloaded, "external-file transforms should reload on ReloadFromDisk");
}

fn file_enrichment_table(path: PathBuf) -> FileConfig {
    FileConfig {
        file: FileSettings {
            path,
            encoding: Default::default(),
        },
        schema: Default::default(),
    }
}

#[tokio::test]
async fn topology_reloads_when_standalone_file_enrichment_table_added() {
    trace_init();

    let dir = tempfile::tempdir().unwrap();
    let csv_path = dir.path().join("table.csv");
    fs::write(&csv_path, "name,value\naaa,111\n").unwrap();

    let mut old_config = Config::builder();
    old_config.add_source("in", basic_source().1);
    old_config.add_sink("out", &["in"], basic_sink(1).1);

    let mut new_config = old_config.clone();
    new_config.add_enrichment_table("geo", &[], file_enrichment_table(csv_path));

    let (mut topology, _) = start_topology(old_config.build().unwrap(), false).await;

    let reloaded = topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap();

    assert!(reloaded, "enrichment table add should rebuild");
    assert!(
        topology
            .config()
            .enrichment_tables()
            .any(|(key, _)| key == &ComponentKey::from("geo"))
    );
}

#[tokio::test]
async fn topology_reloads_when_standalone_file_enrichment_table_changed() {
    trace_init();

    let dir = tempfile::tempdir().unwrap();
    let old_csv = dir.path().join("old.csv");
    let new_csv = dir.path().join("new.csv");
    fs::write(&old_csv, "name,value\naaa,111\n").unwrap();
    fs::write(&new_csv, "name,value\nbbb,222\n").unwrap();

    let mut old_config = Config::builder();
    old_config.add_source("in", basic_source().1);
    old_config.add_sink("out", &["in"], basic_sink(1).1);
    old_config.add_enrichment_table("geo", &[], file_enrichment_table(old_csv));

    let mut new_config = Config::builder();
    new_config.add_source("in", basic_source().1);
    new_config.add_sink("out", &["in"], basic_sink(1).1);
    new_config.add_enrichment_table("geo", &[], file_enrichment_table(new_csv.clone()));

    let (mut topology, _) = start_topology(old_config.build().unwrap(), false).await;

    let reloaded = topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap();

    assert!(reloaded, "enrichment table path change should rebuild");
    let geo = topology
        .config()
        .enrichment_tables()
        .find(|(key, _)| *key == &ComponentKey::from("geo"))
        .map(|(_, table)| table);
    assert!(geo.is_some());
}

#[tokio::test]
async fn topology_reloads_when_standalone_file_enrichment_table_removed() {
    trace_init();

    let dir = tempfile::tempdir().unwrap();
    let csv_path = dir.path().join("table.csv");
    fs::write(&csv_path, "name,value\naaa,111\n").unwrap();

    let mut old_config = Config::builder();
    old_config.add_source("in", basic_source().1);
    old_config.add_sink("out", &["in"], basic_sink(1).1);
    old_config.add_enrichment_table("geo", &[], file_enrichment_table(csv_path));

    let mut new_config = Config::builder();
    new_config.add_source("in", basic_source().1);
    new_config.add_sink("out", &["in"], basic_sink(1).1);

    let (mut topology, _) = start_topology(old_config.build().unwrap(), false).await;

    let reloaded = topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap();

    assert!(reloaded, "enrichment table remove should rebuild");
    assert!(
        topology
            .config()
            .enrichment_tables()
            .all(|(key, _)| key != &ComponentKey::from("geo"))
    );
}

#[tokio::test]
async fn topology_reloads_when_sink_content_changes() {
    trace_init();

    let mut old_config = Config::builder();
    old_config.add_source("in", basic_source().1);
    old_config.add_sink("out", &["in"], basic_sink_with_data(1, "v1").1);

    let mut new_config = Config::builder();
    new_config.add_source("in", basic_source().1);
    new_config.add_sink("out", &["in"], basic_sink_with_data(1, "v2").1);

    let (mut topology, _) = start_topology(old_config.build().unwrap(), false).await;

    let reloaded = topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap();

    assert!(reloaded, "sink content change should rebuild");
}

#[tokio::test]
async fn topology_reloads_when_transform_added() {
    trace_init();

    let mut old_config = Config::builder();
    old_config.add_source("in", basic_source().1);
    old_config.add_sink("out", &["in"], basic_sink(1).1);

    let mut new_config = Config::builder();
    new_config.add_source("in", basic_source().1);
    new_config.add_transform("xform", &["in"], basic_transform("-x", 0.0));
    new_config.add_sink("out", &["xform"], basic_sink(1).1);

    let (mut topology, _) = start_topology(old_config.build().unwrap(), false).await;

    let reloaded = topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap();

    assert!(reloaded, "transform add should rebuild");
    assert!(
        topology
            .config()
            .transform(&ComponentKey::from("xform"))
            .is_some()
    );
}

use std::path::Path;

use crate::{
    config::Config,
    sinks::{
        console::{ConsoleSinkConfig, Target},
        util::encoding::StandardEncodings,
    },
    sources::socket::SocketConfig,
    test_util::{next_addr, start_topology},
};

#[tokio::test]
async fn topology_doesnt_reload_new_data_dir() {
    let mut old_config = Config::builder();
    old_config.add_source("in", SocketConfig::make_basic_tcp_config(next_addr()));
    old_config.add_sink(
        "out",
        &["in"],
        ConsoleSinkConfig {
            target: Target::Stdout,
            encoding: StandardEncodings::Text.into(),
        },
    );
    old_config.global.data_dir = Some(Path::new("/asdf").to_path_buf());
    let mut new_config = old_config.clone();

    let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;

    new_config.global.data_dir = Some(Path::new("/qwerty").to_path_buf());

    topology
        .reload_config_and_respawn(new_config.build().unwrap())
        .await
        .unwrap();

    assert_eq!(
        topology.config.global.data_dir,
        Some(Path::new("/asdf").to_path_buf())
    );
}

use std::path::Path;

use crate::{
    config::Config,
    test_util::{
        mock::{basic_sink, basic_source},
        start_topology, trace_init,
    },
};

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

    topology
        .reload_config_and_respawn(new_config.build().unwrap(), Default::default())
        .await
        .unwrap();

    assert_eq!(
        topology.config.global.data_dir,
        Some(Path::new("/asdf").to_path_buf())
    );
}

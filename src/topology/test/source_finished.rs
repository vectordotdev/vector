use tokio::time::{timeout, Duration};

use crate::{
    config::Config,
    sinks::{
        console::{ConsoleSinkConfig, Target},
        util::encoding::StandardEncodings,
    },
    sources::demo_logs::DemoLogsConfig,
    test_util::start_topology,
};

#[tokio::test]
async fn sources_finished() {
    let mut old_config = Config::builder();
    let demo_logs = DemoLogsConfig::repeat(vec!["text".to_owned()], 1, 0.0);
    old_config.add_source("in", demo_logs);
    old_config.add_sink(
        "out",
        &["in"],
        ConsoleSinkConfig {
            target: Target::Stdout,
            encoding: StandardEncodings::Text.into(),
        },
    );

    let (topology, _crash) = start_topology(old_config.build().unwrap(), false).await;

    timeout(Duration::from_secs(2), topology.sources_finished())
        .await
        .unwrap();
}

use crate::{
    config::Config,
    sinks::console::{ConsoleSinkConfig, Encoding, Target},
    sources::generator::GeneratorConfig,
    test_util::start_topology,
};
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn sources_finished() {
    let mut old_config = Config::builder();
    let generator = GeneratorConfig::repeat(vec!["text".to_owned()], 1, 0.0);
    old_config.add_source("in", generator);
    old_config.add_sink(
        "out",
        &["in"],
        ConsoleSinkConfig {
            target: Target::Stdout,
            encoding: Encoding::Text.into(),
        },
    );

    let (topology, _crash) = start_topology(old_config.build().unwrap(), false).await;

    timeout(Duration::from_secs(2), topology.sources_finished())
        .await
        .unwrap();
}

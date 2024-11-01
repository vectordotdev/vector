use tokio::time::{timeout, Duration};
use vector_lib::codecs::{encoding::FramingConfig, TextSerializerConfig};

use crate::{
    config::Config,
    sinks::console::{ConsoleSinkConfig, Target},
    sources::demo_logs::DemoLogsConfig,
    test_util::{start_topology, trace_init},
};

#[tokio::test]
async fn sources_finished() {
    trace_init();

    let mut old_config = Config::builder();
    let demo_logs =
        DemoLogsConfig::repeat(vec!["text".to_owned()], 1, Duration::from_secs(0), None);
    old_config.add_source("in", demo_logs);
    old_config.add_sink(
        "out",
        &["in"],
        ConsoleSinkConfig {
            target: Target::Stdout,
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            acknowledgements: Default::default(),
        },
    );

    let (topology, _) = start_topology(old_config.build().unwrap(), false).await;

    timeout(Duration::from_secs(2), topology.sources_finished())
        .await
        .unwrap();
}

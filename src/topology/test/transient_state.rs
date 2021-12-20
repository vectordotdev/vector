use std::sync::Arc;

use futures::{future, FutureExt};
use serde::{Deserialize, Serialize};
use stream_cancel::{Trigger, Tripwire};
use tokio::sync::Mutex;

use crate::{
    config::{Config, DataType, SourceConfig, SourceContext},
    sinks::blackhole::BlackholeConfig,
    sources::{stdin::StdinConfig, Source},
    test_util::{start_topology, trace_init},
    transforms::json_parser::JsonParserConfig,
    Error,
};

#[derive(Debug, Deserialize, Serialize)]
pub struct MockSourceConfig {
    #[serde(skip)]
    tripwire: Arc<Mutex<Option<Tripwire>>>,
}

impl MockSourceConfig {
    pub fn new() -> (Trigger, Self) {
        let (trigger, tripwire) = Tripwire::new();
        (
            trigger,
            Self {
                tripwire: Arc::new(Mutex::new(Some(tripwire))),
            },
        )
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "mock")]
impl SourceConfig for MockSourceConfig {
    async fn build(&self, cx: SourceContext) -> Result<Source, Error> {
        let tripwire = self.tripwire.lock().await;

        let out = cx.out;
        Ok(Box::pin(
            future::select(
                cx.shutdown.map(|_| ()).boxed(),
                tripwire
                    .clone()
                    .unwrap()
                    .then(crate::stream::tripwire_handler)
                    .boxed(),
            )
            .map(|_| std::mem::drop(out))
            .unit_error(),
        ))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "mock"
    }
}

#[tokio::test]
async fn closed_source() {
    let mut old_config = Config::builder();
    let (trigger_old, source) = MockSourceConfig::new();
    old_config.add_source("in", source);
    old_config.add_transform(
        "trans",
        &["in"],
        JsonParserConfig {
            drop_field: true,
            ..JsonParserConfig::default()
        },
    );
    old_config.add_sink(
        "out1",
        &["trans"],
        BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        },
    );
    old_config.add_sink(
        "out2",
        &["trans"],
        BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        },
    );

    let mut new_config = Config::builder();
    let (_trigger_new, source) = MockSourceConfig::new();
    new_config.add_source("in", source);
    new_config.add_transform(
        "trans",
        &["in"],
        JsonParserConfig {
            drop_field: false,
            ..JsonParserConfig::default()
        },
    );
    new_config.add_sink(
        "out1",
        &["trans"],
        BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        },
    );

    let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;

    trigger_old.cancel();

    topology.sources_finished().await;

    assert!(topology
        .reload_config_and_respawn(new_config.build().unwrap())
        .await
        .unwrap());
}

#[tokio::test]
async fn remove_sink() {
    trace_init();

    let mut old_config = Config::builder();
    old_config.add_source("in", StdinConfig::default());
    old_config.add_transform(
        "trans",
        &["in"],
        JsonParserConfig {
            drop_field: true,
            ..JsonParserConfig::default()
        },
    );
    old_config.add_sink(
        "out1",
        &["trans"],
        BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        },
    );
    old_config.add_sink(
        "out2",
        &["trans"],
        BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        },
    );

    let mut new_config = Config::builder();
    new_config.add_source("in", StdinConfig::default());
    new_config.add_transform(
        "trans",
        &["in"],
        JsonParserConfig {
            drop_field: false,
            ..JsonParserConfig::default()
        },
    );
    new_config.add_sink(
        "out1",
        &["trans"],
        BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        },
    );

    let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
    assert!(topology
        .reload_config_and_respawn(new_config.build().unwrap())
        .await
        .unwrap());
}

#[tokio::test]
async fn remove_transform() {
    trace_init();

    let mut old_config = Config::builder();
    old_config.add_source("in", StdinConfig::default());
    old_config.add_transform(
        "trans1",
        &["in"],
        JsonParserConfig {
            drop_field: true,
            ..JsonParserConfig::default()
        },
    );
    old_config.add_transform(
        "trans2",
        &["trans1"],
        JsonParserConfig {
            drop_field: true,
            ..JsonParserConfig::default()
        },
    );
    old_config.add_sink(
        "out1",
        &["trans1"],
        BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        },
    );
    old_config.add_sink(
        "out2",
        &["trans2"],
        BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        },
    );

    let mut new_config = Config::builder();
    new_config.add_source("in", StdinConfig::default());
    new_config.add_transform(
        "trans1",
        &["in"],
        JsonParserConfig {
            drop_field: false,
            ..JsonParserConfig::default()
        },
    );
    new_config.add_sink(
        "out1",
        &["trans1"],
        BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        },
    );

    let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
    assert!(topology
        .reload_config_and_respawn(new_config.build().unwrap())
        .await
        .unwrap());
}

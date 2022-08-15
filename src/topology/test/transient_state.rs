use crate::{
    config::Config,
    sinks::blackhole::BlackholeConfig,
    test_util::{
        mock::{basic_sink, basic_source, transforms::BasicTransformConfig, tripwire_source},
        start_topology, trace_init,
    },
};

#[tokio::test]
async fn closed_source() {
    trace_init();

    let mut old_config = Config::builder();
    let (trigger_old, source) = tripwire_source();
    old_config.add_source("in", source);
    old_config.add_transform(
        "trans",
        &["in"],
        BasicTransformConfig::new("a".to_string(), 0.0),
    );
    old_config.add_sink("out1", &["trans"], basic_sink(1).1);
    old_config.add_sink("out2", &["trans"], basic_sink(1).1);

    let mut new_config = Config::builder();
    let (_trigger_new, source) = tripwire_source();
    new_config.add_source("in", source);
    new_config.add_transform(
        "trans",
        &["in"],
        BasicTransformConfig::new("a".to_string(), 0.0),
    );
    new_config.add_sink("out1", &["trans"], basic_sink(1).1);

    let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;

    trigger_old.cancel();

    topology.sources_finished().await;

    assert!(topology
        .reload_config_and_respawn(new_config.build().unwrap())
        .await
        .unwrap());
}

#[test]
fn remove_sink() {
    trace_init();

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed building the Runtime")
        .block_on(async {
            let mut old_config = Config::builder();
            old_config.add_source("in", basic_source().1);
            old_config.add_transform(
                "trans",
                &["in"],
                BasicTransformConfig::new("a".to_string(), 0.0),
            );
            old_config.add_sink(
                "out1",
                &["trans"],
                BlackholeConfig {
                    print_interval_secs: 10,
                    rate: None,
                    acknowledgements: Default::default(),
                },
            );
            old_config.add_sink(
                "out2",
                &["trans"],
                BlackholeConfig {
                    print_interval_secs: 10,
                    rate: None,
                    acknowledgements: Default::default(),
                },
            );

            let mut new_config = Config::builder();
            new_config.add_source("in", basic_source().1);
            new_config.add_transform(
                "trans",
                &["in"],
                BasicTransformConfig::new("b".to_string(), 0.0),
            );
            new_config.add_sink(
                "out1",
                &["trans"],
                BlackholeConfig {
                    print_interval_secs: 10,
                    rate: None,
                    acknowledgements: Default::default(),
                },
            );

            let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
            assert!(topology
                .reload_config_and_respawn(new_config.build().unwrap())
                .await
                .unwrap());

            println!("got here");
        });
}

#[tokio::test]
async fn remove_transform() {
    trace_init();

    let mut old_config = Config::builder();
    old_config.add_source("in", basic_source().1);
    old_config.add_transform(
        "trans1",
        &["in"],
        BasicTransformConfig::new("a".to_string(), 0.0),
    );
    old_config.add_transform(
        "trans2",
        &["trans1"],
        BasicTransformConfig::new("a".to_string(), 0.0),
    );
    old_config.add_sink("out1", &["trans1"], basic_sink(1).1);
    old_config.add_sink("out2", &["trans2"], basic_sink(1).1);

    let mut new_config = Config::builder();
    new_config.add_source("in", basic_source().1);
    new_config.add_transform(
        "trans1",
        &["in"],
        BasicTransformConfig::new("b".to_string(), 0.0),
    );
    new_config.add_sink("out1", &["trans1"], basic_sink(1).1);

    let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
    assert!(topology
        .reload_config_and_respawn(new_config.build().unwrap())
        .await
        .unwrap());
}

#[tokio::test]
async fn replace_transform() {
    trace_init();

    // Create a simple source/transform/sink topology:
    let mut old_config = Config::builder();
    old_config.add_source("in", basic_source().1);
    old_config.add_transform(
        "trans1",
        &["in"],
        BasicTransformConfig::new("a".to_string(), 0.0),
    );
    old_config.add_sink("out1", &["trans1"], basic_sink(1).1);

    // Now create the same simple source/transform/sink topology, but change the transform so it has
    // to be rebuilt:
    let mut new_config = Config::builder();
    new_config.add_source("in", basic_source().1);
    new_config.add_transform(
        "trans1",
        &["in"],
        BasicTransformConfig::new("b".to_string(), 0.0),
    );
    new_config.add_sink("out1", &["trans1"], basic_sink(1).1);

    let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
    assert!(topology
        .reload_config_and_respawn(new_config.build().unwrap())
        .await
        .unwrap());
}

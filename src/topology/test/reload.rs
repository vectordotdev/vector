use std::{
    net::{SocketAddr, TcpListener},
    time::Duration,
};

use futures::StreamExt;
use tokio::time::sleep;
use tokio_stream::wrappers::UnboundedReceiverStream;
use vector_buffers::{BufferConfig, BufferType, WhenFull};
use vector_core::config::ComponentKey;

use crate::{
    config::Config,
    sinks::{
        console::{ConsoleSinkConfig, Target},
        prometheus::exporter::PrometheusExporterConfig,
        util::encoding::{EncodingConfig, StandardEncodings},
    },
    sources::{demo_logs::DemoLogsConfig, splunk_hec::SplunkConfig},
    test_util::{next_addr, start_topology, temp_dir, wait_for_tcp},
    transforms::log_to_metric::{GaugeConfig, LogToMetricConfig, MetricConfig},
};

fn splunk_source_config(addr: SocketAddr) -> SplunkConfig {
    let mut config = SplunkConfig::default();
    config.address = addr;
    config
}

#[tokio::test]
async fn topology_reuse_old_port() {
    let address = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in1", splunk_source_config(address));
    old_config.add_sink(
        "out",
        &["in1"],
        ConsoleSinkConfig {
            target: Target::Stdout,
            encoding: EncodingConfig::from(StandardEncodings::Text).into(),
            acknowledgements: Default::default(),
        },
    );

    let mut new_config = Config::builder();
    new_config.add_source("in2", splunk_source_config(address));
    new_config.add_sink(
        "out",
        &["in2"],
        ConsoleSinkConfig {
            target: Target::Stdout,
            encoding: EncodingConfig::from(StandardEncodings::Text).into(),
            acknowledgements: Default::default(),
        },
    );

    let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
    assert!(topology
        .reload_config_and_respawn(new_config.build().unwrap())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_rebuild_old() {
    let address_0 = next_addr();
    let address_1 = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in1", splunk_source_config(address_0));
    old_config.add_sink(
        "out",
        &["in1"],
        ConsoleSinkConfig {
            target: Target::Stdout,
            encoding: EncodingConfig::from(StandardEncodings::Text).into(),
            acknowledgements: Default::default(),
        },
    );

    let mut new_config = Config::builder();
    new_config.add_source("in1", splunk_source_config(address_1));
    new_config.add_sink(
        "out",
        &["in1"],
        ConsoleSinkConfig {
            target: Target::Stdout,
            encoding: EncodingConfig::from(StandardEncodings::Text).into(),
            acknowledgements: Default::default(),
        },
    );

    // Will cause the new_config to fail on build
    let _bind = TcpListener::bind(address_1).unwrap();

    let (mut topology, _crash) = start_topology(old_config.build().unwrap(), false).await;
    assert!(!topology
        .reload_config_and_respawn(new_config.build().unwrap())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_old() {
    let address = next_addr();

    let mut old_config = Config::builder();
    old_config.add_source("in1", splunk_source_config(address));
    old_config.add_sink(
        "out",
        &["in1"],
        ConsoleSinkConfig {
            target: Target::Stdout,
            encoding: EncodingConfig::from(StandardEncodings::Text).into(),
            acknowledgements: Default::default(),
        },
    );

    let (mut topology, _crash) = start_topology(old_config.clone().build().unwrap(), false).await;
    assert!(topology
        .reload_config_and_respawn(old_config.build().unwrap())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_reuse_old_port_sink() {
    let address = next_addr();

    let source = DemoLogsConfig::repeat(vec!["msg".to_string()], usize::MAX, 0.001);
    let transform = LogToMetricConfig {
        metrics: vec![MetricConfig::Gauge(GaugeConfig {
            field: "message".to_string(),
            name: None,
            namespace: None,
            tags: None,
        })],
    };

    let mut old_config = Config::builder();
    old_config.add_source("in", source.clone());
    old_config.add_transform("trans", &["in"], transform.clone());
    old_config.add_sink(
        "out1",
        &["trans"],
        PrometheusExporterConfig {
            address,
            flush_period_secs: Duration::from_secs(1),
            ..PrometheusExporterConfig::default()
        },
    );

    let mut new_config = Config::builder();
    new_config.add_source("in", source.clone());
    new_config.add_transform("trans", &["in"], transform.clone());
    new_config.add_sink(
        "out1",
        &["trans"],
        PrometheusExporterConfig {
            address,
            flush_period_secs: Duration::from_secs(2),
            ..PrometheusExporterConfig::default()
        },
    );

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
    // Reload with source that uses address of changed sink.
    let address_0 = next_addr();
    let address_1 = next_addr();

    let transform = LogToMetricConfig {
        metrics: vec![MetricConfig::Gauge(GaugeConfig {
            field: "message".to_string(),
            name: None,
            namespace: None,
            tags: None,
        })],
    };

    let mut old_config = Config::builder();
    old_config.add_source(
        "in",
        DemoLogsConfig::repeat(vec!["msg".to_string()], usize::MAX, 0.001),
    );
    old_config.add_transform("trans", &["in"], transform.clone());
    old_config.add_sink(
        "out1",
        &["trans"],
        PrometheusExporterConfig {
            address: address_0,
            flush_period_secs: Duration::from_secs(1),
            ..PrometheusExporterConfig::default()
        },
    );

    let mut new_config = Config::builder();
    new_config.add_source("in", splunk_source_config(address_0));
    new_config.add_transform("trans", &["in"], transform.clone());
    new_config.add_sink(
        "out1",
        &["trans"],
        PrometheusExporterConfig {
            address: address_1,
            flush_period_secs: Duration::from_secs(1),
            ..PrometheusExporterConfig::default()
        },
    );

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
    let address_0 = next_addr();
    let address_1 = next_addr();
    let data_dir = temp_dir();
    std::fs::create_dir(&data_dir).unwrap();

    let mut old_config = Config::builder();
    old_config.global.data_dir = Some(data_dir);
    old_config.add_source(
        "in",
        DemoLogsConfig::repeat(vec!["msg".to_string()], usize::MAX, 0.001),
    );
    old_config.add_transform(
        "trans",
        &["in"],
        LogToMetricConfig {
            metrics: vec![MetricConfig::Gauge(GaugeConfig {
                field: "message".to_string(),
                name: None,
                namespace: None,
                tags: None,
            })],
        },
    );
    old_config.add_sink(
        "out",
        &["trans"],
        PrometheusExporterConfig {
            address: address_0,
            flush_period_secs: Duration::from_secs(1),
            ..PrometheusExporterConfig::default()
        },
    );

    let sink_key = ComponentKey::from("out");
    old_config.sinks[&sink_key].buffer = BufferConfig {
        stages: vec![BufferType::DiskV1 {
            max_size: std::num::NonZeroU64::new(1024).unwrap(),
            when_full: WhenFull::Block,
        }],
    };

    let mut new_config = old_config.clone();
    new_config.sinks[&sink_key].inner = Box::new(PrometheusExporterConfig {
        address: address_1,
        flush_period_secs: Duration::from_secs(1),
        ..PrometheusExporterConfig::default()
    });
    new_config.sinks[&sink_key].buffer = BufferConfig {
        stages: vec![BufferType::DiskV1 {
            max_size: std::num::NonZeroU64::new(1024).unwrap(),
            when_full: WhenFull::Block,
        }],
    };

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
    // This specifically exercises that we can add new components -- no changed or removed
    // components -- via the reload mechanism and without any issues.
    let address1 = next_addr();
    let address2 = next_addr();

    let source = DemoLogsConfig::repeat(vec!["msg".to_string()], usize::MAX, 0.001);
    let transform = LogToMetricConfig {
        metrics: vec![MetricConfig::Gauge(GaugeConfig {
            field: "message".to_string(),
            name: None,
            namespace: None,
            tags: None,
        })],
    };

    let mut old_config = Config::builder();
    old_config.add_source("in1", source.clone());
    old_config.add_transform("trans1", &["in1"], transform.clone());
    old_config.add_sink(
        "out1",
        &["trans1"],
        PrometheusExporterConfig {
            address: address1,
            flush_period_secs: Duration::from_secs(1),
            ..PrometheusExporterConfig::default()
        },
    );

    let mut new_config = Config::builder();
    new_config.add_source("in1", source.clone());
    new_config.add_transform("trans1", &["in1"], transform.clone());
    new_config.add_sink(
        "out1",
        &["trans1"],
        PrometheusExporterConfig {
            address: address1,
            flush_period_secs: Duration::from_secs(1),
            ..PrometheusExporterConfig::default()
        },
    );
    new_config.add_source("in2", source.clone());
    new_config.add_transform("trans2", &["in2"], transform.clone());
    new_config.add_sink(
        "out2",
        &["trans2"],
        PrometheusExporterConfig {
            address: address2,
            flush_period_secs: Duration::from_secs(1),
            ..PrometheusExporterConfig::default()
        },
    );

    reload_sink_test(
        old_config.build().unwrap(),
        new_config.build().unwrap(),
        address1,
        address2,
    )
    .await;
}

async fn reload_sink_test(
    old_config: Config,
    new_config: Config,
    old_address: SocketAddr,
    new_address: SocketAddr,
) {
    let (mut topology, crash) = start_topology(old_config, false).await;
    let mut crash_stream = UnboundedReceiverStream::new(crash);

    // Wait for sink to come online
    wait_for_tcp(old_address).await;

    // Give topology some time to run
    sleep(Duration::from_secs(1)).await;

    assert!(topology
        .reload_config_and_respawn(new_config)
        .await
        .unwrap());

    // Give old time to shutdown if it didn't, and new one to come online.
    sleep(Duration::from_secs(2)).await;

    tokio::select! {
        _ = wait_for_tcp(new_address) => {}//Success
        _ = crash_stream.next() => panic!(),
    }
}

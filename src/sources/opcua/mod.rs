use metrics::gauge;
use vector_common::internal_event::{CountByteSize, EventsReceived, InternalEventHandle};
use vector_common::shutdown::ShutdownSignal;
use vector_config::configurable_component;
use opcua::client::prelude::*;
use opcua::crypto::SecurityPolicy;
use opcua::types::{MessageSecurityMode, TimestampsToReturn, UserTokenPolicy};
use std::{
    str::FromStr,
    sync::{Arc},
};
use futures::executor;
use tokio::sync::mpsc;
use futures_util::FutureExt;
use opcua::sync::{Mutex, RwLock};
use tokio::pin;
use tokio::sync::oneshot;
use tokio::time::sleep;
use crate::{
    config::{GenerateConfig, SourceConfig, SourceContext, SourceOutput},
    event::metric::MetricValue,
    SourceSender,
    internal_events::{
        OpcUaBytesReceived
    },
};

use vector_core::{config::LogNamespace, EstimatedJsonEncodedSizeOf, metric_tags};
use vector_core::event::{MetricKind, Metric};
use crate::internal_events::StreamClosedError;

macro_rules! gauge {
    ($value:expr) => {
        MetricValue::Gauge {
            value: $value as f64,
        }
    };
}

/// Configuration for the `opcua` source.
#[configurable_component(source(
"opcua",
"Read metrics data from OPC UA."
))]
#[serde(deny_unknown_fields)]
#[derive(Clone, Debug, Derivative)]
#[configurable(metadata(status = "beta"))]
pub struct OpcUaSourceConfig {
    /// The OPC/UA URL to connect to.
    ///
    /// The URL takes the form of `opc.tcp://server:port`.
    #[configurable(metadata(docs::examples = "opc.tcp://localhost:4840"))]
    url: String,

    /// The application URI to use when connecting to the server.
    #[configurable(metadata(docs::examples = "urn:example:client"))]
    application_uri: String,

    /// The product URI to use when connecting to the server.
    #[configurable(metadata(docs::examples = "product_uri"))]
    product_uri: String,

    /// Whether to trust the server's certificate.
    #[serde(default)]
    pub trust_server_certs: bool,

    /// Whether to create a sample keypair if one is not found.
    #[serde(default)]
    pub create_sample_keypair: bool,

    /// The node ids to monitor.
    pub node_ids: Vec<NodeIdConfig>,
}

/// Configuration for a node id to monitor
#[configurable_component]
#[derive(Clone, Debug)]
pub struct NodeIdConfig {
    /// The node id to monitor.
    #[configurable(metadata(docs::examples = "ns=2;s=Demo.Static.Scalar.UInt32"))]
    pub node_id: String,

    /// The metric name to use for the node id.
    #[configurable(metadata(docs::examples = "demo_val"))]
    pub metric_name: String,
}

impl GenerateConfig for OpcUaSourceConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            url: "opc.tcp://localhost:4855".to_string(),
            application_uri: "urn:SimpleClient".to_string(),
            product_uri: "urn:SimpleClient".to_string(),
            trust_server_certs: None,
            create_sample_keypair: None,
            node_ids: vec![],
        })
            .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "opcua")]
impl SourceConfig for OpcUaSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let shutdown = cx.shutdown;
        let out = cx.out;

        Ok(Box::pin(opcua_source(self.clone(), shutdown, out)))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

async fn opcua_source(config: OpcUaSourceConfig, shutdown: ShutdownSignal, out: SourceSender) -> Result<(), ()> {
    let mut session_shutdown = shutdown.clone().fuse();
    let (sender, mut receiver) = mpsc::channel::<Vec<Metric>>(1);
    let url = config.url.clone();
    let node_ids = config.node_ids.clone();

    tokio::spawn(async move {
        let mut client = ClientBuilder::new()
            .application_name("Vector Client")
            .application_uri(config.application_uri)
            .product_uri(config.product_uri)
            .trust_server_certs(config.trust_server_certs.unwrap_or(false))
            .create_sample_keypair(config.create_sample_keypair.unwrap_or(false))
            .session_retry_limit(3)
            .client()
            .unwrap();

        let (term_tx, term_rx) = oneshot::channel();

        tokio::task::spawn_blocking(move || {
            if let Ok(session) = client.connect_to_endpoint(
                (
                    url.as_ref(),
                    SecurityPolicy::None.to_str(),
                    MessageSecurityMode::None,
                    UserTokenPolicy::anonymous(),
                ),
                IdentityToken::Anonymous,
            ) {
                _ = term_tx.send(session);
            } else {
                drop(term_tx);
            }
        });

        let session = match term_rx.await {
            Ok(session) => session,
            Err(_) => {
                panic!("Failed to connect to endpoint");
            }
        };

        if let Err(result) = subscribe_to_variables(node_ids, session.clone(), sender) {
            panic!(
                "ERROR: Got an error while subscribing to variables - {}",
                result
            );
        } else {
            let mut session_tx = Some(Session::run_async(session));
            let mut out = out;

            loop {
                tokio::select! {
                    _ = &mut session_shutdown => {
                        break;
                    }

                    events = receiver.recv() => {
                        if let Some(events) = events {
                            let len = events.len();

                            let byte_size = events.estimated_json_encoded_size_of();
                            let events_received = register!(EventsReceived);
                            events_received.emit(CountByteSize(len, byte_size));


                            emit!(OpcUaBytesReceived{
                                byte_size,
                                protocol: "opcua",
                            });

                            if let Err(error) = out.send_batch(events).await {
                                emit!(StreamClosedError { error, count:len });
                            }
                        }
                    }
                }
            }

            if let Some(tx) = session_tx.take() {
                let _ = tx.send(SessionCommand::Stop);
            }
        }
    });

    shutdown.await;

    Ok(())
}

fn subscribe_to_variables(node_ids: Vec<NodeIdConfig>, session: Arc<RwLock<Session>>, tx: mpsc::Sender<Vec<Metric>>) -> Result<(), StatusCode> {
    let session = session.read();
    let (tx_inner, mut rx_inner) = mpsc::channel::<(NodeId, DataValue, DateTime)>(1);
    let tx_inner = Arc::new(Mutex::new(tx_inner));

    let subscription_id = session.create_subscription(
        100.0,
        10,
        30,
        0,
        0,
        true,
        DataChangeCallback::new(move |changed_monitored_items| {
            let tx_inner = tx_inner.lock();

            changed_monitored_items.iter().for_each(|item| {
                item.values().iter().for_each(|value| {
                    let node_id = item.item_to_monitor().node_id.clone();
                    let value = value.clone();
                    let server_timestamp = value.server_timestamp.unwrap();

                    let _ = executor::block_on(tx_inner.send((node_id, value, server_timestamp)));
                });
            });
        }),
    )?;

    let monitored_node_ids: Vec<MonitoredItemCreateRequest> = node_ids.iter()
        .map(|node_id_str| {
            let node_id = NodeId::from_str(node_id_str.node_id.as_str()).unwrap();
            node_id.into()
        })
        .collect();

    session.create_monitored_items(
        subscription_id,
        TimestampsToReturn::Both,
        &monitored_node_ids,
    ).expect("Failed to create monitored items");

    tokio::spawn(async move {
        let sleep = sleep(tokio::time::Duration::from_secs(1));
        pin!(sleep);
        let mut items = Vec::new();

        loop {
            tokio::select! {
                changed_monitored_items = rx_inner.recv() => {
                    items.clear();
                    items.push(changed_monitored_items.unwrap());
                },
                () = &mut sleep => {
                    sleep.as_mut().reset(tokio::time::Instant::now() + tokio::time::Duration::from_secs(1));

                    let events = publish_items(node_ids.clone(), items.clone());
                    let _ = tx.send(events).await;
                }
            }
        }
    });

    Ok(())
}

fn publish_items(node_ids: Vec<NodeIdConfig>, items: Vec<(NodeId, DataValue, DateTime)>) -> Vec<Metric> {
    let count = items.len();
    let mut events = Vec::with_capacity(count);

    for item in items.iter() {
        let (node_id, data_value, timestamp) = item;

        let node_id = node_id.to_string();

        let node_config = node_ids.iter().find(|item_node_id| item_node_id.node_id == node_id);

        let data_value = data_value.clone();
        let data_value = data_value.value.unwrap().as_f64().unwrap();
        let metric = Metric::new(&node_config.unwrap().metric_name, MetricKind::Absolute, gauge!(data_value))
            .with_timestamp(Some(timestamp.as_chrono()))
            .with_namespace(Some("opcua"))
            .with_tags(Some(metric_tags!("nodeId" => node_id)));

        events.push(metric);
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<OpcUaSourceConfig>();
    }
}

/// #[cfg(feature = "opcua-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use tokio::time::{Instant, timeout};
    use vector_common::config::ComponentKey;
    use crate::test_util::collect_n;
    use crate::test_util::components::{
        assert_source_compliance,
        SOURCE_TAGS};
    use super::*;

    #[tokio::test]
    async fn test_connection_with_metric_change() {
        let config = OpcUaSourceConfig {
            url: "opc.tcp://127.0.0.1:4840/UA".to_string(),
            application_uri: "urn:SimpleClient".to_string(),
            product_uri: "urn:SimpleClient".to_string(),
            trust_server_certs: None,
            create_sample_keypair: None,
            node_ids: vec![NodeIdConfig {
                metric_name: "SpindleOverride".to_string(),
                node_id: "ns=34;i=6099".to_string(),
            }, NodeIdConfig {
                metric_name: "FeedSpeed".to_string(),
                node_id: "ns=34;i=6083".to_string(),
            }],
        };


        let events = assert_source_compliance(&SOURCE_TAGS, async move {
            let (tx, rx) = SourceSender::new_test();

            let source_id = ComponentKey::from("opcua");
            let (sc, shutdown) = SourceContext::new_shutdown(&source_id, tx);

            let source = config.build(sc).await.expect("failed to start opcua source");

            tokio::spawn(source);

            let events = timeout(std::time::Duration::from_secs(20), collect_n(rx, 10))
                .await
                .unwrap();

            tokio::task::yield_now().await;

            shutdown.shutdown_all(Some(Instant::now())).await;

            return events;
        }).await;

        assert_eq!(events.len(), 10);
    }
}

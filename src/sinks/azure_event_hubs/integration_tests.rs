//! Integration tests for the Azure Event Hubs sink.
//!
//! Requires the Azure Event Hubs emulator running via Docker Compose.
//! Run with: `cargo test --features azure-event-hubs-integration-tests`

use std::time::Duration;

use azure_messaging_eventhubs::{ConsumerClient, OpenReceiverOptions, StartLocation, StartPosition};
use futures_util::StreamExt;
use vector_lib::{
    codecs::TextSerializerConfig,
    event::{BatchNotifier, BatchStatus},
};

use super::config::AzureEventHubsSinkConfig;
use crate::{
    sinks::prelude::*,
    test_util::{
        components::{SINK_TAGS, assert_sink_compliance},
        random_lines_with_stream,
    },
};

/// The Event Hub entity name configured in eventhubs-config.json.
const EVENT_HUB_NAME: &str = "eh1";
/// The consumer group configured in eventhubs-config.json.
const CONSUMER_GROUP: &str = "cg1";

fn emulator_address() -> String {
    std::env::var("EVENTHUBS_ADDRESS").unwrap_or_else(|_| "localhost".to_string())
}

/// The emulator uses a well-known SAS key.
fn emulator_connection_string() -> String {
    let address = emulator_address();
    format!(
        "Endpoint=sb://{address};SharedAccessKeyName=RootManageSharedAccessKey;\
         SharedAccessKey=SAS_KEY_VALUE;UseDevelopmentEmulator=true;EntityPath={EVENT_HUB_NAME}"
    )
}

fn make_config() -> AzureEventHubsSinkConfig {
    AzureEventHubsSinkConfig {
        connection_string: Some(emulator_connection_string().into()),
        namespace: None,
        event_hub_name: Some(EVENT_HUB_NAME.to_string()),
        encoding: TextSerializerConfig::default().into(),
        request: TowerRequestConfig::default(),
        acknowledgements: Default::default(),
    }
}

#[tokio::test]
async fn azure_event_hubs_sink_healthcheck() {
    crate::test_util::trace_init();
    let config = make_config();

    let (namespace, event_hub_name, credential, custom_endpoint) =
        crate::sources::azure_event_hubs::build_credential(
            config.connection_string.as_ref(),
            config.namespace.as_deref(),
            config.event_hub_name.as_deref(),
        )
        .unwrap();

    let mut builder = azure_messaging_eventhubs::ProducerClient::builder();
    if let Some(endpoint) = custom_endpoint {
        builder = builder.with_custom_endpoint(endpoint);
    }
    let client = builder
        .open(&namespace, &event_hub_name, credential)
        .await
        .expect("Failed to create producer for healthcheck");

    client
        .get_eventhub_properties()
        .await
        .expect("Healthcheck failed");
}

#[tokio::test]
async fn azure_event_hubs_sink_happy_path() {
    crate::test_util::trace_init();
    let config = make_config();

    let num_events = 10;
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (input_lines, events) = random_lines_with_stream(100, num_events, Some(batch));

    // Build and run the sink
    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = config.build(cx).await.expect("Failed to build sink");
        sink.run(events).await
    })
    .await
    .expect("Running sink failed");

    // Verify acknowledgements
    assert_eq!(
        receiver.try_recv(),
        Ok(BatchStatus::Delivered),
        "Events should be acknowledged as delivered"
    );

    // Read events back from the emulator using a ConsumerClient
    let (namespace, _, credential, custom_endpoint) =
        crate::sources::azure_event_hubs::build_credential(
            Some(&emulator_connection_string().into()),
            None,
            Some(EVENT_HUB_NAME),
        )
        .unwrap();

    let mut builder = ConsumerClient::builder()
        .with_consumer_group(CONSUMER_GROUP.to_string());
    if let Some(endpoint) = custom_endpoint {
        builder = builder.with_custom_endpoint(endpoint);
    }
    let consumer = builder
        .open(&namespace, EVENT_HUB_NAME.to_string(), credential)
        .await
        .expect("Failed to create consumer");

    let mut received = Vec::new();
    // Read from both partitions
    for partition_id in &["0", "1"] {
        let options = OpenReceiverOptions {
            start_position: Some(StartPosition {
                location: StartLocation::Earliest,
                ..Default::default()
            }),
            ..Default::default()
        };

        let receiver = consumer
            .open_receiver_on_partition(partition_id.to_string(), Some(options))
            .await
            .expect("Failed to open receiver");

        let mut stream = receiver.stream_events();
        // Read with a timeout
        loop {
            match tokio::time::timeout(Duration::from_secs(5), stream.next()).await {
                Ok(Some(Ok(event))) => {
                    if let Some(body) = event.event_data().body() {
                        received.push(String::from_utf8_lossy(body).to_string());
                    }
                }
                _ => break,
            }
        }
    }

    assert_eq!(
        received.len(),
        num_events,
        "Expected {num_events} events, got {}",
        received.len()
    );

    // Verify all input lines are present (order may differ across partitions)
    let mut received_sorted = received.clone();
    received_sorted.sort();
    let mut expected_sorted = input_lines.clone();
    expected_sorted.sort();
    assert_eq!(received_sorted, expected_sorted);
}

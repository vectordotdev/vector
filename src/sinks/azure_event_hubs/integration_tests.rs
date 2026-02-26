//! Integration tests for the Azure Event Hubs sink.
//!
//! Requires the Azure Event Hubs emulator running via Docker Compose.
//! Run with: `cargo test --features azure-event-hubs-integration-tests`

use std::time::Duration;

use azure_messaging_eventhubs::{
    ConsumerClient, OpenReceiverOptions, StartLocation, StartPosition,
};
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

/// The consumer group configured in eventhubs-config.json.
const CONSUMER_GROUP: &str = "cg1";

fn emulator_address() -> String {
    std::env::var("EVENTHUBS_ADDRESS").unwrap_or_else(|_| "localhost".to_string())
}

/// The emulator uses a well-known SAS key.
fn emulator_connection_string(event_hub_name: &str) -> String {
    let address = emulator_address();
    format!(
        "Endpoint=sb://{address};SharedAccessKeyName=RootManageSharedAccessKey;\
         SharedAccessKey=SAS_KEY_VALUE;UseDevelopmentEmulator=true;EntityPath={event_hub_name}"
    )
}

fn make_config(event_hub_name: &str) -> AzureEventHubsSinkConfig {
    AzureEventHubsSinkConfig {
        connection_string: Some(emulator_connection_string(event_hub_name).into()),
        namespace: None,
        event_hub_name: Some(event_hub_name.to_string()),
        partition_id_field: None,
        batch_enabled: true,
        batch_max_events: 100,
        batch_timeout_secs: 1,
        rate_limit_duration_secs: 1,
        rate_limit_num: i64::MAX as u64,
        retry_max_retries: 8,
        retry_initial_delay_ms: 200,
        retry_max_elapsed_secs: 60,
        encoding: TextSerializerConfig::default().into(),
        acknowledgements: Default::default(),
    }
}

/// Read all events from all partitions of the given Event Hub.
async fn read_all_events(event_hub_name: &str) -> Vec<String> {
    // Allow the emulator time to commit events before reading
    tokio::time::sleep(Duration::from_secs(1)).await;

    let (namespace, _, credential, custom_endpoint) =
        crate::sources::azure_event_hubs::build_credential(
            Some(&emulator_connection_string(event_hub_name).into()),
            None,
            Some(event_hub_name),
        )
        .unwrap();

    let mut builder = ConsumerClient::builder().with_consumer_group(CONSUMER_GROUP.to_string());
    if let Some(endpoint) = custom_endpoint {
        builder = builder.with_custom_endpoint(endpoint);
    }
    let consumer = builder
        .open(&namespace, event_hub_name.to_string(), credential)
        .await
        .expect("Failed to create consumer");

    let mut received = Vec::new();
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
        while let Ok(Some(Ok(event))) =
            tokio::time::timeout(Duration::from_secs(5), stream.next()).await
        {
            if let Some(body) = event.event_data().body() {
                received.push(String::from_utf8_lossy(body).to_string());
            }
        }
    }
    received
}

/// Read events from a specific partition of the given Event Hub.
async fn read_events_from_partition(event_hub_name: &str, partition: &str) -> Vec<String> {
    // Allow the emulator time to commit events before reading
    tokio::time::sleep(Duration::from_secs(1)).await;

    let (namespace, _, credential, custom_endpoint) =
        crate::sources::azure_event_hubs::build_credential(
            Some(&emulator_connection_string(event_hub_name).into()),
            None,
            Some(event_hub_name),
        )
        .unwrap();

    let mut builder = ConsumerClient::builder().with_consumer_group(CONSUMER_GROUP.to_string());
    if let Some(endpoint) = custom_endpoint {
        builder = builder.with_custom_endpoint(endpoint);
    }
    let consumer = builder
        .open(&namespace, event_hub_name.to_string(), credential)
        .await
        .expect("Failed to create consumer");

    let options = OpenReceiverOptions {
        start_position: Some(StartPosition {
            location: StartLocation::Earliest,
            ..Default::default()
        }),
        ..Default::default()
    };
    let receiver = consumer
        .open_receiver_on_partition(partition.to_string(), Some(options))
        .await
        .expect("Failed to open receiver");
    let mut stream = receiver.stream_events();
    let mut received = Vec::new();
    while let Ok(Some(Ok(event))) =
        tokio::time::timeout(Duration::from_secs(5), stream.next()).await
    {
        if let Some(body) = event.event_data().body() {
            received.push(String::from_utf8_lossy(body).to_string());
        }
    }
    received
}

#[tokio::test]
async fn azure_event_hubs_sink_healthcheck() {
    crate::test_util::trace_init();
    let config = make_config("eh-happypath"); // shares hub; healthcheck only reads metadata

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
async fn azure_event_hubs_sink_non_batch_mode() {
    crate::test_util::trace_init();
    let mut config = make_config("eh-nonbatch");
    config.batch_enabled = false;

    let num_events = 5;
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (input_lines, events) = random_lines_with_stream(100, num_events, Some(batch));

    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = config.build(cx).await.expect("Failed to build sink");
        sink.run(events).await
    })
    .await
    .expect("Running sink failed");

    assert_eq!(
        receiver.try_recv(),
        Ok(BatchStatus::Delivered),
        "Events should be acknowledged as delivered"
    );

    let received = read_all_events("eh-nonbatch").await;

    assert!(
        received.len() >= num_events,
        "Expected at least {num_events} events in non-batch mode, got {}",
        received.len()
    );

    for line in &input_lines {
        assert!(
            received.contains(line),
            "Missing input line in received events: {line}"
        );
    }
}

#[tokio::test]
async fn azure_event_hubs_sink_happy_path() {
    crate::test_util::trace_init();
    let config = make_config("eh-happypath");

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

    let received = read_all_events("eh-happypath").await;

    assert!(
        received.len() >= num_events,
        "Expected at least {num_events} events, got {}",
        received.len()
    );

    for line in &input_lines {
        assert!(
            received.contains(line),
            "Missing input line in received events: {line}"
        );
    }
}

#[tokio::test]
async fn azure_event_hubs_sink_partition_routing() {
    crate::test_util::trace_init();
    let mut config = make_config("eh-partition");
    config.batch_enabled = false;
    config.partition_id_field = Some(
        vector_lib::lookup::lookup_v2::OptionalTargetPath::try_from("partition".to_string())
            .unwrap(),
    );

    let target_partition = "0";
    let num_events = 3;

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let events: Vec<Event> = (0..num_events)
        .map(|i| {
            let mut log = LogEvent::from(format!("partition-routed-event-{i}"));
            log.insert("partition", target_partition);
            log.with_batch_notifier(&batch).into()
        })
        .collect();
    drop(batch);

    let input_lines: Vec<String> = (0..num_events)
        .map(|i| format!("partition-routed-event-{i}"))
        .collect();

    let events_stream = futures::stream::iter(events.into_iter().map(Into::into)).boxed();

    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = config.build(cx).await.expect("Failed to build sink");
        sink.run(events_stream).await
    })
    .await
    .expect("Running sink failed");

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let received = read_events_from_partition("eh-partition", target_partition).await;

    for line in &input_lines {
        assert!(
            received.contains(line),
            "Expected event '{line}' on partition {target_partition}, but it was not found"
        );
    }
}

#[tokio::test]
async fn azure_event_hubs_sink_batch_partition_routing() {
    crate::test_util::trace_init();
    let mut config = make_config("eh-batchpartition");
    config.batch_enabled = true;
    config.batch_max_events = 100;
    config.partition_id_field = Some(
        vector_lib::lookup::lookup_v2::OptionalTargetPath::try_from("partition".to_string())
            .unwrap(),
    );

    let num_per_partition = 3;

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let mut events: Vec<Event> = Vec::new();
    for pid in &["0", "1"] {
        for i in 0..num_per_partition {
            let mut log = LogEvent::from(format!("batch-p{pid}-event-{i}"));
            log.insert("partition", *pid);
            events.push(log.with_batch_notifier(&batch).into());
        }
    }
    drop(batch);

    let events_stream = futures::stream::iter(events.into_iter().map(Into::into)).boxed();

    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = config.build(cx).await.expect("Failed to build sink");
        sink.run(events_stream).await
    })
    .await
    .expect("Running sink failed");

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    // Verify events landed on the correct partitions
    for pid in &["0", "1"] {
        let received = read_events_from_partition("eh-batchpartition", pid).await;
        for i in 0..num_per_partition {
            let expected = format!("batch-p{pid}-event-{i}");
            assert!(
                received.contains(&expected),
                "Expected '{expected}' on partition {pid}, got: {received:?}"
            );
        }
    }
}

#[tokio::test]
async fn azure_event_hubs_sink_rate_limit_backpressure() {
    crate::test_util::trace_init();
    let mut config = make_config("eh-ratelimit");
    config.batch_enabled = false;
    config.rate_limit_num = 5;
    config.rate_limit_duration_secs = 1;

    let num_events = 10;
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (input_lines, events) = random_lines_with_stream(100, num_events, Some(batch));

    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = config.build(cx).await.expect("Failed to build sink");
        sink.run(events).await
    })
    .await
    .expect("Running sink failed");

    assert_eq!(
        receiver.try_recv(),
        Ok(BatchStatus::Delivered),
        "All events should be delivered despite rate limiting"
    );

    let received = read_all_events("eh-ratelimit").await;

    for line in &input_lines {
        assert!(
            received.contains(line),
            "Rate-limited event missing: {line}"
        );
    }
}

#[tokio::test]
async fn azure_event_hubs_sink_json_encoding() {
    use vector_lib::codecs::JsonSerializerConfig;

    crate::test_util::trace_init();
    let mut config = make_config("eh-json");
    config.encoding = JsonSerializerConfig::default().into();

    let num_events = 3;
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (_, events) = random_lines_with_stream(100, num_events, Some(batch));

    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = config.build(cx).await.expect("Failed to build sink");
        sink.run(events).await
    })
    .await
    .expect("Running sink failed");

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let received = read_all_events("eh-json").await;

    // Verify at least num_events JSON-parseable bodies exist
    let json_count = received
        .iter()
        .filter(|body| serde_json::from_str::<serde_json::Value>(body).is_ok())
        .count();
    assert!(
        json_count >= num_events,
        "Expected at least {num_events} JSON-encoded events, got {json_count}"
    );
}

#[tokio::test]
async fn azure_event_hubs_sink_batch_overflow() {
    crate::test_util::trace_init();
    let mut config = make_config("eh-batchoverflow");
    config.batch_max_events = 2;

    let num_events = 5;
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (input_lines, events) = random_lines_with_stream(100, num_events, Some(batch));

    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = config.build(cx).await.expect("Failed to build sink");
        sink.run(events).await
    })
    .await
    .expect("Running sink failed");

    assert_eq!(
        receiver.try_recv(),
        Ok(BatchStatus::Delivered),
        "All events should be delivered across multiple batches"
    );

    let received = read_all_events("eh-batchoverflow").await;

    for line in &input_lines {
        assert!(
            received.contains(line),
            "Missing event after batch overflow: {line}"
        );
    }
}

#[tokio::test]
async fn azure_event_hubs_sink_acknowledgements_enabled() {
    crate::test_util::trace_init();
    let mut config = make_config("eh-ack");
    config.acknowledgements = AcknowledgementsConfig::from(true);

    let num_events = 5;
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (input_lines, events) = random_lines_with_stream(100, num_events, Some(batch));

    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = config.build(cx).await.expect("Failed to build sink");
        sink.run(events).await
    })
    .await
    .expect("Running sink failed");

    assert_eq!(
        receiver.try_recv(),
        Ok(BatchStatus::Delivered),
        "Events should be acknowledged as delivered with acknowledgements enabled"
    );

    let received = read_all_events("eh-ack").await;

    for line in &input_lines {
        assert!(
            received.contains(line),
            "Missing event with acks enabled: {line}"
        );
    }
}

#[tokio::test]
async fn azure_event_hubs_sink_rate_limit_batch_mode() {
    crate::test_util::trace_init();
    let mut config = make_config("eh-batchratelimit");
    config.batch_enabled = true;
    config.batch_max_events = 3;
    config.rate_limit_num = 2;
    config.rate_limit_duration_secs = 1;

    let num_events = 6;
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (input_lines, events) = random_lines_with_stream(100, num_events, Some(batch));

    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = config.build(cx).await.expect("Failed to build sink");
        sink.run(events).await
    })
    .await
    .expect("Running sink failed");

    assert_eq!(
        receiver.try_recv(),
        Ok(BatchStatus::Delivered),
        "All events should be delivered despite rate limiting in batch mode"
    );

    let received = read_all_events("eh-batchratelimit").await;

    for line in &input_lines {
        assert!(
            received.contains(line),
            "Rate-limited batch event missing: {line}"
        );
    }
}

#[tokio::test]
async fn azure_event_hubs_sink_empty_stream() {
    crate::test_util::trace_init();
    let config = make_config("eh-ack"); // shares hub; empty stream sends nothing

    let events = futures::stream::empty::<Event>().map(Into::into).boxed();

    let result: Result<(), ()> = assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = config.build(cx).await.expect("Failed to build sink");
        sink.run(events).await
    })
    .await;
    result.expect("Sink should handle empty stream without error");
}

#[tokio::test]
async fn azure_event_hubs_sink_oversized_event() {
    crate::test_util::trace_init();
    let mut config = make_config("eh-oversized");
    config.batch_enabled = true;
    config.batch_max_events = 100;

    // The emulator default max batch size is ~1MB. Create one normal event
    // and one oversized event to exercise the "Event too large" error path.
    let normal_msg = "normal-sized-event";
    let oversized_msg: String = "X".repeat(1024 * 1024 + 100); // >1MB

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let events: Vec<Event> = vec![
        LogEvent::from(normal_msg.to_string())
            .with_batch_notifier(&batch)
            .into(),
        LogEvent::from(oversized_msg)
            .with_batch_notifier(&batch)
            .into(),
    ];
    drop(batch);

    let events_stream = futures::stream::iter(events.into_iter().map(Into::into)).boxed();

    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = config.build(cx).await.expect("Failed to build sink");
        sink.run(events_stream).await
    })
    .await
    .expect("Sink should not crash on oversized events");

    // The batch with the normal event should still have been delivered
    assert_eq!(
        receiver.try_recv(),
        Ok(BatchStatus::Delivered),
        "Normal events should still be delivered alongside oversized ones"
    );

    let received = read_all_events("eh-oversized").await;
    assert!(
        received.contains(&normal_msg.to_string()),
        "The normal-sized event should have been delivered"
    );
}

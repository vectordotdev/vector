//! End-to-end tests proving that authoritative sinks control the ack chain.
//!
//! When a topology has at least one sink marked `authoritative: true`, only those
//! sinks participate in the ack chain. Non-authoritative sinks have their
//! finalizers stripped by the fanout, so a slow or blocked non-authoritative sink
//! does not prevent the source from receiving `BatchStatus::Delivered`.

use futures::StreamExt;
use tokio::time::{Duration, timeout};
use vector_lib::{
    config::AcknowledgementsConfig,
    event::{BatchNotifier, BatchStatus, LogEvent},
    finalization::AddBatchNotifier,
};

use crate::{
    config::Config,
    event::Event,
    test_util::{
        mock::{ack_source, basic_sink_with_acks, no_ack_sink},
        start_topology, trace_init,
    },
};

/// Helper to create an `AcknowledgementsConfig` with specific enabled and authoritative values.
fn ack_config(enabled: bool, authoritative: bool) -> AcknowledgementsConfig {
    AcknowledgementsConfig::new(Some(enabled), Some(authoritative))
}

/// Proves that when an authoritative sink processes events, the source receives
/// `BatchStatus::Delivered` promptly — even when a non-authoritative sink is
/// blocked and never processes its events.
///
/// Topology:
///   ack_source --> auth_sink      (authoritative: true, drains events)
///              \-> slow_sink      (authoritative: false, holds finalizers)
///
/// Expected behavior:
///   - The source gets `Delivered` because the authoritative sink processed events.
///   - The slow (non-authoritative) sink does NOT block the ack.
#[tokio::test]
async fn authoritative_sink_controls_ack_chain() {
    trace_init();

    // Create an ack-aware source.
    let (mut source_tx, source_config) = ack_source();

    // Create the authoritative sink — we will drain its channel to simulate
    // successful processing.
    let (auth_out, auth_sink_config) = basic_sink_with_acks(10, ack_config(true, true));

    // Create the non-authoritative "slow" sink using no_ack_sink, which holds
    // finalizers indefinitely and never triggers ack delivery.
    let (slow_sink_config, _slow_event_rx, _slow_held_finalizers) =
        no_ack_sink(ack_config(true, false));

    // Build the topology: source -> both sinks.
    let mut config = Config::builder();
    config.add_source("ack_source", source_config);
    config.add_sink("auth_sink", &["ack_source"], auth_sink_config);
    config.add_sink("slow_sink", &["ack_source"], slow_sink_config);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    // Create a BatchNotifier so we can observe ack status.
    let (batch, batch_status_rx) = BatchNotifier::new_with_receiver();

    // Create an event with the batch notifier attached.
    let mut event = Event::Log(LogEvent::from("test_authoritative_ack"));
    event.add_batch_notifier(batch.clone());

    // Drop the original batch notifier reference so only the event's copy remains.
    drop(batch);

    // Send the event into the source.
    source_tx.send_event(event).await.unwrap();

    // Drain the authoritative sink's output to trigger finalization.
    // The sink takes_finalizers and drops them, which signals Delivered.
    let auth_handle = tokio::spawn(async move {
        let mut auth_out = auth_out;
        auth_out.next().await
    });

    // Wait for the auth sink to receive the event.
    let auth_event = timeout(Duration::from_secs(5), auth_handle)
        .await
        .expect("Timed out waiting for auth sink to receive event")
        .expect("Auth sink task panicked");
    assert!(
        auth_event.is_some(),
        "Auth sink should have received the event"
    );

    // The non-authoritative sink holds finalizers but since it's non-authoritative,
    // its finalizers were stripped by the fanout. So even though it never acks,
    // it should NOT block the batch status.

    // Verify the source receives BatchStatus::Delivered promptly.
    // BatchStatusReceiver implements Future<Output = BatchStatus> directly.
    let status = timeout(Duration::from_secs(5), batch_status_rx)
        .await
        .expect("Timed out waiting for batch status — non-authoritative sink may have blocked ack");

    assert_eq!(
        status,
        BatchStatus::Delivered,
        "Source should receive Delivered when the authoritative sink processes events, \
         regardless of the non-authoritative sink's state"
    );

    topology.stop().await;
}

/// Proves that when the authoritative sink does NOT process events (holds
/// finalizers), the source does NOT receive an ack — the authoritative
/// sink truly controls the ack chain.
///
/// Topology:
///   ack_source --> auth_sink      (authoritative: true, holds finalizers)
///              \-> fast_sink      (authoritative: false, processes events)
///
/// Expected behavior:
///   - The source does NOT receive an ack because the authoritative sink hasn't
///     finalized the events.
///   - The non-authoritative sink processing events has no effect on ack delivery.
#[tokio::test]
async fn authoritative_sink_blocks_ack_when_not_drained() {
    trace_init();

    // Create an ack-aware source.
    let (mut source_tx, source_config) = ack_source();

    // Create the authoritative sink using no_ack_sink, which receives events
    // but holds their finalizers indefinitely — preventing ack delivery.
    let (auth_sink_config, auth_event_rx, _auth_held_finalizers) =
        no_ack_sink(ack_config(true, true));

    // Create the non-authoritative sink — it processes events normally
    // (takes and drops finalizers), but since it's non-authoritative, this
    // should NOT trigger the ack.
    let (fast_out, fast_sink_config) = basic_sink_with_acks(10, ack_config(true, false));

    // Build the topology.
    let mut config = Config::builder();
    config.add_source("ack_source", source_config);
    config.add_sink("auth_sink", &["ack_source"], auth_sink_config);
    config.add_sink("fast_sink", &["ack_source"], fast_sink_config);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    // Create a BatchNotifier.
    let (batch, batch_status_rx) = BatchNotifier::new_with_receiver();

    // Create an event with the batch notifier.
    let mut event = Event::Log(LogEvent::from("test_auth_blocks"));
    event.add_batch_notifier(batch.clone());
    drop(batch);

    // Send the event.
    source_tx.send_event(event).await.unwrap();

    // Wait for the authoritative sink to receive the event (but it holds finalizers).
    timeout(Duration::from_secs(5), auth_event_rx)
        .await
        .expect("Timed out waiting for auth sink to receive event")
        .expect("Auth sink event notification channel closed");

    // Drain the non-authoritative sink to prove it doesn't trigger the ack.
    let fast_handle = tokio::spawn(async move {
        let mut fast_out = fast_out;
        fast_out.next().await
    });

    // Wait for the non-auth sink to receive and process the event.
    let fast_event = timeout(Duration::from_secs(5), fast_handle)
        .await
        .expect("Timed out waiting for fast sink")
        .expect("Fast sink task panicked");
    assert!(
        fast_event.is_some(),
        "Non-authoritative sink should have received the event"
    );

    // The authoritative sink holds finalizers. The ack should NOT arrive
    // because the authoritative sink hasn't finalized.
    let ack_result = timeout(Duration::from_millis(500), batch_status_rx).await;

    assert!(
        ack_result.is_err(),
        "Source should NOT receive an ack when the authoritative sink hasn't processed events. \
         The non-authoritative sink draining should not trigger the ack."
    );

    topology.stop().await;
}

/// Proves that an isolated pipeline (source -> sink) without any authoritative
/// sinks preserves legacy wait-for-all acking when another pipeline in the
/// same topology DOES have an authoritative sink.
///
/// This is the key regression test for the per-edge stripping fix. Before
/// the fix, the strip decision was per-component: any component not in the
/// authoritative set had finalizers stripped. This meant an isolated
/// non-authoritative pipeline's sink would have finalizers stripped just
/// because an unrelated authoritative sink existed elsewhere, causing the
/// isolated source to ack events before its sink processed them.
///
/// Topology:
///   source_auth -> auth_sink     (authoritative: true, drains events)
///   source_iso  -> iso_sink      (authoritative: false, holds finalizers)
///
/// Expected behavior:
///   - source_iso does NOT receive an ack because iso_sink holds finalizers.
///   - The authoritative pipeline does not affect iso's ack chain.
#[tokio::test]
async fn isolated_pipeline_preserves_legacy_acking() {
    trace_init();

    // Create the authoritative pipeline: ack_source -> auth_sink.
    let (mut auth_source_tx, auth_source_config) = ack_source();
    let (auth_out, auth_sink_config) = basic_sink_with_acks(10, ack_config(true, true));

    // Create the isolated non-authoritative pipeline: iso_source -> iso_sink.
    // iso_sink holds finalizers indefinitely (via no_ack_sink), simulating a
    // slow/blocked sink that hasn't processed events yet.
    let (mut iso_source_tx, iso_source_config) = ack_source();
    let (iso_sink_config, iso_event_rx, _iso_held_finalizers) =
        no_ack_sink(ack_config(true, false));

    // Build the topology with both pipelines.
    let mut config = Config::builder();
    config.add_source("source_auth", auth_source_config);
    config.add_source("source_iso", iso_source_config);
    config.add_sink("auth_sink", &["source_auth"], auth_sink_config);
    config.add_sink("iso_sink", &["source_iso"], iso_sink_config);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    // --- Send an event through the isolated pipeline ---
    let (iso_batch, iso_batch_status_rx) = BatchNotifier::new_with_receiver();
    let mut iso_event = Event::Log(LogEvent::from("test_isolated_pipeline"));
    iso_event.add_batch_notifier(iso_batch.clone());
    drop(iso_batch);

    iso_source_tx.send_event(iso_event).await.unwrap();

    // Wait for iso_sink to receive the event (it holds finalizers).
    timeout(Duration::from_secs(5), iso_event_rx)
        .await
        .expect("Timed out waiting for iso_sink to receive event")
        .expect("iso_sink event notification channel closed");

    // Also send an event through the auth pipeline and drain it, to prove
    // the auth pipeline's ack doesn't interfere.
    let (auth_batch, _auth_batch_status_rx) = BatchNotifier::new_with_receiver();
    let mut auth_event = Event::Log(LogEvent::from("test_auth_pipeline"));
    auth_event.add_batch_notifier(auth_batch.clone());
    drop(auth_batch);

    auth_source_tx.send_event(auth_event).await.unwrap();

    let auth_handle = tokio::spawn(async move {
        let mut auth_out = auth_out;
        auth_out.next().await
    });
    timeout(Duration::from_secs(5), auth_handle)
        .await
        .expect("Timed out waiting for auth sink")
        .expect("Auth sink task panicked");

    // The isolated pipeline's sink holds finalizers. If per-edge stripping
    // works correctly, iso_sink's finalizers are NOT stripped (because
    // source_iso is not on any authoritative path), so the ack should be
    // blocked. If the old per-component stripping was used, iso_sink's
    // finalizers would be stripped and the ack would arrive immediately.
    let iso_ack_result = timeout(Duration::from_millis(500), iso_batch_status_rx).await;

    assert!(
        iso_ack_result.is_err(),
        "Isolated pipeline source should NOT receive an ack when its sink holds finalizers. \
         If this fails, per-edge stripping is broken: the authoritative pipeline elsewhere \
         in the topology is incorrectly causing the isolated pipeline's finalizers to be stripped."
    );

    topology.stop().await;
}

use std::{
    collections::HashMap,
    iter,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

use crate::{
    config::{Config, ConfigDiff, SinkOuter},
    event::{into_event_stream, Event, EventArray, EventContainer, LogEvent},
    test_util::{
        mock::{
            basic_sink, basic_sink_failing_healthcheck, basic_sink_with_data, basic_source,
            basic_source_with_data, basic_source_with_event_counter, basic_transform,
            error_definition_transform,
        },
        start_topology, trace_init,
    },
    topology::{RunningTopology, TopologyPieces},
};
use crate::{schema::Definition, source_sender::SourceSenderItem};
use futures::{future, stream, StreamExt};
use tokio::{
    task::yield_now,
    time::{sleep, Duration},
};
use vector_lib::buffers::{BufferConfig, BufferType, WhenFull};
use vector_lib::config::ComponentKey;
use vector_lib::config::OutputId;

mod backpressure;
mod compliance;
#[cfg(all(feature = "sinks-socket", feature = "sources-socket"))]
mod crash;
mod doesnt_reload;
#[cfg(all(feature = "sources-http_server", feature = "sinks-http"))]
mod end_to_end;
#[cfg(all(
    feature = "sources-prometheus",
    feature = "sinks-prometheus",
    feature = "sources-internal_metrics",
    feature = "sources-splunk_hec"
))]
mod reload;
#[cfg(all(feature = "sinks-console", feature = "sources-demo_logs"))]
mod source_finished;
mod transient_state;

fn basic_config() -> Config {
    trace_init();

    let mut config = Config::builder();
    config.add_source("in1", basic_source().1);
    config.add_sink("out1", &["in1"], basic_sink(10).1);
    config.build().unwrap()
}

fn basic_config_with_sink_failing_healthcheck() -> Config {
    trace_init();

    let mut config = Config::builder();
    config.add_source("in1", basic_source().1);
    config.add_sink("out1", &["in1"], basic_sink_failing_healthcheck(10).1);
    config.build().unwrap()
}

fn into_message(event: Event) -> String {
    let message_key = crate::config::log_schema()
        .message_key_target_path()
        .unwrap();
    event
        .as_log()
        .get(message_key)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn into_message_stream(array: SourceSenderItem) -> impl futures::Stream<Item = String> {
    stream::iter(array.events.into_events().map(into_message))
}

#[tokio::test]
async fn topology_shutdown_while_active() {
    trace_init();

    let (mut in1, source1, counter) = basic_source_with_event_counter(true);

    let transform1 = basic_transform(" transformed", 0.0);
    let (out1, sink1) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_sink("out1", &["t1"], sink1);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let pump_handle = tokio::spawn(async move {
        let mut stream = futures::stream::repeat(Event::Log(LogEvent::from("test")));
        in1.send_event_stream(&mut stream).await
    });

    let sink_rx_handle = tokio::spawn(async move { out1.collect::<Vec<_>>().await });

    // Wait until at least 100 events have been seen by the source so we know the pump is running
    // and pushing events through the pipeline.
    while counter.load(Ordering::SeqCst) < 100 {
        yield_now().await;
    }

    // Now stop the topology to trigger the shutdown on the source:
    topology.stop().await;

    // Now that shutdown has begun we should be able to drain the Sink without blocking forever,
    // as the source should shut down and close its output channel.
    let processed_events = sink_rx_handle.await.unwrap();
    assert_eq!(processed_events.len(), counter.load(Ordering::Relaxed));
    for event in processed_events
        .into_iter()
        .flat_map(|item| EventArray::into_events(item.into()))
    {
        assert_eq!(
            event.as_log()[&crate::config::log_schema()
                .message_key()
                .unwrap()
                .to_string()],
            "test transformed".to_owned().into()
        );
    }

    // We expect the pump to fail with an error since we shut down the source it was sending to
    // while it was running.
    assert!(pump_handle.await.unwrap().is_err());
}

#[tokio::test]
async fn topology_source_and_sink() {
    trace_init();

    let (mut in1, source1) = basic_source();
    let (out1, sink1) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let mut event = Event::Log(LogEvent::from("this"));
    in1.send_event(event.clone()).await.unwrap();

    topology.stop().await;

    let res = out1.flat_map(into_event_stream).collect::<Vec<_>>().await;

    event.set_source_id(Arc::new(ComponentKey::from("in1")));
    event.set_upstream_id(Arc::new(OutputId::from("test")));
    event
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

    assert_eq!(vec![event], res);
}

#[tokio::test]
async fn topology_multiple_sources() {
    trace_init();

    let (mut in1, source1) = basic_source();
    let (mut in2, source2) = basic_source();
    let (mut out1, sink1) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_source("in2", source2);
    config.add_sink("out1", &["in1", "in2"], sink1);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let mut event1 = Event::Log(LogEvent::from("this"));
    let mut event2 = Event::Log(LogEvent::from("that"));

    in1.send_event(event1.clone()).await.unwrap();

    let out_event1: Option<EventArray> = out1.next().await.map(|item| item.into());

    in2.send_event(event2.clone()).await.unwrap();

    let out_event2: Option<EventArray> = out1.next().await.map(|item| item.into());

    topology.stop().await;

    event1.set_source_id(Arc::new(ComponentKey::from("in1")));
    event2.set_source_id(Arc::new(ComponentKey::from("in2")));

    event1.set_upstream_id(Arc::new(OutputId::from("test")));
    event1
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

    event2.set_upstream_id(Arc::new(OutputId::from("test")));
    event2
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

    assert_eq!(out_event1, Some(event1.into()));
    assert_eq!(out_event2, Some(event2.into()));
}

#[tokio::test]
async fn topology_multiple_sinks() {
    trace_init();

    // Create source #1 as `in1`, sink #1, and sink #2, with both sink #1 and sink #2 attached to `in1`.
    let (mut in1, source1) = basic_source();
    let (out1, sink1) = basic_sink(10);
    let (out2, sink2) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);
    config.add_sink("out2", &["in1"], sink2);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    // Send an event into source #1:
    let mut event = Event::Log(LogEvent::from("this"));
    in1.send_event(event.clone()).await.unwrap();

    // Drop the inputs to the two sources, which will ensure they drain all items and stop
    // themselves, and also fully stop the topology:
    drop(in1);
    topology.stop().await;

    let res1 = out1.flat_map(into_event_stream).collect::<Vec<_>>().await;
    let res2 = out2.flat_map(into_event_stream).collect::<Vec<_>>().await;

    // We should see that both sinks got the exact same event:
    event.set_source_id(Arc::new(ComponentKey::from("in1")));

    event.set_upstream_id(Arc::new(OutputId::from("test")));
    event
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

    let expected = vec![event];
    assert_eq!(expected, res1);
    assert_eq!(expected, res2);
}

#[tokio::test]
async fn topology_transform_chain() {
    trace_init();

    let (mut in1, source1) = basic_source();
    let transform1 = basic_transform(" first", 0.0);
    let transform2 = basic_transform(" second", 0.0);
    let (out1, sink1) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_transform("t2", &["t1"], transform2);
    config.add_sink("out1", &["t2"], sink1);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let event = Event::Log(LogEvent::from("this"));

    in1.send_event(event).await.unwrap();

    topology.stop().await;

    let res = out1.flat_map(into_message_stream).collect::<Vec<_>>().await;

    assert_eq!(vec!["this first second"], res);
}

#[tokio::test]
async fn topology_remove_one_source() {
    trace_init();

    let (mut in1, source1) = basic_source();
    let (mut in2, source2) = basic_source();
    let (_out1, sink1) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_source("in2", source2);
    config.add_sink("out1", &["in1", "in2"], sink1);

    let (mut topology, _) = start_topology(config.build().unwrap(), false).await;

    let (out1, sink1) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", basic_source().1);
    config.add_sink("out1", &["in1"], sink1);

    assert!(topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap());

    // Send an event into both source #1 and source #2:
    let mut event1 = Event::Log(LogEvent::from("this"));
    let event2 = Event::Log(LogEvent::from("that"));
    let h_out1 = tokio::spawn(out1.flat_map(into_event_stream).collect::<Vec<_>>());

    in1.send_event(event1.clone()).await.unwrap();
    in2.send_event(event2.clone()).await.unwrap_err();

    // Drop the inputs to the two sources, which will ensure they drain all items and stop
    // themselves, and also fully stop the topology:
    drop(in1);
    drop(in2);
    topology.stop().await;

    event1.set_source_id(Arc::new(ComponentKey::from("in1")));

    event1.set_upstream_id(Arc::new(OutputId::from("test")));
    event1
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

    let res = h_out1.await.unwrap();
    assert_eq!(vec![event1], res);
}

#[tokio::test]
async fn topology_remove_one_sink() {
    trace_init();

    let (mut in1, source1) = basic_source();
    let (out1, sink1) = basic_sink(10);
    let (out2, sink2) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);
    config.add_sink("out2", &["in1"], sink2);

    let (mut topology, _) = start_topology(config.build().unwrap(), false).await;

    let mut config = Config::builder();
    config.add_source("in1", basic_source().1);
    config.add_sink("out1", &["in1"], basic_sink(10).1);

    assert!(topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap());

    let mut event = Event::Log(LogEvent::from("this"));

    in1.send_event(event.clone()).await.unwrap();

    topology.stop().await;

    let res1 = out1.flat_map(into_event_stream).collect::<Vec<_>>().await;
    let res2 = out2.flat_map(into_event_stream).collect::<Vec<_>>().await;

    event.set_source_id(Arc::new(ComponentKey::from("in1")));

    event.set_upstream_id(Arc::new(OutputId::from("test")));
    event
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

    assert_eq!(vec![event], res1);
    assert_eq!(Vec::<Event>::new(), res2);
}

#[tokio::test]
async fn topology_remove_one_transform() {
    trace_init();

    // Create a simple source/transform/transform/sink topology, wired up in that order:
    let (mut in1, source1) = basic_source();
    let transform1 = basic_transform(" transformed", 0.0);
    let transform2 = basic_transform(" transformed", 0.0);
    let (out1, sink1) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_transform("t2", &["t1"], transform2);
    config.add_sink("out1", &["t2"], sink1);

    let (mut topology, _) = start_topology(config.build().unwrap(), false).await;

    // Now create an identical topology, but remove one of the transforms:
    let (mut in2, source2) = basic_source();
    let transform2 = basic_transform(" transformed", 0.0);
    let (out2, sink2) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source2);
    config.add_transform("t2", &["in1"], transform2);
    config.add_sink("out1", &["t2"], sink2);

    assert!(topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap());

    // Send the same event to both sources:
    let event = Event::Log(LogEvent::from("this"));
    let h_out1 = tokio::spawn(out1.flat_map(into_message_stream).collect::<Vec<_>>());
    let h_out2 = tokio::spawn(out2.flat_map(into_message_stream).collect::<Vec<_>>());
    in1.send_event(event.clone()).await.unwrap();
    in2.send_event(event.clone()).await.unwrap();

    // Drop the inputs to the two sources, which will ensure they drain all items and stop
    // themselves, and also fully stop the topology:
    drop(in1);
    drop(in2);
    topology.stop().await;

    // We should see that because the source and sink didn't change -- only the one transform being
    // removed -- that the event sent to the first source is the one that makes it through, but that
    // it now goes through the changed transform chain: one transform instead of two.
    let res1 = h_out1.await.unwrap();
    let res2 = h_out2.await.unwrap();
    assert_eq!(vec!["this transformed"], res1);
    assert_eq!(Vec::<String>::new(), res2);
}

#[tokio::test]
async fn topology_swap_source() {
    trace_init();

    // Add source #1 as `in1`, and sink #1 as `out1`, with sink #1 attached to `in1`:
    let (mut in1, source1) = basic_source();
    let (out1, sink1) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);

    let (mut topology, _) = start_topology(config.build().unwrap(), false).await;

    // Now, create sink #2 and replace `out2` with it, and add source #2 as `in2`, attached to `out1`:
    let (mut in2, source2) = basic_source();
    let (out2, sink2) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in2", source2);
    config.add_sink("out1", &["in2"], sink2);

    assert!(topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap());

    // Send an event into both source #1 and source #2:
    let event1 = Event::Log(LogEvent::from("this"));
    let mut event2 = Event::Log(LogEvent::from("that"));

    let h_out1 = tokio::spawn(out1.flat_map(into_event_stream).collect::<Vec<_>>());
    let h_out2 = tokio::spawn(out2.flat_map(into_event_stream).collect::<Vec<_>>());
    in1.send_event(event1.clone()).await.unwrap_err();
    in2.send_event(event2.clone()).await.unwrap();

    // Drop the inputs to the two sources, which will ensure they drain all items and stop
    // themselves, and also fully stop the topology:
    drop(in1);
    drop(in2);
    topology.stop().await;

    let res1 = h_out1.await.unwrap();
    let res2 = h_out2.await.unwrap();

    // We should see that despite replacing a sink of the same name, sending to source #1 -- which
    // the sink at `out1` was initially connected to -- does not send to either sink #1 or sink #2,
    // as we've removed it from the topology prior to the sends.
    assert_eq!(Vec::<Event>::new(), res1);

    event2.set_source_id(Arc::new(ComponentKey::from("in2")));
    event2.set_upstream_id(Arc::new(OutputId::from("test")));
    event2
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

    assert_eq!(vec![event2], res2);
}

#[tokio::test]
async fn topology_swap_transform() {
    trace_init();

    // Add source #1 as `in1`, transform #1 as `t1`, and sink #1 as `out1`, with transform #1
    // attached to `in1` and sink #1 attached to `t1`:
    let (mut in1, source1) = basic_source();
    let transform1 = basic_transform(" transformed", 0.0);
    let (out1, sink1) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_sink("out1", &["t1"], sink1);

    let (mut topology, _) = start_topology(config.build().unwrap(), false).await;

    // Now, create source #2 and replace `in1` with it, add transform #2 as `t1`, attached to `in1`,
    // and add sink #2 as `out1`, attached to `t1`:
    let (mut in2, source2) = basic_source();
    let transform2 = basic_transform(" replaced", 0.0);
    let (out2, sink2) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source2);
    config.add_transform("t1", &["in1"], transform2);
    config.add_sink("out1", &["t1"], sink2);

    assert!(topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap());

    // Send an event into both source #1 and source #2:
    let event1 = Event::Log(LogEvent::from("this"));
    let event2 = Event::Log(LogEvent::from("that"));

    let h_out1 = tokio::spawn(out1.flat_map(into_message_stream).collect::<Vec<_>>());
    let h_out2 = tokio::spawn(out2.flat_map(into_message_stream).collect::<Vec<_>>());
    in1.send_event(event1.clone()).await.unwrap();
    in2.send_event(event2.clone()).await.unwrap();

    // Drop the inputs to the two sources, which will ensure they drain all items and stop
    // themselves, and also fully stop the topology:
    drop(in1);
    drop(in2);
    topology.stop().await;

    let res1 = h_out1.await.unwrap();
    let res2 = h_out2.await.unwrap();

    // We should see that since source #1 and #2 were the same, as well as sink #1 and sink #2,
    // despite both being added as `in1`, that source #1 was not rebuilt, so the item sent to source
    // #1 was the item that got transformed, which was emitted via `out1`/`h_out1`/`res1`.
    assert_eq!(vec!["this replaced"], res1);
    assert_eq!(Vec::<String>::new(), res2);
}

#[tokio::test]
async fn topology_swap_sink() {
    trace_init();

    // Add source #1 as `in1`, and sink #1 as `out1`, with sink #1 attached to `in1`:
    let (mut in1, source1) = basic_source();
    let (out1, sink1) = basic_sink_with_data(10, "v1");

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);

    let (mut topology, _) = start_topology(config.build().unwrap(), false).await;

    // Now, create an identical topology except that the sink has changed which will force it to be rebuilt:
    let (mut in2, source2) = basic_source();
    let (out2, sink2) = basic_sink_with_data(10, "v2");

    let mut config = Config::builder();
    config.add_source("in1", source2);
    config.add_sink("out1", &["in1"], sink2);

    assert!(topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap());

    // Send an event into both source #1 and source #2:
    let mut event1 = Event::Log(LogEvent::from("this"));
    let event2 = Event::Log(LogEvent::from("that"));

    let h_out1 = tokio::spawn(out1.flat_map(into_event_stream).collect::<Vec<_>>());
    let h_out2 = tokio::spawn(out2.flat_map(into_event_stream).collect::<Vec<_>>());
    in1.send_event(event1.clone()).await.unwrap();
    in2.send_event(event2.clone()).await.unwrap();

    // Drop the inputs to the two sources, which will ensure they drain all items and stop
    // themselves, and also fully stop the topology:
    drop(in1);
    drop(in2);
    topology.stop().await;

    let res1 = h_out1.await.unwrap();
    let res2 = h_out2.await.unwrap();

    // We should see that since source #1 and #2 were the same, despite both being added as `in1`,
    // that source #1 was not rebuilt, so the item sent to source #1 was the item that got sent to
    // the new sink, which _was_ rebuilt:
    assert_eq!(Vec::<Event>::new(), res1);

    event1.set_source_id(Arc::new(ComponentKey::from("in1")));
    event1.set_upstream_id(Arc::new(OutputId::from("test")));
    event1
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
    assert_eq!(vec![event1], res2);
}

#[tokio::test]
async fn topology_swap_transform_is_atomic() {
    trace_init();

    let (mut in1, source1) = basic_source();
    let transform1v1 = basic_transform(" transformed", 0.0);
    let (out1, sink1) = basic_sink(10);

    let running = Arc::new(AtomicBool::new(true));
    let run_control = Arc::clone(&running);

    let send_counter = Arc::new(AtomicUsize::new(0));
    let recv_counter = Arc::new(AtomicUsize::new(0));
    let send_total = Arc::clone(&send_counter);
    let recv_total = Arc::clone(&recv_counter);

    let events = move || {
        if running.load(Ordering::Acquire) {
            send_counter.fetch_add(1, Ordering::Release);
            Some(Event::Log(LogEvent::from("this")))
        } else {
            None
        }
    };
    let input = async move {
        in1.send_event_stream(stream::iter(iter::from_fn(events)))
            .await
            .unwrap();
    };
    let output = out1.for_each(move |_| {
        recv_counter.fetch_add(1, Ordering::Release);
        future::ready(())
    });

    let h_out = tokio::spawn(output);
    let h_in = tokio::spawn(input);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1v1);
    config.add_sink("out1", &["t1"], sink1);

    let (mut topology, _) = start_topology(config.build().unwrap(), false).await;
    sleep(Duration::from_millis(10)).await;

    let transform1v2 = basic_transform(" replaced", 0.0);

    let mut config = Config::builder();
    config.add_source("in1", basic_source().1);
    config.add_transform("t1", &["in1"], transform1v2);
    config.add_sink("out1", &["t1"], basic_sink(10).1);

    assert!(topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap());

    run_control.store(false, Ordering::Release);
    h_in.await.unwrap();
    topology.stop().await;
    h_out.await.unwrap();

    assert_eq!(
        send_total.load(Ordering::Acquire),
        recv_total.load(Ordering::Acquire)
    );
}

#[tokio::test]
async fn topology_rebuild_connected() {
    trace_init();

    let (_in1, source1) = basic_source_with_data("v1");
    let (_out1, sink1) = basic_sink_with_data(10, "v1");

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);

    let (mut topology, _) = start_topology(config.build().unwrap(), false).await;

    let (mut in1, source1) = basic_source_with_data("v2");
    let (out1, sink1) = basic_sink_with_data(10, "v2");

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);

    assert!(topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap());

    let mut event1 = Event::Log(LogEvent::from("this"));
    let mut event2 = Event::Log(LogEvent::from("that"));
    let h_out1 = tokio::spawn(out1.flat_map(into_event_stream).collect::<Vec<_>>());
    in1.send_event(event1.clone()).await.unwrap();
    in1.send_event(event2.clone()).await.unwrap();

    drop(in1);
    topology.stop().await;

    let res = h_out1.await.unwrap();

    event1.set_source_id(Arc::new(ComponentKey::from("in1")));
    event2.set_source_id(Arc::new(ComponentKey::from("in1")));

    event1.set_upstream_id(Arc::new(OutputId::from("test")));
    event2.set_upstream_id(Arc::new(OutputId::from("test")));
    event1
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
    event2
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

    assert_eq!(vec![event1, event2], res);
}

#[tokio::test]
async fn topology_rebuild_connected_transform() {
    trace_init();

    let (mut in1, source1) = basic_source_with_data("v1");
    let transform1 = basic_transform(" transformed", 0.0);
    let transform2 = basic_transform(" transformed", 0.0);
    let (out1, sink1) = basic_sink_with_data(10, "v1");

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_transform("t2", &["t1"], transform2);
    config.add_sink("out1", &["t2"], sink1);

    let (mut topology, _) = start_topology(config.build().unwrap(), false).await;

    let (mut in2, source2) = basic_source_with_data("v1"); // not changing
    let transform1 = basic_transform("", 0.0);
    let transform2 = basic_transform("", 0.0);
    let (out2, sink2) = basic_sink_with_data(10, "v2");

    let mut config = Config::builder();
    config.add_source("in1", source2);
    config.add_transform("t1", &["in1"], transform1);
    config.add_transform("t2", &["t1"], transform2);
    config.add_sink("out1", &["t2"], sink2);

    assert!(topology
        .reload_config_and_respawn(config.build().unwrap(), Default::default())
        .await
        .unwrap());

    let mut event = Event::Log(LogEvent::from("this"));
    let h_out1 = tokio::spawn(out1.flat_map(into_event_stream).collect::<Vec<_>>());
    let h_out2 = tokio::spawn(out2.flat_map(into_event_stream).collect::<Vec<_>>());

    in1.send_event(event.clone()).await.unwrap();
    in2.send_event(event.clone()).await.unwrap();

    drop(in1);
    drop(in2);
    topology.stop().await;

    let res1 = h_out1.await.unwrap();
    let res2 = h_out2.await.unwrap();
    assert_eq!(Vec::<Event>::new(), res1);

    event.set_source_id(Arc::new(ComponentKey::from("in1")));
    event.set_upstream_id(Arc::new(OutputId::from("test")));
    event
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

    assert_eq!(vec![event], res2);
}

#[tokio::test]
async fn topology_required_healthcheck_fails_start() {
    let mut config = basic_config_with_sink_failing_healthcheck();
    config.healthchecks.require_healthy = true;
    assert!(
        RunningTopology::start_init_validated(config, Default::default())
            .await
            .is_none()
    );
}

#[tokio::test]
async fn topology_optional_healthcheck_does_not_fail_start() {
    let config = basic_config_with_sink_failing_healthcheck();
    assert!(
        RunningTopology::start_init_validated(config, Default::default())
            .await
            .is_some()
    );
}

#[tokio::test]
async fn topology_optional_healthcheck_does_not_fail_reload() {
    let config = basic_config();
    let (mut topology, _) = start_topology(config, false).await;
    let config = basic_config_with_sink_failing_healthcheck();
    assert!(topology
        .reload_config_and_respawn(config, Default::default())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_healthcheck_not_run_on_unchanged_reload() {
    let config = basic_config();

    let (mut topology, _) = start_topology(config, false).await;
    let mut config = basic_config_with_sink_failing_healthcheck();
    config.healthchecks.require_healthy = true;
    assert!(topology
        .reload_config_and_respawn(config, Default::default())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_healthcheck_run_for_changes_on_reload() {
    trace_init();

    let mut config = Config::builder();
    // We can't just drop the sender side since that will close the source.
    let (_ch0, src) = basic_source();
    config.add_source("in1", src);
    config.add_sink("out1", &["in1"], basic_sink(10).1);

    let (mut topology, _) = start_topology(config.build().unwrap(), false).await;

    let mut config = Config::builder();
    // We can't just drop the sender side since that will close the source.
    let (_ch1, src) = basic_source();
    config.add_source("in1", src);
    config.add_sink("out2", &["in1"], basic_sink_failing_healthcheck(10).1);

    let mut config = config.build().unwrap();
    config.healthchecks.require_healthy = true;
    assert!(!topology
        .reload_config_and_respawn(config, Default::default())
        .await
        .unwrap());
}

#[tokio::test]
async fn topology_disk_buffer_flushes_on_idle() {
    trace_init();

    let tmpdir = tempfile::tempdir().expect("no tmpdir");
    let event = Event::Log(LogEvent::from("foo"));

    let (mut in1, source1) = basic_source();
    let transform1 = basic_transform("", 0.0);
    let (mut out1, sink1) = basic_sink(10);

    let mut config = Config::builder();
    config.set_data_dir(tmpdir.path());
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    let mut sink1_outer = SinkOuter::new(
        // read from both the source and the transform
        vec![String::from("in1"), String::from("t1")],
        sink1,
    );
    sink1_outer.buffer = BufferConfig::Single(BufferType::DiskV2 {
        max_size: std::num::NonZeroU64::new(268435488).unwrap(),
        when_full: WhenFull::DropNewest,
    });
    config.add_sink_outer("out1", sink1_outer);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    in1.send_event(event).await.unwrap();

    // ensure that we get the first copy of the event within a reasonably short amount of time
    // (either from the source or the transform)
    let res = tokio::time::timeout(Duration::from_secs(1), out1.next())
        .await
        .expect("timeout 1")
        .map(|array| into_message(array.into_events().next().unwrap()))
        .expect("no output");
    assert_eq!("foo", res);

    // ensure that we get the second copy of the event
    let res = tokio::time::timeout(Duration::from_secs(1), out1.next())
        .await
        .expect("timeout 2")
        .map(|array| into_message(array.into_events().next().unwrap()))
        .expect("no output");
    assert_eq!("foo", res);

    // stop the topology only after we've received both copies of the event, to ensure it wasn't
    // shutdown that flushed them
    topology.stop().await;

    // make sure there are no unexpected stragglers
    let rest = out1.collect::<Vec<_>>().await;
    assert!(rest.is_empty());
}

#[tokio::test]
async fn topology_transform_error_definition() {
    trace_init();

    let mut config = Config::builder();

    config.add_source("in", basic_source().1);
    config.add_transform("transform", &["in"], error_definition_transform());
    config.add_sink("sink", &["transform"], basic_sink(10).1);

    let config = config.build().unwrap();
    let diff = ConfigDiff::initial(&config);
    let errors =
        match TopologyPieces::build(&config, &diff, HashMap::new(), Default::default()).await {
            Ok(_) => panic!("build pieces should not succeed"),
            Err(err) => err,
        };

    assert_eq!(
        r#"Transform "transform": It all went horribly wrong"#,
        errors[0]
    );
}

#[tokio::test]
async fn source_metadata_reaches_sink() {
    trace_init();

    let (mut in1, source1) = basic_source();
    let (mut in2, source2) = basic_source();
    let (mut out1, sink1) = basic_sink(10);

    let mut config = Config::builder();
    config.add_source("in1", source1);
    config.add_source("in2", source2);
    config.add_transform("transform", &["in1", "in2"], basic_transform("", 0.0));
    config.add_sink("out1", &["transform"], sink1);

    let (topology, _) = start_topology(config.build().unwrap(), false).await;

    let event1 = Event::Log(LogEvent::from("this"));
    let event2 = Event::Log(LogEvent::from("that"));

    in1.send_event(event1.clone()).await.unwrap();

    let out_event1 = out1.next().await.unwrap();
    let out_event1 = out_event1.events.iter_events().next().unwrap();

    in2.send_event(event2.clone()).await.unwrap();

    let out_event2 = out1.next().await.unwrap();
    let out_event2 = out_event2.events.iter_events().next().unwrap();

    topology.stop().await;

    assert_eq!(
        **out_event1.into_log().metadata().source_id().unwrap(),
        ComponentKey::from("in1")
    );
    assert_eq!(
        **out_event2.into_log().metadata().source_id().unwrap(),
        ComponentKey::from("in2")
    );
}

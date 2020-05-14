#![allow(clippy::bool_comparison)]
#![allow(clippy::match_bool)]
#![allow(clippy::redundant_clone)]

mod support;

use crate::support::{sink, sink_failing_healthcheck, source, transform, MockSourceConfig};
use futures01::{
    future, future::Future, sink::Sink, stream::iter_ok, stream::Stream, sync::mpsc::SendError,
    sync::oneshot,
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use std::{iter, thread};
use vector::event::{self, Event};
use vector::test_util::{runtime, shutdown_on_idle, trace_init};
use vector::topology;
use vector::topology::config::Config;

fn basic_config() -> Config {
    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_sink("out1", &["in1"], sink(10).1);
    config
}

fn basic_config_with_sink_failing_healthcheck() -> Config {
    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_sink("out1", &["in1"], sink_failing_healthcheck(10).1);
    config
}

fn into_message(event: Event) -> String {
    event
        .as_log()
        .get(&event::log_schema().message_key())
        .unwrap()
        .to_string_lossy()
}

#[test]
fn topology_shutdown_while_active() {
    let source_event_counter = Arc::new(AtomicUsize::new(0));
    let source_event_total = source_event_counter.clone();

    let mut rt = runtime();
    let (in1, rx) = futures01::sync::mpsc::channel(1000);

    let source1 = MockSourceConfig::new_with_event_counter(rx, source_event_counter);
    let transform1 = transform(" transformed", 0.0);
    let (out1, sink1) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_sink("out1", &["t1"], sink1);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let pump_future = iter_ok::<_, SendError<vector::event::Event>>(iter::from_fn(move || {
        Some(Event::from("test"))
    }))
    .forward(in1);
    let pump_handle = oneshot::spawn(pump_future, &rt.executor());

    // Wait until at least 100 events have been seen by the source so we know the pump is running
    // and pushing events through the pipeline.
    while source_event_total.load(Ordering::SeqCst) < 100 {
        thread::sleep(Duration::from_millis(10));
    }

    // Now shut down the RunningTopology while Events are still being processed.
    let stop_complete = oneshot::spawn(topology.stop(), &rt.executor());

    // Now that shutdown has begun we should be able to drain the Sink without blocking forever,
    // as the source should shut down and close its output channel.
    let processed_events = out1.collect().wait().unwrap();
    assert_eq!(
        processed_events.len(),
        source_event_total.load(Ordering::Relaxed)
    );
    for event in processed_events {
        assert_eq!(
            event.as_log()[&event::log_schema().message_key()],
            "test transformed".into()
        );
    }

    rt.block_on(stop_complete).unwrap();

    // We expect the pump to fail with an error since we shut down the source it was sending to
    // while it was running.
    let _err: SendError<Event> = rt.block_on(pump_handle).unwrap_err();

    shutdown_on_idle(rt);
}

#[test]
fn topology_source_and_sink() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (out1, sink1) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let event = Event::from("this");
    in1.send(event.clone()).wait().unwrap();

    rt.block_on(topology.stop()).unwrap();

    let res = out1.collect().wait().unwrap();

    shutdown_on_idle(rt);
    assert_eq!(vec![event], res);
}

#[test]
fn topology_multiple_sources() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (in2, source2) = source();
    let (out1, sink1) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_source("in2", source2);
    config.add_sink("out1", &["in1", "in2"], sink1);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let event1 = Event::from("this");
    let event2 = Event::from("that");

    in1.send(event1.clone()).wait().unwrap();

    let (out_event1, out1) = out1.into_future().wait().unwrap();

    in2.send(event2.clone()).wait().unwrap();

    let (out_event2, _out1) = out1.into_future().wait().unwrap();

    rt.block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);

    assert_eq!(out_event1, Some(event1));
    assert_eq!(out_event2, Some(event2));
}

#[test]
fn topology_multiple_sinks() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (out1, sink1) = sink(10);
    let (out2, sink2) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);
    config.add_sink("out2", &["in1"], sink2);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let event = Event::from("this");

    in1.send(event.clone()).wait().unwrap();

    rt.block_on(topology.stop()).unwrap();

    let res1 = out1.collect().wait().unwrap();
    let res2 = out2.collect().wait().unwrap();

    shutdown_on_idle(rt);
    assert_eq!(vec![event.clone()], res1);
    assert_eq!(vec![event], res2);
}

#[test]
fn topology_transform_chain() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let transform1 = transform(" first", 0.0);
    let transform2 = transform(" second", 0.0);
    let (out1, sink1) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_transform("t2", &["t1"], transform2);
    config.add_sink("out1", &["t2"], sink1);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let event = Event::from("this");

    in1.send(event.clone()).wait().unwrap();

    rt.block_on(topology.stop()).unwrap();

    let res = out1.map(into_message).collect().wait().unwrap();

    shutdown_on_idle(rt);
    assert_eq!(vec!["this first second"], res);
}

#[test]
fn topology_remove_one_source() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (in2, source2) = source();
    let (_out1, sink1) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_source("in2", source2);
    config.add_sink("out1", &["in1", "in2"], sink1);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (out1, sink1) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_sink("out1", &["in1"], sink1);

    assert!(topology
        .reload_config_and_respawn(config, &mut rt, false)
        .unwrap());

    let event1 = Event::from("this");
    let event2 = Event::from("that");
    let h_out1 = oneshot::spawn(out1.collect(), &rt.executor());
    let h_in1 = oneshot::spawn(in1.send(event1.clone()), &rt.executor());
    let h_in2 = oneshot::spawn(in2.send(event2.clone()), &rt.executor());
    rt.block_on(h_in1).unwrap();
    rt.block_on(h_in2).unwrap_err();
    rt.block_on(topology.stop()).unwrap();
    let res = rt.block_on(h_out1).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(vec![event1], res);
}

#[test]
fn topology_remove_one_sink() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (out1, sink1) = sink(10);
    let (out2, sink2) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);
    config.add_sink("out2", &["in1"], sink2);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_sink("out1", &["in1"], sink(10).1);

    assert!(topology
        .reload_config_and_respawn(config, &mut rt, false)
        .unwrap());

    let event = Event::from("this");

    in1.send(event.clone()).wait().unwrap();

    rt.block_on(topology.stop()).unwrap();

    let res1 = out1.collect().wait().unwrap();
    let res2 = out2.collect().wait().unwrap();

    shutdown_on_idle(rt);
    assert_eq!(vec![event], res1);
    assert_eq!(Vec::<Event>::new(), res2);
}

#[test]
fn topology_remove_one_transform() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let transform1 = transform(" transformed", 0.0);
    let transform2 = transform(" transformed", 0.0);
    let (out1, sink1) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_transform("t2", &["t1"], transform2);
    config.add_sink("out1", &["t2"], sink1);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let transform2 = transform(" transformed", 0.0);

    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_transform("t2", &["in1"], transform2);
    config.add_sink("out1", &["t2"], sink(10).1);

    assert!(topology
        .reload_config_and_respawn(config, &mut rt, false)
        .unwrap());

    let event = Event::from("this");
    let h_out1 = oneshot::spawn(out1.map(into_message).collect(), &rt.executor());
    let h_in1 = oneshot::spawn(in1.send(event.clone()), &rt.executor());
    rt.block_on(h_in1).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res = rt.block_on(h_out1).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(vec!["this transformed"], res);
}

#[test]
fn topology_swap_source() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (out1v1, sink1v1) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1v1);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (in2, source2) = source();
    let (out1v2, sink1v2) = sink(10);

    let mut config = Config::empty();
    config.add_source("in2", source2);
    config.add_sink("out1", &["in2"], sink1v2);

    assert!(topology
        .reload_config_and_respawn(config, &mut rt, false)
        .unwrap());

    let event1 = Event::from("this");
    let event2 = Event::from("that");

    let h_out1v1 = oneshot::spawn(out1v1.collect(), &rt.executor());
    let h_out1v2 = oneshot::spawn(out1v2.collect(), &rt.executor());
    let h_in1 = oneshot::spawn(in1.send(event1.clone()), &rt.executor());
    let h_in2 = oneshot::spawn(in2.send(event2.clone()), &rt.executor());
    rt.block_on(h_in1).unwrap_err();
    rt.block_on(h_in2).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res1v1 = rt.block_on(h_out1v1).unwrap();
    let res1v2 = rt.block_on(h_out1v2).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(Vec::<Event>::new(), res1v1);
    assert_eq!(vec![event2], res1v2);
}

#[test]
fn topology_swap_sink() {
    trace_init();
    let mut rt = runtime();
    let (in1, source1) = source();
    let (out1, sink1) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (out2, sink2) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_sink("out2", &["in1"], sink2);

    assert!(topology
        .reload_config_and_respawn(config, &mut rt, false)
        .unwrap());

    let event = Event::from("this");
    let h_out1 = oneshot::spawn(out1.collect(), &rt.executor());
    let h_out2 = oneshot::spawn(out2.collect(), &rt.executor());
    let h_in1 = oneshot::spawn(in1.send(event.clone()), &rt.executor());
    rt.block_on(h_in1).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res1 = rt.block_on(h_out1).unwrap();
    let res2 = rt.block_on(h_out2).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(Vec::<Event>::new(), res1);
    assert_eq!(vec![event], res2);
}

#[test]
fn topology_swap_transform() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let transform1 = transform(" transformed", 0.0);
    let (out1v1, sink1v1) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_sink("out1", &["t1"], sink1v1);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let transform2 = transform(" replaced", 0.0);
    let (out1v2, sink1v2) = sink(10);

    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_transform("t2", &["in1"], transform2);
    config.add_sink("out1", &["t2"], sink1v2);

    assert!(topology
        .reload_config_and_respawn(config, &mut rt, false)
        .unwrap());

    let event = Event::from("this");
    let h_out1v1 = oneshot::spawn(out1v1.map(into_message).collect(), &rt.executor());
    let h_out1v2 = oneshot::spawn(out1v2.map(into_message).collect(), &rt.executor());
    let h_in1 = oneshot::spawn(in1.send(event.clone()), &rt.executor());
    rt.block_on(h_in1).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res1v1 = rt.block_on(h_out1v1).unwrap();
    let res1v2 = rt.block_on(h_out1v2).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(Vec::<String>::new(), res1v1);
    assert_eq!(vec!["this replaced"], res1v2);
}

#[ignore] // TODO: issue #2186
#[test]
fn topology_swap_transform_is_atomic() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let transform1v1 = transform(" transformed", 0.0);
    let (out1, sink1) = sink(10);

    let running = Arc::new(AtomicBool::new(true));
    let run_control = running.clone();

    let send_counter = Arc::new(AtomicUsize::new(0));
    let recv_counter = Arc::new(AtomicUsize::new(0));
    let send_total = send_counter.clone();
    let recv_total = recv_counter.clone();

    let events = move || match running.load(Ordering::Acquire) {
        true => {
            send_counter.fetch_add(1, Ordering::Release);
            Some(Event::from("this"))
        }
        false => None,
    };
    let input = iter_ok::<_, ()>(iter::from_fn(events));
    let input = input
        .forward(in1.sink_map_err(|e| panic!("{:?}", e)))
        .map(|_| ());
    let output = out1.map_err(|_| ()).for_each(move |_| {
        recv_counter.fetch_add(1, Ordering::Release);
        future::ok(())
    });
    let h_out = oneshot::spawn(output, &rt.executor());
    let h_in = oneshot::spawn(input, &rt.executor());

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1v1);
    config.add_sink("out1", &["t1"], sink1);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));

    let transform1v2 = transform(" replaced", 0.0);

    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_transform("t1", &["in1"], transform1v2);
    config.add_sink("out1", &["t1"], sink(10).1);

    assert!(topology
        .reload_config_and_respawn(config, &mut rt, false)
        .unwrap());
    std::thread::sleep(std::time::Duration::from_millis(10));

    run_control.store(false, Ordering::Release);
    rt.block_on(h_in).unwrap();
    rt.block_on(topology.stop()).unwrap();
    rt.block_on(h_out).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(
        send_total.load(Ordering::Acquire),
        recv_total.load(Ordering::Acquire)
    );
}

#[test]
fn topology_required_healthcheck_fails_start() {
    let config = basic_config_with_sink_failing_healthcheck();
    assert!(topology::start(config, &mut runtime(), true).is_none());
}

#[test]
fn topology_optional_healthcheck_does_not_fail_start() {
    let config = basic_config_with_sink_failing_healthcheck();
    assert!(topology::start(config, &mut runtime(), false).is_some());
}

#[test]
fn topology_optional_healthcheck_does_not_fail_reload() {
    let mut rt = runtime();
    let config = basic_config();
    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    let config = basic_config_with_sink_failing_healthcheck();
    assert!(topology
        .reload_config_and_respawn(config, &mut rt, false)
        .unwrap());
}

#[test]
fn topology_healthcheck_not_run_on_unchanged_reload() {
    let mut rt = runtime();
    let config = basic_config();
    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    let config = basic_config_with_sink_failing_healthcheck();
    assert!(topology
        .reload_config_and_respawn(config, &mut rt, true)
        .unwrap());
}

#[test]
fn topology_healthcheck_run_for_changes_on_reload() {
    let mut rt = runtime();

    let mut config = Config::empty();
    // We can't just drop the sender side since that will close the source.
    let (_ch0, src) = source();
    config.add_source("in1", src);
    config.add_sink("out1", &["in1"], sink(10).1);
    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let mut config = Config::empty();
    // We can't just drop the sender side since that will close the source.
    let (_ch1, src) = source();
    config.add_source("in1", src);
    config.add_sink("out2", &["in1"], sink_failing_healthcheck(10).1);

    assert!(
        topology
            .reload_config_and_respawn(config, &mut rt, true)
            .unwrap()
            == false
    );
}

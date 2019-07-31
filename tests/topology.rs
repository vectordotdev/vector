#[macro_use]
extern crate tracing;

pub mod support;

use crate::support::{sink, source, transform};
use futures::{sink::Sink, stream::Stream};
use vector::event::{Event, MESSAGE};
use vector::test_util::{runtime, shutdown_on_idle};
use vector::topology;
use vector::topology::config::Config;

fn into_message(event: Event) -> String {
    event.as_log().get(&MESSAGE).unwrap().to_string_lossy()
}

#[test]
fn topology_source_and_sink() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (out1, sink1) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let event = Event::from("this");
    rt.block_on(in1.send(event.clone())).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res = rt.block_on(out1.collect()).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(vec![event], res);
}

#[test]
fn topology_multiple_sources() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (in2, source2) = source();
    let (out1, sink1) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_source("in2", source2);
    config.add_sink("out1", &["in1", "in2"], sink1);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let event1 = Event::from("this");
    let event2 = Event::from("that");
    rt.block_on(in1.send(event1.clone())).unwrap();
    rt.block_on(in2.send(event2.clone())).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res = rt.block_on(out1.collect()).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(vec![event1, event2], res);
}

#[test]
fn topology_multiple_sinks() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (out1, sink1) = sink();
    let (out2, sink2) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);
    config.add_sink("out2", &["in1"], sink2);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let event = Event::from("this");
    rt.block_on(in1.send(event.clone())).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res1 = rt.block_on(out1.collect()).unwrap();
    let res2 = rt.block_on(out2.collect()).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(vec![event.clone()], res1);
    assert_eq!(vec![event], res2);
}

#[test]
fn topology_transform_chain() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let transform1 = transform(" first");
    let transform2 = transform(" second");
    let (out1, sink1) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_transform("t2", &["t1"], transform2);
    config.add_sink("out1", &["t2"], sink1);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let event = Event::from("this");
    rt.block_on(in1.send(event.clone())).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res = rt.block_on(out1.map(into_message).collect()).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(vec!["this first second"], res);
}

#[test]
fn topology_remove_one_source() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (in2, source2) = source();
    let (_out1, sink1) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_source("in2", source2);
    config.add_sink("out1", &["in1", "in2"], sink1);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (out1, sink1) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_sink("out1", &["in1"], sink1);

    assert!(topology.reload_config_and_respawn(config, &mut rt, false));

    let event1 = Event::from("this");
    let event2 = Event::from("that");
    rt.block_on(in1.send(event1.clone())).unwrap();
    rt.block_on(in2.send(event2.clone())).unwrap_err();
    rt.block_on(topology.stop()).unwrap();
    let res = rt.block_on(out1.collect()).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(vec![event1], res);
}

#[test]
fn topology_remove_one_sink() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (out1, sink1) = sink();
    let (out2, sink2) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);
    config.add_sink("out2", &["in1"], sink2);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_sink("out1", &["in1"], sink().1);

    assert!(topology.reload_config_and_respawn(config, &mut rt, false));

    let event = Event::from("this");
    rt.block_on(in1.send(event.clone())).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res1 = rt.block_on(out1.collect()).unwrap();
    let res2 = rt.block_on(out2.collect()).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(vec![event], res1);
    assert_eq!(Vec::<Event>::new(), res2);
}

#[test]
fn topology_remove_one_transform() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let transform1 = transform(" transformed");
    let transform2 = transform(" transformed");
    let (out1, sink1) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_transform("t2", &["t1"], transform2);
    config.add_sink("out1", &["t2"], sink1);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let transform2 = transform(" transformed");

    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_transform("t2", &["in1"], transform2);
    config.add_sink("out1", &["t2"], sink().1);

    assert!(topology.reload_config_and_respawn(config, &mut rt, false));

    let event = Event::from("this");
    rt.block_on(in1.send(event.clone())).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res = rt.block_on(out1.map(into_message).collect()).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(vec!["this transformed"], res);
}

#[test]
fn topology_swap_source() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (out1v1, sink1v1) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1v1);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (in2, source2) = source();
    let (out1v2, sink1v2) = sink();

    let mut config = Config::empty();
    config.add_source("in2", source2);
    config.add_sink("out1", &["in2"], sink1v2);

    assert!(topology.reload_config_and_respawn(config, &mut rt, false));

    let event1 = Event::from("this");
    let event2 = Event::from("that");
    rt.block_on(in1.send(event1.clone())).unwrap_err();
    rt.block_on(in2.send(event2.clone())).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res1v1 = rt.block_on(out1v1.collect()).unwrap();
    let res1v2 = rt.block_on(out1v2.collect()).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(Vec::<Event>::new(), res1v1);
    assert_eq!(vec![event2], res1v2);
}

#[test]
fn topology_swap_sink() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let (out1, sink1) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_sink("out1", &["in1"], sink1);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (out2, sink2) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_sink("out2", &["in1"], sink2);

    assert!(topology.reload_config_and_respawn(config, &mut rt, false));

    let event = Event::from("this");
    rt.block_on(in1.send(event.clone())).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res1 = rt.block_on(out1.collect()).unwrap();
    let res2 = rt.block_on(out2.collect()).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(Vec::<Event>::new(), res1);
    assert_eq!(vec![event], res2);
}

#[test]
fn topology_swap_transform() {
    let mut rt = runtime();
    let (in1, source1) = source();
    let transform1 = transform(" transformed");
    let (out1v1, sink1v1) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source1);
    config.add_transform("t1", &["in1"], transform1);
    config.add_sink("out1", &["t1"], sink1v1);

    let (mut topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let transform2 = transform(" replaced");
    let (out1v2, sink1v2) = sink();

    let mut config = Config::empty();
    config.add_source("in1", source().1);
    config.add_transform("t2", &["in1"], transform2);
    config.add_sink("out1", &["t2"], sink1v2);

    assert!(topology.reload_config_and_respawn(config, &mut rt, false));

    let event = Event::from("this");
    rt.block_on(in1.send(event.clone())).unwrap();
    rt.block_on(topology.stop()).unwrap();
    let res1v1 = rt.block_on(out1v1.map(into_message).collect()).unwrap();
    let res1v2 = rt.block_on(out1v2.map(into_message).collect()).unwrap();
    shutdown_on_idle(rt);
    assert_eq!(Vec::<String>::new(), res1v1);
    assert_eq!(vec!["this replaced"], res1v2);
}

mod builder;
pub mod config;
mod fanout;

pub use self::config::Config;

use crate::buffers;
use futures::{sync::oneshot, Future};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use stream_cancel::Trigger;
use tokio_trace_futures::Instrument;

pub struct Topology {
    state: State,
}

enum State {
    Ready(builder::Pieces, Config),
    Running(RunningTopology),
    Stopped,
}

#[allow(dead_code)]
struct RunningTopology {
    inputs: HashMap<String, buffers::BufferInputCloner>,
    outputs: HashMap<String, fanout::ControlChannel>,
    source_tasks: HashMap<String, oneshot::SpawnHandle<(), ()>>,
    shutdown_triggers: HashMap<String, Trigger>,
    config: Config,
}

impl Topology {
    pub fn build(config: Config) -> Result<(Self, Vec<String>), Vec<String>> {
        let (components, warnings) = builder::build_pieces(&config)?;

        let topology = Self {
            state: State::Ready(components, config),
        };

        Ok((topology, warnings))
    }

    pub fn healthchecks(&mut self) -> impl Future<Item = (), Error = ()> {
        if let State::Ready(ref mut components, _) = &mut self.state {
            let healthchecks = components
                .healthchecks
                .drain()
                .map(|(_, v)| v)
                .collect::<Vec<_>>();
            futures::future::join_all(healthchecks).map(|_| ())
        } else {
            // TODO: make healthchecks reusable
            unimplemented!("Can only run healthchecks before calling start");
        }
    }

    pub fn start(&mut self, rt: &mut tokio::runtime::Runtime) {
        let state = std::mem::replace(&mut self.state, State::Stopped);
        let (components, config) = if let State::Ready(components, config) = state {
            (components, config)
        } else {
            panic!("Can only call start once, immediately after building");
        };

        let builder::Pieces {
            inputs,
            outputs,
            shutdown_triggers,
            tasks,
            source_tasks,
            healthchecks: _healthchecks,
        } = components;

        let mut new_inputs = HashMap::new();
        for (name, (tx, input_names)) in inputs {
            for input_name in input_names {
                outputs[&input_name]
                    .unbounded_send(fanout::ControlMessage::Add(name.clone(), tx.get()))
                    .unwrap();
            }

            new_inputs.insert(name, tx);
        }

        for (name, task) in tasks.into_iter() {
            info!("Starting sink {}", name);
            rt.spawn(task.instrument(span!("sink", name = name.as_str())));
        }

        let source_tasks = source_tasks
            .into_iter()
            .map(|(name, task)| (name, oneshot::spawn(task, &rt.executor())))
            .collect();

        self.state = State::Running(RunningTopology {
            inputs: new_inputs,
            outputs,
            config,
            shutdown_triggers,
            source_tasks,
        });
    }

    pub fn stop(&mut self) {
        // Dropping inputs and shutdown_triggers will cause everything to start shutting down
        self.state = State::Stopped;
    }

    pub fn reload_config(&mut self, new_config: Config, rt: &mut tokio::runtime::Runtime) {
        if let State::Running(running) = &mut self.state {
            running.reload_config(new_config, rt);
        } else {
            panic!("Can only reload config on a running Topology");
        }
    }
}

impl RunningTopology {
    fn reload_config(&mut self, new_config: Config, rt: &mut tokio::runtime::Runtime) {
        info!("Reloading config");

        let mut new_pieces = match builder::build_pieces(&new_config) {
            Err(errors) => {
                for error in errors {
                    error!("Configuration error: {}", error);
                }
                return;
            }
            Ok((new_pieces, warnings)) => {
                for warning in warnings {
                    error!("Configuration warning: {}", warning);
                }
                new_pieces
            }
        };

        fn to_remove_change_add<C>(
            old: &IndexMap<String, C>,
            new: &IndexMap<String, C>,
        ) -> (HashSet<String>, HashSet<String>, HashSet<String>)
        where
            C: serde::Serialize + serde::Deserialize<'static>,
        {
            let old_names = old.keys().cloned().collect::<HashSet<_>>();
            let new_names = new.keys().cloned().collect::<HashSet<_>>();

            let to_change = old_names
                .intersection(&new_names)
                .filter(|&n| {
                    let old_toml = toml::Value::try_from(&old[n]).unwrap();
                    let new_toml = toml::Value::try_from(&new[n]).unwrap();
                    old_toml != new_toml
                })
                .cloned()
                .collect::<HashSet<_>>();

            let to_remove = &old_names - &new_names;
            let to_add = &new_names - &old_names;

            (to_remove, to_change, to_add)
        }

        // Sources
        let (sources_to_remove, sources_to_change, sources_to_add) =
            to_remove_change_add(&self.config.sources, &new_config.sources);

        for name in sources_to_remove {
            info!("Removing source {:?}", name);

            self.remove_outputs(&name);
            self.shutdown_source(&name);
        }

        for name in sources_to_change {
            info!("Rebuiling source {:?}", name);

            self.remove_outputs(&name);
            self.shutdown_source(&name);

            self.setup_outputs(&name, &mut new_pieces);

            self.spawn_source(&name, &mut new_pieces, rt);
            self.spawn(&name, &mut new_pieces, rt);
        }

        for name in sources_to_add {
            info!("Adding source {:?}", name);

            self.setup_outputs(&name, &mut new_pieces);
            self.spawn_source(&name, &mut new_pieces, rt);
            self.spawn(&name, &mut new_pieces, rt);
        }

        // Transforms
        let (transforms_to_remove, transforms_to_change, transforms_to_add) =
            to_remove_change_add(&self.config.transforms, &new_config.transforms);

        for name in transforms_to_remove {
            info!("Removing transform {:?}", name);

            self.remove_inputs(&name);
            self.remove_outputs(&name);
        }

        for name in transforms_to_change {
            info!("Rebuilding transform {:?}", name);

            self.spawn(&name, &mut new_pieces, rt);

            self.setup_outputs(&name, &mut new_pieces);
            self.replace_inputs(&name, &mut new_pieces);
        }

        for name in transforms_to_add {
            info!("Adding transform {:?}", name);

            self.setup_inputs(&name, &mut new_pieces);
            self.spawn(&name, &mut new_pieces, rt);
            self.setup_outputs(&name, &mut new_pieces);
        }

        // Sinks
        let (sinks_to_remove, sinks_to_change, sinks_to_add) =
            to_remove_change_add(&self.config.sinks, &new_config.sinks);

        for name in sinks_to_remove {
            info!("Removing sink {:?}", name);

            self.remove_inputs(&name);
        }

        for name in sinks_to_change {
            info!("Rebuilding sink {:?}", name);

            self.spawn(&name, &mut new_pieces, rt);
            self.replace_inputs(&name, &mut new_pieces);
        }

        for name in sinks_to_add {
            info!("Adding sink {:?}", name);

            self.setup_inputs(&name, &mut new_pieces);
            self.spawn(&name, &mut new_pieces, rt);
        }
    }

    fn spawn(
        &mut self,
        name: &String,
        new_pieces: &mut builder::Pieces,
        rt: &mut tokio::runtime::Runtime,
    ) {
        let task = new_pieces.tasks.remove(name).unwrap();
        rt.spawn(task);
    }

    fn spawn_source(
        &mut self,
        name: &String,
        new_pieces: &mut builder::Pieces,
        rt: &mut tokio::runtime::Runtime,
    ) {
        let shutdown_trigger = new_pieces.shutdown_triggers.remove(name).unwrap();
        self.shutdown_triggers
            .insert(name.clone(), shutdown_trigger);

        let source_task = new_pieces.source_tasks.remove(name).unwrap();
        self.source_tasks
            .insert(name.clone(), oneshot::spawn(source_task, &rt.executor()));
    }

    fn shutdown_source(&mut self, name: &String) {
        self.shutdown_triggers.remove(name).unwrap().cancel();
        self.source_tasks.remove(name).wait().unwrap();
    }

    fn remove_outputs(&mut self, name: &String) {
        self.outputs.remove(name);
    }

    fn remove_inputs(&mut self, name: &String) {
        self.inputs.remove(name);

        let sink_inputs = self.config.sinks.get(name).map(|s| &s.inputs);
        let trans_inputs = self.config.transforms.get(name).map(|t| &t.inputs);

        let inputs = sink_inputs.or(trans_inputs);

        if let Some(inputs) = inputs {
            for input in inputs {
                self.outputs[input]
                    .unbounded_send(fanout::ControlMessage::Remove(name.clone()))
                    .unwrap();
            }
        }
    }

    fn setup_outputs(&mut self, name: &String, new_pieces: &mut builder::Pieces) {
        let output = new_pieces.outputs.remove(name).unwrap();

        for (sink_name, sink) in &self.config.sinks {
            if sink.inputs.contains(&name) {
                output
                    .unbounded_send(fanout::ControlMessage::Add(
                        sink_name.clone(),
                        self.inputs[sink_name].get(),
                    ))
                    .unwrap();
            }
        }
        for (transform_name, transform) in &self.config.transforms {
            if transform.inputs.contains(&name) {
                output
                    .unbounded_send(fanout::ControlMessage::Add(
                        transform_name.clone(),
                        self.inputs[transform_name].get(),
                    ))
                    .unwrap();
            }
        }

        self.outputs.insert(name.clone(), output);
    }

    fn setup_inputs(&mut self, name: &String, new_pieces: &mut builder::Pieces) {
        let (tx, inputs) = new_pieces.inputs.remove(name).unwrap();
        // TODO: tx needs to get added to self.inputs, but I'm purposely holding off on doing
        // so until a test exposes this hole

        for input in inputs {
            self.outputs[&input]
                .unbounded_send(fanout::ControlMessage::Add(name.clone(), tx.get()))
                .unwrap();
        }
    }

    fn replace_inputs(&mut self, name: &String, new_pieces: &mut builder::Pieces) {
        let (tx, inputs) = new_pieces.inputs.remove(name).unwrap();

        let sink_inputs = self.config.sinks.get(name).map(|s| &s.inputs);
        let trans_inputs = self.config.transforms.get(name).map(|t| &t.inputs);
        let old_inputs = sink_inputs
            .or(trans_inputs)
            .unwrap()
            .into_iter()
            .collect::<HashSet<_>>();

        let new_inputs = inputs.iter().collect::<HashSet<_>>();

        let inputs_to_remove = &old_inputs - &new_inputs;
        let inputs_to_add = &new_inputs - &old_inputs;
        let inputs_to_replace = old_inputs.intersection(&new_inputs);

        for input in inputs_to_remove {
            if let Some(output) = self.outputs.get(input) {
                output
                    .unbounded_send(fanout::ControlMessage::Remove(name.clone()))
                    .unwrap();
            }
        }

        for input in inputs_to_add {
            self.outputs[input]
                .unbounded_send(fanout::ControlMessage::Add(name.clone(), tx.get()))
                .unwrap();
        }

        for &input in inputs_to_replace {
            self.outputs[input]
                .unbounded_send(fanout::ControlMessage::Replace(name.clone(), tx.get()))
                .unwrap();
        }

        self.inputs.insert(name.clone(), tx);
    }
}

#[cfg(test)]
mod tests {
    use crate::sinks::tcp::TcpSinkConfig;
    use crate::sources::tcp::TcpConfig;
    use crate::test_util::{
        next_addr, random_lines, receive_lines, receive_lines_with_count, send_lines,
        shutdown_on_idle, wait_for, wait_for_tcp,
    };
    use crate::topology::config::Config;
    use crate::topology::Topology;
    use crate::transforms::sampler::SamplerConfig;
    use futures::{stream, Future, Stream};
    use std::collections::HashSet;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use stream_cancel::{StreamExt, Tripwire};

    #[test]
    fn topology_add_sink() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let num_lines: usize = 100;

        let in_addr = next_addr();
        let out1_addr = next_addr();
        let out2_addr = next_addr();

        let (output_lines1, output_lines1_count) =
            receive_lines_with_count(&out1_addr, &rt.executor());
        let output_lines2 = receive_lines(&out2_addr, &rt.executor());

        let mut old_config = Config::empty();
        old_config.add_source("in", TcpConfig::new(in_addr));
        old_config.add_sink("out1", &["in"], TcpSinkConfig { address: out1_addr });
        let mut new_config = old_config.clone();
        let (mut topology, _warnings) = Topology::build(old_config).unwrap();

        topology.start(&mut rt);

        // Wait for server to accept traffic
        wait_for_tcp(in_addr);

        let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines1.clone().into_iter());
        rt.block_on(send).unwrap();

        new_config.add_sink("out2", &["in"], TcpSinkConfig { address: out2_addr });

        wait_for(|| output_lines1_count.load(Ordering::Relaxed) >= 100);

        topology.reload_config(new_config, &mut rt);

        let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines2.clone().into_iter());
        rt.block_on(send).unwrap();

        // Shut down server
        topology.stop();
        shutdown_on_idle(rt);

        let output_lines1 = output_lines1.wait().unwrap();
        assert_eq!(num_lines * 2, output_lines1.len());
        assert_eq!(input_lines1, &output_lines1[..num_lines]);
        assert_eq!(input_lines2, &output_lines1[num_lines..]);

        let output_lines2 = output_lines2.wait().unwrap();
        assert_eq!(num_lines, output_lines2.len());
        assert_eq!(input_lines2, output_lines2);
    }

    #[test]
    fn topology_remove_sink() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let num_lines: usize = 100;

        let in_addr = next_addr();
        let out1_addr = next_addr();
        let out2_addr = next_addr();

        let (output_lines1, output_lines1_count) =
            receive_lines_with_count(&out1_addr, &rt.executor());
        let output_lines2 = receive_lines(&out2_addr, &rt.executor());

        let mut old_config = Config::empty();
        old_config.add_source("in", TcpConfig::new(in_addr));
        old_config.add_sink("out1", &["in"], TcpSinkConfig { address: out1_addr });
        old_config.add_sink("out2", &["in"], TcpSinkConfig { address: out2_addr });
        let mut new_config = old_config.clone();
        let (mut topology, _warnings) = Topology::build(old_config).unwrap();

        topology.start(&mut rt);

        // Wait for server to accept traffic
        wait_for_tcp(in_addr);

        let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines1.clone().into_iter());
        rt.block_on(send).unwrap();

        new_config.sinks.remove(&"out2".to_string());

        wait_for(|| output_lines1_count.load(Ordering::Relaxed) >= 100);

        topology.reload_config(new_config, &mut rt);

        // out2 should disconnect after the reload
        let output_lines2 = output_lines2.wait().unwrap();

        let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines2.clone().into_iter());
        rt.block_on(send).unwrap();

        // Shut down server
        topology.stop();
        shutdown_on_idle(rt);

        let output_lines1 = output_lines1.wait().unwrap();
        assert_eq!(num_lines * 2, output_lines1.len());
        assert_eq!(input_lines1, &output_lines1[..num_lines]);
        assert_eq!(input_lines2, &output_lines1[num_lines..]);

        assert_eq!(num_lines, output_lines2.len());
        assert_eq!(input_lines1, output_lines2);
    }

    #[test]
    fn topology_change_sink() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let num_lines: usize = 100;

        let in_addr = next_addr();
        let out1_addr = next_addr();
        let out2_addr = next_addr();

        let (output_lines1, output_lines1_count) =
            receive_lines_with_count(&out1_addr, &rt.executor());
        let output_lines2 = receive_lines(&out2_addr, &rt.executor());

        let mut old_config = Config::empty();
        old_config.add_source("in", TcpConfig::new(in_addr));
        old_config.add_sink("out", &["in"], TcpSinkConfig { address: out1_addr });
        let mut new_config = old_config.clone();
        let (mut topology, _warnings) = Topology::build(old_config).unwrap();

        topology.start(&mut rt);

        // Wait for server to accept traffic
        wait_for_tcp(in_addr);

        let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines1.clone().into_iter());
        rt.block_on(send).unwrap();

        new_config.sinks[&"out".to_string()].inner = Box::new(TcpSinkConfig { address: out2_addr });

        wait_for(|| output_lines1_count.load(Ordering::Relaxed) >= 100);

        topology.reload_config(new_config, &mut rt);

        let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines2.clone().into_iter());
        rt.block_on(send).unwrap();

        // Shut down server
        topology.stop();
        shutdown_on_idle(rt);

        let output_lines1 = output_lines1.wait().unwrap();
        assert_eq!(num_lines, output_lines1.len());
        assert_eq!(input_lines1, output_lines1);

        let output_lines2 = output_lines2.wait().unwrap();
        assert_eq!(num_lines, output_lines2.len());
        assert_eq!(input_lines2, output_lines2);
    }

    // The previous test pauses to make sure the old version of the sink has receieved all messages
    // sent before the reload. This test does not pause, making sure the new sink is atomically
    // swapped in for the old one and that no records are lost in the changeover.
    #[test]
    fn topology_change_sink_no_gap() {
        let in_addr = next_addr();
        let out1_addr = next_addr();
        let out2_addr = next_addr();

        for _ in 0..10 {
            let mut rt = tokio::runtime::Runtime::new().unwrap();

            let (output_lines1, output_lines1_count) =
                receive_lines_with_count(&out1_addr, &rt.executor());
            let (output_lines2, output_lines2_count) =
                receive_lines_with_count(&out2_addr, &rt.executor());

            let mut old_config = Config::empty();
            old_config.add_source("in", TcpConfig::new(in_addr));
            old_config.add_sink("out", &["in"], TcpSinkConfig { address: out1_addr });
            let mut new_config = old_config.clone();
            let (mut topology, _warnings) = Topology::build(old_config).unwrap();

            topology.start(&mut rt);

            // Wait for server to accept traffic
            wait_for_tcp(in_addr);

            let (input_trigger, input_tripwire) = Tripwire::new();

            let num_input_lines = Arc::new(AtomicUsize::new(0));
            let num_input_lines2 = Arc::clone(&num_input_lines);
            let input_lines = stream::iter_ok(random_lines(100))
                .take_until(input_tripwire)
                .inspect(move |_| {
                    num_input_lines2.fetch_add(1, Ordering::Relaxed);
                })
                .wait()
                .map(|r: Result<String, ()>| r.unwrap());

            let send = send_lines(in_addr, input_lines);
            rt.spawn(send);

            new_config.sinks[&"out".to_string()].inner =
                Box::new(TcpSinkConfig { address: out2_addr });

            wait_for(|| output_lines1_count.load(Ordering::Relaxed) > 0);

            topology.reload_config(new_config, &mut rt);
            wait_for(|| output_lines2_count.load(Ordering::Relaxed) > 0);

            // Shut down server
            input_trigger.cancel();
            topology.stop();
            let output_lines1 = output_lines1.wait().unwrap();
            let output_lines2 = output_lines2.wait().unwrap();
            shutdown_on_idle(rt);

            assert!(output_lines1.len() > 0);
            assert!(output_lines2.len() > 0);

            assert_eq!(
                num_input_lines.load(Ordering::Relaxed),
                output_lines1.len() + output_lines2.len()
            );
        }
    }

    #[test]
    fn topology_add_source() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let num_lines: usize = 100;

        let in_addr = next_addr();
        let out_addr = next_addr();

        let output_lines = receive_lines(&out_addr, &rt.executor());

        let mut old_config = Config::empty();
        old_config.add_sink("out", &[], TcpSinkConfig { address: out_addr });
        let mut new_config = old_config.clone();
        let (mut topology, _warnings) = Topology::build(old_config).unwrap();

        topology.start(&mut rt);

        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(std::net::TcpStream::connect(in_addr).is_err());

        new_config.add_source("in", TcpConfig::new(in_addr));
        new_config.sinks[&"out".to_string()]
            .inputs
            .push("in".to_string());

        topology.reload_config(new_config, &mut rt);

        wait_for_tcp(in_addr);

        let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines.clone().into_iter());
        rt.block_on(send).unwrap();

        // Shut down server
        topology.stop();
        shutdown_on_idle(rt);

        let output_lines = output_lines.wait().unwrap();
        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[test]
    fn topology_remove_source() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let num_lines: usize = 100;

        let in_addr = next_addr();
        let out_addr = next_addr();

        let output_lines = receive_lines(&out_addr, &rt.executor());

        let mut old_config = Config::empty();
        old_config.add_source("in", TcpConfig::new(in_addr));
        old_config.add_sink("out", &["in"], TcpSinkConfig { address: out_addr });
        let mut new_config = old_config.clone();
        let (mut topology, _warnings) = Topology::build(old_config).unwrap();

        topology.start(&mut rt);

        wait_for_tcp(in_addr);

        let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines.clone().into_iter());
        rt.block_on(send).unwrap();

        new_config.sources.remove(&"in".to_string());
        new_config.sinks[&"out".to_string()].inputs.clear();

        topology.reload_config(new_config, &mut rt);

        wait_for(|| std::net::TcpStream::connect(in_addr).is_err());

        // Shut down server
        topology.stop();
        shutdown_on_idle(rt);

        let output_lines = output_lines.wait().unwrap();
        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[test]
    fn topology_remove_source_add_source_with_same_port() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let num_lines: usize = 100;

        let in_addr = next_addr();
        let out_addr = next_addr();

        let (output_lines1, output_lines1_count) =
            receive_lines_with_count(&out_addr, &rt.executor());

        let mut old_config = Config::empty();
        old_config.add_source("in1", TcpConfig::new(in_addr));
        old_config.add_sink("out", &["in1"], TcpSinkConfig { address: out_addr });
        let mut new_config = old_config.clone();
        let (mut topology, _warnings) = Topology::build(old_config).unwrap();

        topology.start(&mut rt);

        wait_for_tcp(in_addr);

        let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines1.clone().into_iter());
        rt.block_on(send).unwrap();

        wait_for(|| output_lines1_count.load(Ordering::Relaxed) >= num_lines);

        new_config.sources.remove(&"in1".to_string());
        new_config.add_source("in2", TcpConfig::new(in_addr));
        new_config.sinks[&"out".to_string()].inputs = vec!["in2".to_string()];

        topology.reload_config(new_config, &mut rt);

        // The sink gets rebuilt, causing it to open a new connection
        let output_lines1 = output_lines1.wait().unwrap();
        let output_lines2 = receive_lines(&out_addr, &rt.executor());

        let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines2.clone().into_iter());
        rt.block_on(send).unwrap();

        // Shut down server
        topology.stop();
        shutdown_on_idle(rt);

        assert_eq!(num_lines, output_lines1.len());
        assert_eq!(input_lines1, output_lines1);

        let output_lines2 = output_lines2.wait().unwrap();
        assert_eq!(num_lines, output_lines2.len());
        assert_eq!(input_lines2, output_lines2);
    }

    #[test]
    fn topology_change_source() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let in_addr = next_addr();
        let out_addr = next_addr();

        let (output_lines, output_lines_count) =
            receive_lines_with_count(&out_addr, &rt.executor());

        let mut old_config = Config::empty();
        old_config.add_source(
            "in",
            TcpConfig {
                address: in_addr,
                max_length: 20,
            },
        );
        old_config.add_sink("out", &["in"], TcpSinkConfig { address: out_addr });
        let mut new_config = old_config.clone();
        let (mut topology, _warnings) = Topology::build(old_config).unwrap();

        topology.start(&mut rt);

        wait_for_tcp(in_addr);

        let input_lines1 = vec![
            "short1",
            "mediummedium1",
            "longlonglonglonglonglong1",
            "mediummedium2",
            "short2",
        ];

        for &line in &input_lines1 {
            rt.block_on(send_lines(in_addr, std::iter::once(line.to_owned())))
                .unwrap();
        }

        wait_for(|| output_lines_count.load(Ordering::Relaxed) >= 4);

        new_config.sources[&"in".to_string()] = Box::new(TcpConfig {
            address: in_addr,
            max_length: 10,
        });

        topology.reload_config(new_config, &mut rt);

        std::thread::sleep(std::time::Duration::from_millis(50));
        wait_for_tcp(in_addr);

        let input_lines2 = vec![
            "short3",
            "mediummedium3",
            "longlonglonglonglonglong2",
            "mediummedium4",
            "short4",
        ];

        for &line in &input_lines2 {
            rt.block_on(send_lines(in_addr, std::iter::once(line.to_owned())))
                .unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Shut down server
        topology.stop();
        shutdown_on_idle(rt);

        let output_lines = output_lines
            .wait()
            .unwrap()
            .into_iter()
            .collect::<HashSet<_>>();
        assert_eq!(
            output_lines,
            vec![
                // From old
                "short1".to_owned(),
                "mediummedium1".to_owned(),
                "mediummedium2".to_owned(),
                "short2".to_owned(),
                // From new
                "short3".to_owned(),
                "short4".to_owned(),
            ]
            .into_iter()
            .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn topology_add_transform() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let num_lines: usize = 100;

        let in_addr = next_addr();
        let out_addr = next_addr();

        let (output_lines1, output_lines1_count) =
            receive_lines_with_count(&out_addr, &rt.executor());

        let mut old_config = Config::empty();
        old_config.add_source("in", TcpConfig::new(in_addr));
        old_config.add_sink("out", &["in"], TcpSinkConfig { address: out_addr });
        let mut new_config = old_config.clone();
        let (mut topology, _warnings) = Topology::build(old_config).unwrap();

        topology.start(&mut rt);

        // Wait for server to accept traffic
        wait_for_tcp(in_addr);

        let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines1.clone().into_iter());
        rt.block_on(send).unwrap();

        new_config.add_transform(
            "sampler",
            &["in"],
            SamplerConfig {
                rate: 2,
                pass_list: vec![],
            },
        );
        new_config.sinks[&"out".to_string()].inputs = vec!["sampler".to_string()];

        wait_for(|| output_lines1_count.load(Ordering::Relaxed) >= num_lines);

        topology.reload_config(new_config, &mut rt);

        // The sink gets rebuilt, causing it to open a new connection
        let output_lines1 = output_lines1.wait().unwrap();
        let output_lines2 = receive_lines(&out_addr, &rt.executor());

        let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines2.clone().into_iter());
        rt.block_on(send).unwrap();

        // Shut down server
        topology.stop();
        shutdown_on_idle(rt);

        assert_eq!(num_lines, output_lines1.len());
        assert_eq!(input_lines1, output_lines1);

        let output_lines2 = output_lines2.wait().unwrap();
        assert!(output_lines2.len() > 0);
        assert!(output_lines2.len() < num_lines);
    }

    #[test]
    fn topology_remove_transform() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let num_lines: usize = 100;

        let in_addr = next_addr();
        let out_addr = next_addr();

        let (output_lines1, output_lines1_count) =
            receive_lines_with_count(&out_addr, &rt.executor());

        let mut old_config = Config::empty();
        old_config.add_source("in", TcpConfig::new(in_addr));
        old_config.add_transform(
            "sampler",
            &["in"],
            SamplerConfig {
                rate: 2,
                pass_list: vec![],
            },
        );
        old_config.add_sink("out", &["sampler"], TcpSinkConfig { address: out_addr });
        let mut new_config = old_config.clone();
        let (mut topology, _warnings) = Topology::build(old_config).unwrap();

        topology.start(&mut rt);

        // Wait for server to accept traffic
        wait_for_tcp(in_addr);

        let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines1.clone().into_iter());
        rt.block_on(send).unwrap();

        new_config.transforms.remove(&"sampler".to_string());
        new_config.sinks[&"out".to_string()].inputs = vec!["in".to_string()];

        wait_for(|| output_lines1_count.load(Ordering::Relaxed) >= 1);
        std::thread::sleep(std::time::Duration::from_millis(50));

        topology.reload_config(new_config, &mut rt);

        // The sink gets rebuilt, causing it to open a new connection
        let output_lines1 = output_lines1.wait().unwrap();
        let output_lines2 = receive_lines(&out_addr, &rt.executor());

        let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines2.clone().into_iter());
        rt.block_on(send).unwrap();

        // Shut down server
        topology.stop();
        shutdown_on_idle(rt);

        assert!(output_lines1.len() > 0);
        assert!(output_lines1.len() < num_lines);

        let output_lines2 = output_lines2.wait().unwrap();
        assert_eq!(num_lines, output_lines2.len());
        assert_eq!(input_lines2, output_lines2);
    }

    #[test]
    fn topology_change_transform() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let num_lines: usize = 100;

        let in_addr = next_addr();
        let out_addr = next_addr();

        let (output_lines, output_lines_count) =
            receive_lines_with_count(&out_addr, &rt.executor());

        let mut old_config = Config::empty();
        old_config.add_source("in", TcpConfig::new(in_addr));
        old_config.add_transform(
            "sampler",
            &["in"],
            SamplerConfig {
                rate: 2,
                pass_list: vec![],
            },
        );
        old_config.add_sink("out", &["sampler"], TcpSinkConfig { address: out_addr });
        let mut new_config = old_config.clone();
        let (mut topology, _warnings) = Topology::build(old_config).unwrap();

        topology.start(&mut rt);

        // Wait for server to accept traffic
        wait_for_tcp(in_addr);

        let input_lines1 = random_lines(100)
            .map(|s| s + "before")
            .take(num_lines)
            .collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines1.clone().into_iter());
        rt.block_on(send).unwrap();

        wait_for(|| output_lines_count.load(Ordering::Relaxed) >= 1);
        std::thread::sleep(std::time::Duration::from_millis(50));

        new_config.transforms[&"sampler".to_string()].inner = Box::new(SamplerConfig {
            rate: 10,
            pass_list: vec![],
        });

        topology.reload_config(new_config, &mut rt);

        std::thread::sleep(std::time::Duration::from_millis(50));

        let input_lines2 = random_lines(100)
            .map(|s| s + "after")
            .take(num_lines)
            .collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines2.clone().into_iter());
        rt.block_on(send).unwrap();

        // Shut down server
        topology.stop();
        shutdown_on_idle(rt);

        let output_lines = output_lines.wait().unwrap();

        let num_before = output_lines
            .iter()
            .filter(|&l| l.ends_with("before"))
            .count();
        let num_after = output_lines
            .iter()
            .filter(|&l| l.ends_with("after"))
            .count();

        assert!(num_before > 0);
        assert!(num_before < num_lines);

        assert!(num_after > 0);
        assert!(num_after < num_lines);

        assert!(num_before > num_after);
    }
}

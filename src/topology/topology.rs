use super::{builder, fanout, Config};
use crate::buffers;
use futures::Future;
use log::{error, info};
use std::collections::{HashMap, HashSet};
use stream_cancel::Trigger;

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

        for task in tasks.into_iter().flat_map(|(_, ts)| ts) {
            rt.spawn(task);
        }

        self.state = State::Running(RunningTopology {
            inputs: new_inputs,
            outputs,
            config,
            shutdown_triggers,
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

        let old_config = &self.config;

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

        // Sources
        let old_source_names: HashSet<&String> = old_config.sources.keys().collect::<HashSet<_>>();
        let new_source_names: HashSet<&String> = new_config.sources.keys().collect::<HashSet<_>>();

        let sources_to_remove = &old_source_names - &new_source_names;
        let sources_to_add = &new_source_names - &old_source_names;

        for name in sources_to_remove {
            info!("Removing source {:?}", name);

            self.shutdown_triggers.remove(name).unwrap().cancel();
            self.outputs.remove(name);
        }

        for name in sources_to_add {
            info!("Adding source {:?}", name);

            self.shutdown_triggers.insert(
                name.clone(),
                new_pieces.shutdown_triggers.remove(name).unwrap(),
            );

            self.outputs
                .insert(name.clone(), new_pieces.outputs.remove(name).unwrap());

            let tasks = new_pieces.tasks.remove(name).unwrap();
            for task in tasks {
                rt.spawn(task);
            }
        }

        // Sinks
        let old_sink_names: HashSet<&String> = old_config.sinks.keys().collect::<HashSet<_>>();
        let new_sink_names: HashSet<&String> = new_config.sinks.keys().collect::<HashSet<_>>();

        let sinks_to_change: HashSet<&String> = old_sink_names
            .intersection(&new_sink_names)
            .filter(|&&n| {
                let old_toml = toml::Value::try_from(&old_config.sinks[n]).unwrap();
                let new_toml = toml::Value::try_from(&new_config.sinks[n]).unwrap();
                old_toml != new_toml
            })
            .map(|&n| n)
            .collect::<HashSet<_>>();

        let sinks_to_remove = &old_sink_names - &new_sink_names;
        let sinks_to_add = &new_sink_names - &old_sink_names;

        for name in sinks_to_remove {
            info!("Removing sink {:?}", name);

            self.inputs.remove(name);

            for input in &old_config.sinks[name].inputs {
                self.outputs[input]
                    .unbounded_send(fanout::ControlMessage::Remove(name.clone()))
                    .unwrap();
            }
        }

        for name in sinks_to_change {
            info!("Rebuilding sink {:?}", name);

            let name = name.to_owned();
            let (tx, _) = new_pieces.inputs.remove(&name).unwrap();

            let tasks = new_pieces.tasks.remove(&name).unwrap();
            for task in tasks {
                rt.spawn(task);
            }

            let old_inputs = old_config.sinks[&name]
                .inputs
                .iter()
                .collect::<HashSet<_>>();
            let new_inputs = new_config.sinks[&name]
                .inputs
                .iter()
                .collect::<HashSet<_>>();

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
        }

        for name in sinks_to_add {
            info!("Adding sink {:?}", name);

            let name = name.to_owned();
            let (tx, inputs) = new_pieces.inputs.remove(&name).unwrap();
            // TODO: tx needs to get added to self.inputs, but I'm purposely holding off on doing
            // so until a test exposes this hole

            let tasks = new_pieces.tasks.remove(&name).unwrap();
            for task in tasks {
                rt.spawn(task);
            }
            for input in inputs {
                self.outputs[&input]
                    .unbounded_send(fanout::ControlMessage::Add(name.clone(), tx.get()))
                    .unwrap();
            }
        }
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
    use futures::{stream, Future, Stream};
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
        for _ in 0..10 {
            let mut rt = tokio::runtime::Runtime::new().unwrap();

            let in_addr = next_addr();
            let out1_addr = next_addr();
            let out2_addr = next_addr();

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
}

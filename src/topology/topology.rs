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

        let old_sink_names = old_config.sinks.keys().collect::<HashSet<_>>();
        let new_sink_names = new_config.sinks.keys().collect::<HashSet<_>>();
        let sinks_to_add = &new_sink_names - &old_sink_names;

        for name in sinks_to_add {
            info!("Adding sink {:?}", name);

            let name = name.to_owned();
            let (tx, inputs) = new_pieces.inputs.remove(&name).unwrap();

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
        next_addr, random_lines, receive_lines, send_lines, shutdown_on_idle, wait_for_tcp,
    };
    use crate::topology::config::Config;
    use crate::topology::Topology;
    use futures::Future;

    #[test]
    fn topology_add_sink() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let num_lines: usize = 100;

        let in_addr = next_addr();
        let out1_addr = next_addr();
        let out2_addr = next_addr();

        let output_lines1 = receive_lines(&out1_addr, &rt.executor());
        let output_lines2 = receive_lines(&out2_addr, &rt.executor());

        let mut old_config = Config::empty();
        old_config.add_source("in", TcpConfig::new(in_addr));
        old_config.add_sink("out1", &["in"], TcpSinkConfig { address: out1_addr });
        let (mut topology, _warnings) = Topology::build(old_config).unwrap();

        topology.start(&mut rt);

        // Wait for server to accept traffic
        wait_for_tcp(in_addr);

        let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let send = send_lines(in_addr, input_lines1.clone().into_iter());
        rt.block_on(send).unwrap();

        let mut new_config = Config::empty();
        new_config.add_source("in", TcpConfig::new(in_addr));
        new_config.add_sink("out1", &["in"], TcpSinkConfig { address: out1_addr });
        new_config.add_sink("out2", &["in"], TcpSinkConfig { address: out2_addr });

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
        assert!(output_lines2.len() >= num_lines);
        assert!(output_lines2.ends_with(&input_lines2));
    }
}

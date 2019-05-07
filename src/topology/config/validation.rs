use crate::topology::{config::DataType, Config};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
enum Node {
    Source {
        ty: DataType,
    },
    Transform {
        in_ty: DataType,
        out_ty: DataType,
        inputs: Vec<String>,
    },
    Sink {
        ty: DataType,
        inputs: Vec<String>,
    },
}

pub fn typecheck(config: &Config) -> Result<(), Vec<String>> {
    let mut nodes = HashMap::new();

    // TODO: validate that node names are unique across sources/transforms/sinks?
    for (name, config) in config.sources.iter() {
        nodes.insert(
            name,
            Node::Source {
                ty: config.output_type(),
            },
        );
    }

    for (name, config) in config.transforms.iter() {
        nodes.insert(
            name,
            Node::Transform {
                in_ty: config.inner.input_type(),
                out_ty: config.inner.output_type(),
                inputs: config.inputs.clone(),
            },
        );
    }

    for (name, config) in config.sinks.iter() {
        nodes.insert(
            name,
            Node::Sink {
                ty: config.inner.input_type(),
                inputs: config.inputs.clone(),
            },
        );
    }

    let paths = config
        .sinks
        .keys()
        .flat_map(|node| paths(&nodes, node, Vec::new()))
        .collect::<Vec<_>>();

    let mut errors = Vec::new();

    for path in paths {
        for pair in path.windows(2) {
            let (x, y) = (&pair[0], &pair[1]);
            if nodes.get(x).is_none() || nodes.get(y).is_none() {
                continue;
            }
            match (nodes[&x].clone(), nodes[&y].clone()) {
                (Node::Source { ty: ty1 }, Node::Sink { ty: ty2, .. })
                | (Node::Source { ty: ty1 }, Node::Transform { in_ty: ty2, .. })
                | (Node::Transform { out_ty: ty1, .. }, Node::Transform { in_ty: ty2, .. })
                | (Node::Transform { out_ty: ty1, .. }, Node::Sink { ty: ty2, .. }) => {
                    if ty1 != ty2 {
                        errors.push(format!(
                            "Data type mismatch between {} ({:?}) and {} ({:?})",
                            x, ty1, y, ty2
                        ));
                    }
                }
                (Node::Sink { .. }, _) | (_, Node::Source { .. }) => unreachable!(),
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        errors.sort();
        errors.dedup();
        Err(errors)
    }
}

fn paths(nodes: &HashMap<&String, Node>, node: &String, mut path: Vec<String>) -> Vec<Vec<String>> {
    path.push(node.clone());
    match nodes.get(node).clone() {
        Some(Node::Source { .. }) | None => {
            path.reverse();
            vec![path]
        }
        Some(Node::Transform { inputs, .. }) | Some(Node::Sink { inputs, .. }) => inputs
            .iter()
            .flat_map(|input| paths(nodes, input, path.clone()))
            .collect(),
    }
}

// Modified version of Kahn's topological sort algorithm that ignores the actual sorted output and
// only cares if the sort was possible (i.e. whether or not there was a cycle in the input graph).
pub fn contains_cycle(config: &Config) -> bool {
    let nodes = config
        .sources
        .keys()
        .chain(config.transforms.keys())
        .chain(config.sinks.keys())
        .collect::<HashSet<_>>();

    let mut edges = HashSet::new();
    for (name, transform) in config.transforms.iter() {
        for input in transform.inputs.iter() {
            if nodes.contains(input) {
                edges.insert((input, name));
            }
        }
    }
    for (name, sink) in config.sinks.iter() {
        for input in sink.inputs.iter() {
            if nodes.contains(input) {
                edges.insert((input, name));
            }
        }
    }

    let mut no_incoming = nodes
        .into_iter()
        .filter(|n| !edges.iter().any(|(_t, h)| h == n))
        .collect::<Vec<_>>();
    while let Some(node) = no_incoming.pop() {
        let outgoing = edges
            .clone()
            .into_iter()
            .filter(|(tail, _head)| tail == &node)
            .collect::<Vec<_>>();
        for edge in outgoing {
            edges.remove(&edge);
            let successor = edge.1;
            if edges.iter().filter(|(_t, head)| head == &successor).count() == 0 {
                no_incoming.push(successor);
            }
        }
    }
    !edges.is_empty()
}

#[cfg(test)]
mod test {
    use super::{contains_cycle, typecheck};
    use crate::topology::Config;

    #[test]
    fn detects_cycles() {
        let cyclic = Config::load(
            r#"
            [sources.in]
            type = "tcp"
            address = "127.0.0.1:1235"

            [transforms.one]
            type = "sampler"
            inputs = ["in", "three"]
            rate = 10
            pass_list = []

            [transforms.two]
            type = "sampler"
            inputs = ["one"]
            rate = 10
            pass_list = []

            [transforms.three]
            type = "sampler"
            inputs = ["two"]
            rate = 10
            pass_list = []

            [sinks.out]
            type = "tcp"
            inputs = ["three"]
            address = "127.0.0.1:9999"
          "#
            .as_bytes(),
        )
        .unwrap();

        assert_eq!(true, contains_cycle(&cyclic));
    }

    #[test]
    fn doesnt_detect_noncycles() {
        let acyclic = Config::load(
            r#"
            [sources.in]
            type = "tcp"
            address = "127.0.0.1:1235"

            [transforms.one]
            type = "sampler"
            inputs = ["in"]
            rate = 10
            pass_list = []

            [transforms.two]
            type = "sampler"
            inputs = ["in"]
            rate = 10
            pass_list = []

            [transforms.three]
            type = "sampler"
            inputs = ["one", "two"]
            rate = 10
            pass_list = []

            [sinks.out]
            type = "tcp"
            inputs = ["three"]
            address = "127.0.0.1:9999"
          "#
            .as_bytes(),
        )
        .unwrap();

        assert_eq!(false, contains_cycle(&acyclic));
    }

    #[test]
    fn detects_type_mismatches() {
        // Define a "minimal" Metric-typed sink so our example config can trigger a type error. As
        // soon as we've actually implemented a Metric-typed sink or transform, we can get rid of
        // this and just use one of those.
        // TODO: remove all this once we have an actual non-Log component
        use crate::{
            buffers::Acker,
            sinks::{
                blackhole::{BlackholeConfig, BlackholeSink},
                Healthcheck, RouterSink,
            },
            topology::config::DataType,
        };
        use serde::{Deserialize, Serialize};

        #[derive(Deserialize, Serialize, Debug)]
        pub struct MetricsSinkConfig;

        #[typetag::serde(name = "metrics_sink")]
        impl crate::topology::config::SinkConfig for MetricsSinkConfig {
            fn build(&self, acker: Acker) -> Result<(RouterSink, Healthcheck), String> {
                Ok((
                    Box::new(BlackholeSink::new(
                        BlackholeConfig { print_amount: 1 },
                        acker,
                    )),
                    Box::new(futures::future::ok(())),
                ))
            }

            fn input_type(&self) -> DataType {
                DataType::Metric
            }
        }
        // End of stuff to delete

        let badly_typed = Config::load(
            r#"
            [sources.in]
            type = "tcp"
            address = "127.0.0.1:1235"

            [sinks.out]
            type = "metrics_sink"
            inputs = ["in"]
          "#
            .as_bytes(),
        )
        .unwrap();

        assert_eq!(
            Err(vec![
                "Data type mismatch between in (Log) and out (Metric)".into()
            ]),
            typecheck(&badly_typed)
        );
    }
}

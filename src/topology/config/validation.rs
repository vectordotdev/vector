use crate::topology::{config::DataType, Config};
use std::collections::{HashMap, HashSet};

pub fn typecheck(config: &Config) -> Result<(), Vec<String>> {
    Graph::from(config).typecheck()
}

pub fn contains_cycle(config: &Config) -> bool {
    Graph::from(config).contains_cycle()
}

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

#[derive(Default)]
struct Graph {
    nodes: HashMap<String, Node>,
}

impl Graph {
    fn add_source(&mut self, name: &str, ty: DataType) {
        self.nodes.insert(name.to_string(), Node::Source { ty });
    }

    fn add_transform(
        &mut self,
        name: &str,
        in_ty: DataType,
        out_ty: DataType,
        inputs: Vec<impl Into<String>>,
    ) {
        let inputs = self.clean_inputs(inputs);
        self.nodes.insert(
            name.to_string(),
            Node::Transform {
                in_ty,
                out_ty,
                inputs,
            },
        );
    }

    fn add_sink(&mut self, name: &str, ty: DataType, inputs: Vec<impl Into<String>>) {
        let inputs = self.clean_inputs(inputs);
        self.nodes
            .insert(name.to_string(), Node::Sink { ty, inputs });
    }

    fn paths(&self) -> Vec<Vec<String>> {
        self.nodes
            .iter()
            .filter_map(|(name, node)| match node {
                Node::Sink { .. } => Some(name),
                _ => None,
            })
            .flat_map(|node| paths_rec(&self.nodes, node, Vec::new()))
            .collect()
    }

    fn clean_inputs(&self, inputs: Vec<impl Into<String>>) -> Vec<String> {
        inputs.into_iter().map(Into::into).collect()
    }

    fn typecheck(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        for path in self.paths() {
            for pair in path.windows(2) {
                let (x, y) = (&pair[0], &pair[1]);
                if self.nodes.get(x).is_none() || self.nodes.get(y).is_none() {
                    continue;
                }
                match (self.nodes[x].clone(), self.nodes[y].clone()) {
                    (Node::Source { ty: ty1 }, Node::Sink { ty: ty2, .. })
                    | (Node::Source { ty: ty1 }, Node::Transform { in_ty: ty2, .. })
                    | (Node::Transform { out_ty: ty1, .. }, Node::Transform { in_ty: ty2, .. })
                    | (Node::Transform { out_ty: ty1, .. }, Node::Sink { ty: ty2, .. }) => {
                        if ty1 != ty2 && ty2 != DataType::Any {
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

    fn edges(&self) -> HashSet<(String, String)> {
        let mut edges = HashSet::new();
        let valid_names = self.nodes.keys().collect::<HashSet<_>>();
        for (name, node) in self.nodes.iter() {
            match node {
                Node::Transform { inputs, .. } | Node::Sink { inputs, .. } => {
                    for i in inputs {
                        if valid_names.contains(i) {
                            edges.insert((i.clone(), name.clone()));
                        }
                    }
                }
                Node::Source { .. } => {}
            }
        }
        edges
    }

    // Modified version of Kahn's topological sort algorithm that ignores the actual sorted output and
    // only cares if the sort was possible (i.e. whether or not there was a cycle in the input graph).
    fn contains_cycle(&self) -> bool {
        let nodes = self
            .nodes
            .keys()
            .map(|k| k.to_string())
            .collect::<HashSet<_>>();
        let mut edges = self.edges();

        let mut no_incoming = nodes
            .into_iter()
            .filter(|n| !edges.iter().any(|(_t, h)| h == n))
            .collect::<Vec<String>>();
        while let Some(node) = no_incoming.pop() {
            let outgoing = edges
                .iter()
                .filter(|(tail, _head)| *tail == node)
                .cloned()
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
}

impl From<&Config> for Graph {
    fn from(config: &Config) -> Self {
        let mut graph = Graph::default();

        // TODO: validate that node names are unique across sources/transforms/sinks?
        for (name, config) in config.sources.iter() {
            graph.add_source(name, config.output_type());
        }

        for (name, config) in config.transforms.iter() {
            graph.add_transform(
                name,
                config.inner.input_type(),
                config.inner.output_type(),
                config.inputs.clone(),
            );
        }

        for (name, config) in config.sinks.iter() {
            graph.add_sink(name, config.inner.input_type(), config.inputs.clone());
        }

        graph
    }
}

fn paths_rec(nodes: &HashMap<String, Node>, node: &str, mut path: Vec<String>) -> Vec<Vec<String>> {
    path.push(node.to_string());
    match nodes.get(node).clone() {
        Some(Node::Source { .. }) | None => {
            path.reverse();
            vec![path]
        }
        Some(Node::Transform { inputs, .. }) | Some(Node::Sink { inputs, .. }) => inputs
            .iter()
            .flat_map(|input| paths_rec(nodes, input, path.clone()))
            .collect(),
    }
}

#[cfg(test)]
mod test {
    use super::Graph;
    use crate::topology::config::DataType;
    use pretty_assertions::assert_eq;

    #[test]
    fn detects_cycles() {
        let mut graph = Graph::default();
        graph.add_source("in", DataType::Log);
        graph.add_transform("one", DataType::Log, DataType::Log, vec!["in", "three"]);
        graph.add_transform("two", DataType::Log, DataType::Log, vec!["one"]);
        graph.add_transform("three", DataType::Log, DataType::Log, vec!["two"]);
        graph.add_sink("out", DataType::Log, vec!["three"]);

        assert_eq!(true, graph.contains_cycle());
    }

    #[test]
    fn doesnt_detect_noncycles() {
        let mut graph = Graph::default();
        graph.add_source("in", DataType::Log);
        graph.add_transform("one", DataType::Log, DataType::Log, vec!["in"]);
        graph.add_transform("two", DataType::Log, DataType::Log, vec!["in"]);
        graph.add_transform("three", DataType::Log, DataType::Log, vec!["one", "two"]);
        graph.add_sink("out", DataType::Log, vec!["three"]);

        assert_eq!(false, graph.contains_cycle());
    }

    #[test]
    fn detects_type_mismatches() {
        let mut graph = Graph::default();
        graph.add_source("in", DataType::Log);
        graph.add_sink("out", DataType::Metric, vec!["in"]);

        assert_eq!(
            Err(vec![
                "Data type mismatch between in (Log) and out (Metric)".into()
            ]),
            graph.typecheck()
        );
    }

    #[test]
    fn allows_log_or_metric_into_any() {
        let mut graph = Graph::default();
        graph.add_source("log_source", DataType::Log);
        graph.add_source("metric_source", DataType::Metric);
        graph.add_sink(
            "any_sink",
            DataType::Any,
            vec!["log_source", "metric_source"],
        );

        assert_eq!(Ok(()), graph.typecheck());
    }

    #[test]
    fn doesnt_allow_any_into_log_or_metric() {
        let mut graph = Graph::default();
        graph.add_source("any_source", DataType::Any);
        graph.add_transform(
            "log_to_any",
            DataType::Log,
            DataType::Any,
            vec!["any_source"],
        );
        graph.add_transform(
            "any_to_log",
            DataType::Any,
            DataType::Log,
            vec!["any_source"],
        );
        graph.add_sink(
            "log_sink",
            DataType::Log,
            vec!["any_source", "log_to_any", "any_to_log"],
        );
        graph.add_sink(
            "metric_sink",
            DataType::Metric,
            vec!["any_source", "log_to_any"],
        );

        assert_eq!(
            Err(vec![
                "Data type mismatch between any_source (Any) and log_sink (Log)".into(),
                "Data type mismatch between any_source (Any) and log_to_any (Log)".into(),
                "Data type mismatch between any_source (Any) and metric_sink (Metric)".into(),
                "Data type mismatch between log_to_any (Any) and log_sink (Log)".into(),
                "Data type mismatch between log_to_any (Any) and metric_sink (Metric)".into(),
            ]),
            graph.typecheck()
        );
    }

    #[test]
    fn allows_both_directions_for_metrics() {
        let mut graph = Graph::default();
        graph.add_source("log_source", DataType::Log);
        graph.add_source("metric_source", DataType::Metric);
        graph.add_transform(
            "log_to_log",
            DataType::Log,
            DataType::Log,
            vec!["log_source"],
        );
        graph.add_transform(
            "metric_to_metric",
            DataType::Metric,
            DataType::Metric,
            vec!["metric_source"],
        );
        graph.add_transform(
            "any_to_any",
            DataType::Any,
            DataType::Any,
            vec!["log_to_log", "metric_to_metric"],
        );
        graph.add_transform(
            "any_to_log",
            DataType::Any,
            DataType::Log,
            vec!["any_to_any"],
        );
        graph.add_transform(
            "any_to_metric",
            DataType::Any,
            DataType::Metric,
            vec!["any_to_any"],
        );
        graph.add_sink("log_sink", DataType::Log, vec!["any_to_log"]);
        graph.add_sink("metric_sink", DataType::Metric, vec!["any_to_metric"]);

        assert_eq!(Ok(()), graph.typecheck());
    }
}

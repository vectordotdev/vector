use super::pipeline::{Pipeline, Pipelines};
use super::{builder::ConfigBuilder, DataType};
use std::collections::HashMap;

impl ConfigBuilder {
    pub fn typecheck(&self, pipelines: &Pipelines) -> Result<(), Vec<String>> {
        Graph::from(self).with_pipelines(pipelines).typecheck()
    }
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

impl Node {
    fn add_input(&mut self, input: String) {
        match self {
            Self::Transform { inputs, .. } => {
                inputs.push(input);
            }
            Self::Sink { inputs, .. } => {
                inputs.push(input);
            }
            _ => {}
        }
    }
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

    fn add_transform_input(&mut self, name: &str, input: String) {
        if let Some(node) = self.nodes.get_mut(name) {
            node.add_input(input);
        }
    }

    fn add_sink(&mut self, name: &str, ty: DataType, inputs: Vec<impl Into<String>>) {
        let inputs = self.clean_inputs(inputs);
        self.nodes
            .insert(name.to_string(), Node::Sink { ty, inputs });
    }

    fn paths(&self) -> Result<Vec<Vec<String>>, Vec<String>> {
        let mut errors = Vec::new();

        let nodes = self
            .nodes
            .iter()
            .filter_map(|(name, node)| match node {
                Node::Sink { .. } => Some(name),
                _ => None,
            })
            .flat_map(|node| {
                paths_rec(&self.nodes, node, Vec::new()).unwrap_or_else(|err| {
                    errors.push(err);
                    Vec::new()
                })
            })
            .collect();

        if !errors.is_empty() {
            errors.sort();
            errors.dedup();
            Err(errors)
        } else {
            Ok(nodes)
        }
    }

    fn clean_inputs(&self, inputs: Vec<impl Into<String>>) -> Vec<String> {
        inputs.into_iter().map(Into::into).collect()
    }

    fn typecheck(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        for path in self.paths()? {
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
                        if ty1 != ty2 && ty1 != DataType::Any && ty2 != DataType::Any {
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

    fn with_pipeline(mut self, pipeline_id: &str, pipeline: &Pipeline) -> Self {
        for (name, transform) in pipeline.transforms.iter() {
            let prefixed_name = format!("{}#{}", pipeline_id, name);
            self.add_transform(
                prefixed_name.as_str(),
                transform.inner.inner.input_type(),
                transform.inner.inner.output_type(),
                transform
                    .inner
                    .inputs
                    .iter()
                    .map(|input| {
                        // prefix with pipeline id when the name input is part of the pipeline
                        if pipeline.transforms.contains_key(input.as_str()) {
                            format!("{}#{}", pipeline_id, input)
                        } else {
                            input.clone()
                        }
                    })
                    .collect(),
            );
            // doing that in a single round because outputs are only
            // from the global config
            for output in transform.outputs.iter() {
                self.add_transform_input(prefixed_name.as_str(), output.clone());
            }
        }
        self
    }

    fn with_pipelines(self, pipelines: &Pipelines) -> Self {
        pipelines
            .0
            .iter()
            .fold(self, |res, (pipeline_id, pipeline)| {
                res.with_pipeline(pipeline_id.as_str(), pipeline)
            })
    }
}

impl From<&ConfigBuilder> for Graph {
    fn from(config: &ConfigBuilder) -> Self {
        let mut graph = Graph::default();

        // TODO: validate that node names are unique across sources/transforms/sinks?
        for (name, config) in config.sources.iter() {
            graph.add_source(name, config.inner.output_type());
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

fn paths_rec(
    nodes: &HashMap<String, Node>,
    node: &str,
    mut path: Vec<String>,
) -> Result<Vec<Vec<String>>, String> {
    if let Some(i) = path.iter().position(|p| p == node) {
        let mut segment = path.split_off(i);
        segment.push(node.into());
        // I think this is maybe easier to grok from source -> sink, but I'm not
        // married to either.
        segment.reverse();
        return Err(format!(
            "Cyclic dependency detected in the chain [ {} ]",
            segment.join(" -> ")
        ));
    }
    path.push(node.to_string());
    match nodes.get(node) {
        Some(Node::Source { .. }) | None => {
            path.reverse();
            Ok(vec![path])
        }
        Some(Node::Transform { inputs, .. }) | Some(Node::Sink { inputs, .. }) => {
            let mut paths = Vec::new();
            for input in inputs {
                match paths_rec(nodes, input, path.clone()) {
                    Ok(mut p) => paths.append(&mut p),
                    Err(err) => {
                        return Err(err);
                    }
                }
            }
            Ok(paths)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn paths_detects_cycles() {
        let mut graph = Graph::default();
        graph.add_source("in", DataType::Log);
        graph.add_transform("one", DataType::Log, DataType::Log, vec!["in", "three"]);
        graph.add_transform("two", DataType::Log, DataType::Log, vec!["one"]);
        graph.add_transform("three", DataType::Log, DataType::Log, vec!["two"]);
        graph.add_sink("out", DataType::Log, vec!["three"]);

        assert_eq!(
            Err(vec![
                "Cyclic dependency detected in the chain [ three -> one -> two -> three ]".into()
            ]),
            graph.paths()
        );

        let mut graph = Graph::default();
        graph.add_source("in", DataType::Log);
        graph.add_transform("one", DataType::Log, DataType::Log, vec!["in", "three"]);
        graph.add_transform("two", DataType::Log, DataType::Log, vec!["one"]);
        graph.add_transform("three", DataType::Log, DataType::Log, vec!["two"]);
        graph.add_sink("out", DataType::Log, vec!["two"]);

        assert_eq!(
            Err(vec![
                "Cyclic dependency detected in the chain [ two -> three -> one -> two ]".into()
            ]),
            graph.paths()
        );
        assert_eq!(
            Err(vec![
                "Cyclic dependency detected in the chain [ two -> three -> one -> two ]".into()
            ]),
            graph.typecheck()
        );

        let mut graph = Graph::default();
        graph.add_source("in", DataType::Log);
        graph.add_transform("in", DataType::Log, DataType::Log, vec!["in"]);
        graph.add_sink("out", DataType::Log, vec!["in"]);

        // This isn't really a cyclic dependency but let me have this one.
        assert_eq!(
            Err(vec![
                "Cyclic dependency detected in the chain [ in -> in ]".into()
            ]),
            graph.paths()
        );
    }

    #[test]
    fn paths_doesnt_detect_noncycles() {
        let mut graph = Graph::default();
        graph.add_source("in", DataType::Log);
        graph.add_transform("one", DataType::Log, DataType::Log, vec!["in"]);
        graph.add_transform("two", DataType::Log, DataType::Log, vec!["in"]);
        graph.add_transform("three", DataType::Log, DataType::Log, vec!["one", "two"]);
        graph.add_sink("out", DataType::Log, vec!["three"]);

        graph.paths().unwrap();
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
    fn allows_any_into_log_or_metric() {
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

        assert_eq!(graph.typecheck(), Ok(()));
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

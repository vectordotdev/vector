use super::{ComponentKey, DataType, OutputId, SinkOuter, SourceOuter, TransformOuter};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub enum Node {
    Source {
        ty: DataType,
    },
    Transform {
        in_ty: DataType,
        out_ty: DataType,
        named_outputs: Vec<String>,
    },
    Sink {
        ty: DataType,
    },
}

#[derive(Debug, Clone)]
struct Edge {
    from: OutputId,
    to: ComponentKey,
}

#[derive(Default)]
pub struct Graph {
    nodes: HashMap<ComponentKey, Node>,
    edges: Vec<Edge>,
}

impl Graph {
    pub fn new(
        sources: &IndexMap<ComponentKey, SourceOuter>,
        transforms: &IndexMap<ComponentKey, TransformOuter<String>>,
        sinks: &IndexMap<ComponentKey, SinkOuter<String>>,
    ) -> Result<Self, Vec<String>> {
        Self::new_inner(sources, transforms, sinks, false)
    }

    pub fn new_unchecked(
        sources: &IndexMap<ComponentKey, SourceOuter>,
        transforms: &IndexMap<ComponentKey, TransformOuter<String>>,
        sinks: &IndexMap<ComponentKey, SinkOuter<String>>,
    ) -> Self {
        Self::new_inner(sources, transforms, sinks, true).expect("errors ignored")
    }

    fn new_inner(
        sources: &IndexMap<ComponentKey, SourceOuter>,
        transforms: &IndexMap<ComponentKey, TransformOuter<String>>,
        sinks: &IndexMap<ComponentKey, SinkOuter<String>>,
        ignore_errors: bool,
    ) -> Result<Self, Vec<String>> {
        let mut graph = Graph::default();
        let mut errors = Vec::new();

        // First, insert all of the different node types
        for (id, config) in sources.iter() {
            graph.nodes.insert(
                id.clone(),
                Node::Source {
                    ty: config.inner.output_type(),
                },
            );
        }

        for (id, config) in transforms.iter() {
            graph.nodes.insert(
                id.clone(),
                Node::Transform {
                    in_ty: config.inner.input_type(),
                    out_ty: config.inner.output_type(),
                    named_outputs: config.inner.named_outputs(),
                },
            );
        }

        for (id, config) in sinks.iter() {
            graph.nodes.insert(
                id.clone(),
                Node::Sink {
                    ty: config.inner.input_type(),
                },
            );
        }

        // With all of the nodes added, go through inputs and add edges, resolving strings into
        // actual `OutputId`s along the way.
        let available_inputs = graph.input_map()?;

        for (id, config) in transforms.iter() {
            for input in config.inputs.iter() {
                if let Err(e) = graph.add_input(input, id, &available_inputs) {
                    errors.push(e);
                }
            }
        }

        for (id, config) in sinks.iter() {
            for input in config.inputs.iter() {
                if let Err(e) = graph.add_input(input, id, &available_inputs) {
                    errors.push(e);
                }
            }
        }

        if errors.is_empty() || ignore_errors {
            Ok(graph)
        } else {
            Err(errors)
        }
    }

    fn add_input(
        &mut self,
        from: &str,
        to: &ComponentKey,
        available_inputs: &HashMap<String, OutputId>,
    ) -> Result<(), String> {
        if let Some(output_id) = available_inputs.get(from) {
            self.edges.push(Edge {
                from: output_id.clone(),
                to: to.clone(),
            });
            Ok(())
        } else {
            let output_type = match self.nodes.get(to) {
                Some(Node::Transform { .. }) => "transform",
                Some(Node::Sink { .. }) => "sink",
                _ => panic!("only transforms and sinks have inputs"),
            };
            Err(format!(
                "Input \"{}\" for {} \"{}\" doesn't match any components.",
                from, output_type, to
            ))
        }
    }

    fn paths(&self) -> Result<Vec<Vec<ComponentKey>>, Vec<String>> {
        let mut errors = Vec::new();

        let nodes = self
            .nodes
            .iter()
            .filter_map(|(name, node)| match node {
                Node::Sink { .. } => Some(name),
                _ => None,
            })
            .flat_map(|node| {
                paths_rec(self, node, Vec::new()).unwrap_or_else(|err| {
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

    pub fn typecheck(&self) -> Result<(), Vec<String>> {
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

    pub fn valid_inputs(&self) -> HashSet<OutputId> {
        self.nodes
            .iter()
            .flat_map(|(key, node)| match node {
                Node::Sink { .. } => vec![],
                Node::Source { .. } => vec![key.clone().into()],
                Node::Transform { named_outputs, .. } => {
                    let mut outputs = vec![key.clone().into()];
                    outputs.extend(
                        named_outputs
                            .clone()
                            .into_iter()
                            .map(|n| OutputId::from((key, n))),
                    );
                    outputs
                }
            })
            .collect()
    }

    /// Produce a map of output IDs for the current set of nodes in the graph, keyed by their string
    /// representation. Returns errors for any nodes that have the same string representation,
    /// making input specifications ambiguous.
    ///
    /// When we get a dotted path in the `inputs` section of a user's config, we need to determine
    /// which of a few things that represents:
    ///
    ///   1. A component that's part of an expanded macro (e.g. `route.branch`)
    ///   2. A component within a pipeline (e.g. `pipeline.name`)
    ///   3. A named output of a branching transform (e.g. `name.errors`)
    ///
    /// A naive way to do that is to compare the string representation of all valid inputs to the
    /// provided string and pick the one that matches. This works better if you can assume that there
    /// are no conflicting string representations, so this function reports any ambiguity as an
    /// error when creating the lookup map.
    pub fn input_map(&self) -> Result<HashMap<String, OutputId>, Vec<String>> {
        let mut mapped: HashMap<String, OutputId> = HashMap::new();
        let mut errors = HashSet::new();

        for id in self.valid_inputs() {
            if let Some(_other) = mapped.insert(id.to_string(), id.clone()) {
                errors.insert(format!("Input specifier {} is ambiguous", id));
            }
        }

        if errors.is_empty() {
            Ok(mapped)
        } else {
            Err(errors.into_iter().collect())
        }
    }

    pub fn inputs_for(&self, node: &ComponentKey) -> Vec<OutputId> {
        self.edges
            .iter()
            .filter(|edge| &edge.to == node)
            .map(|edge| edge.from.clone())
            .collect()
    }
}

fn paths_rec(
    graph: &Graph,
    node: &ComponentKey,
    mut path: Vec<ComponentKey>,
) -> Result<Vec<Vec<ComponentKey>>, String> {
    if let Some(i) = path.iter().position(|p| p == node) {
        let mut segment = path.split_off(i);
        segment.push(node.into());
        // I think this is maybe easier to grok from source -> sink, but I'm not
        // married to either.
        segment.reverse();
        return Err(format!(
            "Cyclic dependency detected in the chain [ {} ]",
            segment
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>()
                .join(" -> ")
        ));
    }
    path.push(node.clone());
    match graph.nodes.get(node) {
        Some(Node::Source { .. }) | None => {
            path.reverse();
            Ok(vec![path])
        }
        Some(Node::Transform { .. }) | Some(Node::Sink { .. }) => {
            let inputs = graph
                .edges
                .iter()
                .filter(|e| &e.to == node)
                .map(|e| e.from.clone());
            let mut paths = Vec::new();
            for input in inputs {
                match paths_rec(graph, &input.component, path.clone()) {
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

    impl Graph {
        fn add_source(&mut self, id: &str, ty: DataType) {
            self.nodes.insert(id.into(), Node::Source { ty });
        }

        fn add_transform(
            &mut self,
            id: &str,
            in_ty: DataType,
            out_ty: DataType,
            inputs: Vec<&str>,
        ) {
            let id = ComponentKey::from(id);
            let inputs = clean_inputs(inputs);
            self.nodes.insert(
                id.clone(),
                Node::Transform {
                    in_ty,
                    out_ty,
                    named_outputs: Default::default(),
                },
            );
            for from in inputs {
                self.edges.push(Edge {
                    from,
                    to: id.clone(),
                });
            }
        }

        fn add_transform_output(&mut self, id: &str, name: &str) {
            let id = id.into();
            match self.nodes.get_mut(&id) {
                Some(Node::Transform { named_outputs, .. }) => named_outputs.push(name.into()),
                _ => panic!("invalid transform"),
            }
        }

        fn add_sink(&mut self, id: &str, ty: DataType, inputs: Vec<&str>) {
            let id = ComponentKey::from(id);
            let inputs = clean_inputs(inputs);
            self.nodes.insert(id.clone(), Node::Sink { ty });
            for from in inputs {
                self.edges.push(Edge {
                    from,
                    to: id.clone(),
                });
            }
        }

        fn test_add_input(&mut self, node: &str, input: &str) -> Result<(), String> {
            let available_inputs = self.input_map().unwrap();
            self.add_input(input, &node.into(), &available_inputs)
        }
    }

    fn clean_inputs(inputs: Vec<&str>) -> Vec<OutputId> {
        inputs.into_iter().map(Into::into).collect()
    }

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

    #[test]
    fn allows_multiple_transform_outputs() {
        let mut graph = Graph::default();
        graph.add_source("log_source", DataType::Log);
        graph.add_transform(
            "log_to_log",
            DataType::Log,
            DataType::Log,
            vec!["log_source"],
        );
        graph.add_transform_output("log_to_log", "errors");
        graph.add_sink("good_log_sink", DataType::Log, vec!["log_to_log"]);

        // don't add inputs to these yet since they're not validated via these helpers
        graph.add_sink("errored_log_sink", DataType::Log, vec![]);
        graph.add_sink("bad_log_sink", DataType::Log, vec![]);

        // make sure we're good with dotted paths
        assert_eq!(
            Ok(()),
            graph.test_add_input("errored_log_sink", "log_to_log.errors")
        );

        // make sure that we're not cool with an unknown dotted path
        let expected = "Input \"log_to_log.not_errors\" for sink \"bad_log_sink\" doesn't match any components.".to_string();
        assert_eq!(
            Err(expected),
            graph.test_add_input("bad_log_sink", "log_to_log.not_errors")
        );
    }

    #[test]
    fn disallows_ambiguous_inputs() {
        let mut graph = Graph::default();
        // these all look like "foo.bar", but should only yield one error
        graph.nodes.insert(
            ComponentKey::global("foo.bar"),
            Node::Source { ty: DataType::Any },
        );
        graph.nodes.insert(
            ComponentKey::pipeline("foo", "bar"),
            Node::Source { ty: DataType::Any },
        );
        graph.nodes.insert(
            ComponentKey::global("foo"),
            Node::Transform {
                in_ty: DataType::Any,
                out_ty: DataType::Any,
                named_outputs: vec![String::from("bar")],
            },
        );

        // make sure we return more than one
        graph.nodes.insert(
            ComponentKey::pipeline("baz", "errors"),
            Node::Source { ty: DataType::Any },
        );
        graph.nodes.insert(
            ComponentKey::global("baz"),
            Node::Transform {
                in_ty: DataType::Any,
                out_ty: DataType::Any,
                named_outputs: vec![String::from("errors")],
            },
        );

        let mut errors = graph.input_map().unwrap_err();
        errors.sort();
        assert_eq!(
            errors,
            vec![
                String::from("Input specifier baz.errors is ambiguous"),
                String::from("Input specifier foo.bar is ambiguous"),
            ]
        );
    }
}

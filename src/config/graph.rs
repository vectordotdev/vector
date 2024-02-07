use indexmap::{set::IndexSet, IndexMap};
use std::collections::{HashMap, HashSet, VecDeque};

use super::{
    schema, ComponentKey, DataType, OutputId, SinkOuter, SourceOuter, SourceOutput, TransformOuter,
    TransformOutput,
};

#[derive(Debug, Clone)]
pub enum Node {
    Source {
        outputs: Vec<SourceOutput>,
    },
    Transform {
        in_ty: DataType,
        outputs: Vec<TransformOutput>,
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
        schema: schema::Options,
    ) -> Result<Self, Vec<String>> {
        Self::new_inner(sources, transforms, sinks, false, schema)
    }

    pub fn new_unchecked(
        sources: &IndexMap<ComponentKey, SourceOuter>,
        transforms: &IndexMap<ComponentKey, TransformOuter<String>>,
        sinks: &IndexMap<ComponentKey, SinkOuter<String>>,
        schema: schema::Options,
    ) -> Self {
        Self::new_inner(sources, transforms, sinks, true, schema).expect("errors ignored")
    }

    fn new_inner(
        sources: &IndexMap<ComponentKey, SourceOuter>,
        transforms: &IndexMap<ComponentKey, TransformOuter<String>>,
        sinks: &IndexMap<ComponentKey, SinkOuter<String>>,
        ignore_errors: bool,
        schema: schema::Options,
    ) -> Result<Self, Vec<String>> {
        let mut graph = Graph::default();
        let mut errors = Vec::new();

        // First, insert all of the different node types
        for (id, config) in sources.iter() {
            graph.nodes.insert(
                id.clone(),
                Node::Source {
                    outputs: config.inner.outputs(schema.log_namespace()),
                },
            );
        }

        for (id, transform) in transforms.iter() {
            graph.nodes.insert(
                id.clone(),
                Node::Transform {
                    in_ty: transform.inner.input().data_type(),
                    outputs: transform.inner.outputs(
                        vector_lib::enrichment::TableRegistry::default(),
                        &[(id.into(), schema::Definition::any())],
                        schema.log_namespace(),
                    ),
                },
            );
        }

        for (id, config) in sinks.iter() {
            graph.nodes.insert(
                id.clone(),
                Node::Sink {
                    ty: config.inner.input().data_type(),
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

        if ignore_errors || errors.is_empty() {
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

    /// Return the input type of a given component.
    ///
    /// # Panics
    ///
    /// Will panic if the given key is not present in the graph or identifies a source, which can't
    /// have inputs.
    fn get_input_type(&self, key: &ComponentKey) -> DataType {
        match self.nodes[key] {
            Node::Source { .. } => panic!("no inputs on sources"),
            Node::Transform { in_ty, .. } => in_ty,
            Node::Sink { ty } => ty,
        }
    }

    /// Return the output type associated with a given `OutputId`.
    ///
    /// # Panics
    ///
    /// Will panic if the given id is not present in the graph or identifies a sink, which can't
    /// have inputs.
    fn get_output_type(&self, id: &OutputId) -> DataType {
        match &self.nodes[&id.component] {
            Node::Source { outputs } => outputs
                .iter()
                .find(|output| output.port == id.port)
                .map(|output| output.ty)
                .expect("output didn't exist"),
            Node::Transform { outputs, .. } => outputs
                .iter()
                .find(|output| output.port == id.port)
                .map(|output| output.ty)
                .expect("output didn't exist"),
            Node::Sink { .. } => panic!("no outputs on sinks"),
        }
    }

    pub fn typecheck(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // check that all edges connect components with compatible data types
        for edge in &self.edges {
            let from_ty = self.get_output_type(&edge.from);
            let to_ty = self.get_input_type(&edge.to);

            if !from_ty.intersects(to_ty) {
                errors.push(format!(
                    "Data type mismatch between {} ({}) and {} ({})",
                    edge.from, from_ty, edge.to, to_ty
                ));
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

    pub fn check_for_cycles(&self) -> Result<(), String> {
        // find all sinks
        let sinks = self.nodes.iter().filter_map(|(name, node)| match node {
            Node::Sink { .. } => Some(name),
            _ => None,
        });

        // run DFS from each sink while keep tracking the current stack to detect cycles
        for s in sinks {
            let mut traversal: VecDeque<ComponentKey> = VecDeque::new();
            let mut visited: HashSet<ComponentKey> = HashSet::new();
            let mut stack: IndexSet<ComponentKey> = IndexSet::new();

            traversal.push_back(s.to_owned());
            while !traversal.is_empty() {
                let n = traversal.back().expect("can't be empty").clone();
                if !visited.contains(&n) {
                    visited.insert(n.clone());
                    stack.insert(n.clone());
                } else {
                    // we came back to the node after exploring all its children - remove it from the stack and traversal
                    stack.shift_remove(&n);
                    traversal.pop_back();
                }
                let inputs = self
                    .edges
                    .iter()
                    .filter(|e| e.to == n)
                    .map(|e| e.from.clone());
                for input in inputs {
                    if !visited.contains(&input.component) {
                        traversal.push_back(input.component);
                    } else if stack.contains(&input.component) {
                        // we reached the node while it is on the current stack - it's a cycle
                        let path = stack
                            .iter()
                            .skip(1) // skip the sink
                            .rev()
                            .map(|item| item.to_string())
                            .collect::<Vec<_>>();
                        return Err(format!(
                            "Cyclic dependency detected in the chain [ {} -> {} ]",
                            input.component.id(),
                            path.join(" -> ")
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn valid_inputs(&self) -> HashSet<OutputId> {
        self.nodes
            .iter()
            .flat_map(|(key, node)| match node {
                Node::Sink { .. } => vec![],
                Node::Source { outputs } => outputs
                    .iter()
                    .map(|output| OutputId {
                        component: key.clone(),
                        port: output.port.clone(),
                    })
                    .collect(),
                Node::Transform { outputs, .. } => outputs
                    .iter()
                    .map(|output| OutputId {
                        component: key.clone(),
                        port: output.port.clone(),
                    })
                    .collect(),
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
    ///   2. A named output of a branching transform (e.g. `name.errors`)
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

    /// From a given root node, get all paths from the root node to leaf nodes
    /// where the leaf node must be a sink. This is useful for determining which
    /// components are relevant in a Vector unit test.
    ///
    /// Caller must check for cycles before calling this function.
    pub fn paths_to_sink_from(&self, root: &ComponentKey) -> Vec<Vec<ComponentKey>> {
        let mut traversal: VecDeque<(ComponentKey, Vec<_>)> = VecDeque::new();
        let mut paths = Vec::new();

        traversal.push_back((root.to_owned(), Vec::new()));
        while !traversal.is_empty() {
            let (n, mut path) = traversal.pop_back().expect("can't be empty");
            path.push(n.clone());
            let neighbors = self
                .edges
                .iter()
                .filter(|e| e.from.component == n)
                .map(|e| e.to.clone())
                .collect::<Vec<_>>();

            if neighbors.is_empty() {
                paths.push(path.clone());
            } else {
                for neighbor in neighbors {
                    traversal.push_back((neighbor, path.clone()));
                }
            }
        }

        // Keep only components from paths that end at a sink
        paths
            .into_iter()
            .filter(|path| {
                if let Some(key) = path.last() {
                    matches!(self.nodes.get(key), Some(Node::Sink { ty: _ }))
                } else {
                    false
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod test {
    use similar_asserts::assert_eq;
    use vector_lib::schema::Definition;

    use super::*;

    impl Graph {
        fn add_source(&mut self, id: &str, ty: DataType) {
            self.nodes.insert(
                id.into(),
                Node::Source {
                    outputs: vec![match ty {
                        DataType::Metric => SourceOutput::new_metrics(),
                        DataType::Trace => SourceOutput::new_traces(),
                        _ => SourceOutput::new_logs(ty, Definition::any()),
                    }],
                },
            );
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
                    outputs: vec![TransformOutput::new(
                        out_ty,
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    )],
                },
            );
            for from in inputs {
                self.edges.push(Edge {
                    from,
                    to: id.clone(),
                });
            }
        }

        fn add_transform_output(&mut self, id: &str, name: &str, ty: DataType) {
            let id = id.into();
            match self.nodes.get_mut(&id) {
                Some(Node::Transform { outputs, .. }) => outputs.push(
                    TransformOutput::new(
                        ty,
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    )
                    .with_port(name),
                ),
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
            Err("Cyclic dependency detected in the chain [ three -> one -> two -> three ]".into()),
            graph.check_for_cycles()
        );

        let mut graph = Graph::default();
        graph.add_source("in", DataType::Log);
        graph.add_transform("one", DataType::Log, DataType::Log, vec!["in", "three"]);
        graph.add_transform("two", DataType::Log, DataType::Log, vec!["one"]);
        graph.add_transform("three", DataType::Log, DataType::Log, vec!["two"]);
        graph.add_sink("out", DataType::Log, vec!["two"]);

        assert_eq!(
            Err("Cyclic dependency detected in the chain [ two -> three -> one -> two ]".into()),
            graph.check_for_cycles()
        );
        assert_eq!(
            Err("Cyclic dependency detected in the chain [ two -> three -> one -> two ]".into()),
            graph.check_for_cycles()
        );

        let mut graph = Graph::default();
        graph.add_source("in", DataType::Log);
        graph.add_transform("in", DataType::Log, DataType::Log, vec!["in"]);
        graph.add_sink("out", DataType::Log, vec!["in"]);

        // This isn't really a cyclic dependency but let me have this one.
        assert_eq!(
            Err("Cyclic dependency detected in the chain [ in -> in ]".into()),
            graph.check_for_cycles()
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

        graph.check_for_cycles().unwrap();
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
            DataType::all(),
            vec!["log_source", "metric_source"],
        );

        assert_eq!(Ok(()), graph.typecheck());
    }

    #[test]
    fn allows_any_into_log_or_metric() {
        let mut graph = Graph::default();
        graph.add_source("any_source", DataType::all());
        graph.add_transform(
            "log_to_any",
            DataType::Log,
            DataType::all(),
            vec!["any_source"],
        );
        graph.add_transform(
            "any_to_log",
            DataType::all(),
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
            DataType::all(),
            DataType::all(),
            vec!["log_to_log", "metric_to_metric"],
        );
        graph.add_transform(
            "any_to_log",
            DataType::all(),
            DataType::Log,
            vec!["any_to_any"],
        );
        graph.add_transform(
            "any_to_metric",
            DataType::all(),
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
        graph.add_transform_output("log_to_log", "errors", DataType::Log);
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
            ComponentKey::from("foo.bar"),
            Node::Source {
                outputs: vec![SourceOutput::new_logs(DataType::all(), Definition::any())],
            },
        );
        graph.nodes.insert(
            ComponentKey::from("foo.bar"),
            Node::Source {
                outputs: vec![SourceOutput::new_logs(DataType::all(), Definition::any())],
            },
        );
        graph.nodes.insert(
            ComponentKey::from("foo"),
            Node::Transform {
                in_ty: DataType::all(),
                outputs: vec![
                    TransformOutput::new(
                        DataType::all(),
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    ),
                    TransformOutput::new(
                        DataType::all(),
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    )
                    .with_port("bar"),
                ],
            },
        );

        // make sure we return more than one
        graph.nodes.insert(
            ComponentKey::from("baz.errors"),
            Node::Source {
                outputs: vec![SourceOutput::new_logs(DataType::all(), Definition::any())],
            },
        );
        graph.nodes.insert(
            ComponentKey::from("baz"),
            Node::Transform {
                in_ty: DataType::all(),
                outputs: vec![
                    TransformOutput::new(
                        DataType::all(),
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    ),
                    TransformOutput::new(
                        DataType::all(),
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    )
                    .with_port("errors"),
                ],
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

    #[test]
    fn paths_to_sink_simple() {
        let mut graph = Graph::default();
        graph.add_source("in", DataType::Log);
        graph.add_transform("one", DataType::Log, DataType::Log, vec!["in"]);
        graph.add_transform("two", DataType::Log, DataType::Log, vec!["one"]);
        graph.add_transform("three", DataType::Log, DataType::Log, vec!["two"]);
        graph.add_sink("out", DataType::Log, vec!["three"]);

        let paths: Vec<Vec<_>> = graph
            .paths_to_sink_from(&ComponentKey::from("in"))
            .into_iter()
            .map(|keys| keys.into_iter().map(|key| key.to_string()).collect())
            .collect();

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], vec!["in", "one", "two", "three", "out"])
    }

    #[test]
    fn paths_to_sink_non_existent_root() {
        let graph = Graph::default();
        let paths = graph.paths_to_sink_from(&ComponentKey::from("in"));

        assert_eq!(paths.len(), 0);
    }

    #[test]
    fn paths_to_sink_irrelevant_transforms() {
        let mut graph = Graph::default();
        graph.add_source("source", DataType::Log);
        // These transforms do not link to a sink
        graph.add_transform("t1", DataType::Log, DataType::Log, vec!["source"]);
        graph.add_transform("t2", DataType::Log, DataType::Log, vec!["t1"]);
        graph.add_transform("t3", DataType::Log, DataType::Log, vec!["t1"]);
        // These transforms do link to a sink
        graph.add_transform("t4", DataType::Log, DataType::Log, vec!["source"]);
        graph.add_transform("t5", DataType::Log, DataType::Log, vec!["source"]);
        graph.add_sink("sink1", DataType::Log, vec!["t4"]);
        graph.add_sink("sink2", DataType::Log, vec!["t5"]);

        let paths: Vec<Vec<_>> = graph
            .paths_to_sink_from(&ComponentKey::from("source"))
            .into_iter()
            .map(|keys| keys.into_iter().map(|key| key.to_string()).collect())
            .collect();

        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], vec!["source", "t5", "sink2"]);
        assert_eq!(paths[1], vec!["source", "t4", "sink1"]);
    }

    #[test]
    fn paths_to_sink_multiple_inputs_into_sink() {
        let mut graph = Graph::default();
        graph.add_source("source", DataType::Log);
        graph.add_transform("t1", DataType::Log, DataType::Log, vec!["source"]);
        graph.add_transform("t2", DataType::Log, DataType::Log, vec!["t1"]);
        graph.add_transform("t3", DataType::Log, DataType::Log, vec!["t1"]);
        graph.add_sink("sink1", DataType::Log, vec!["t2", "t3"]);

        let paths: Vec<Vec<_>> = graph
            .paths_to_sink_from(&ComponentKey::from("source"))
            .into_iter()
            .map(|keys| keys.into_iter().map(|key| key.to_string()).collect())
            .collect();

        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], vec!["source", "t1", "t3", "sink1"]);
        assert_eq!(paths[1], vec!["source", "t1", "t2", "sink1"]);
    }
}

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
};

use indexmap::set::IndexSet;

use super::{
    ComponentKey, ComponentKind, DataType, OutputId, SourceOutput, TransformOutput,
    WildcardMatching,
};

/// Port metadata derived from a component for graph validation.
///
/// Schemas remain on the component configuration; this graph only uses port
/// names and event data types to resolve and validate connections.
#[derive(Debug, Clone)]
pub(super) struct OutputSpec {
    pub port: Option<String>,
    pub data_type: DataType,
}

impl From<SourceOutput> for OutputSpec {
    fn from(output: SourceOutput) -> Self {
        Self {
            port: output.port,
            data_type: output.ty,
        }
    }
}

impl From<TransformOutput> for OutputSpec {
    fn from(output: TransformOutput) -> Self {
        Self {
            port: output.port,
            data_type: output.ty,
        }
    }
}

impl fmt::Display for OutputSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.port {
            Some(port) => write!(f, "port: \"{port}\","),
            None => write!(f, "port: None,"),
        }?;
        write!(f, " types: {}", self.data_type)
    }
}

/// The event input accepted by an execution node, before resolving connections.
#[derive(Debug, Clone)]
pub(super) struct InputSpec {
    pub data_type: DataType,
    pub upstream: Vec<String>,
}

/// Prepared execution-node metadata. A configured component may own multiple
/// nodes; ownership does not create event-flow edges between them.
#[derive(Debug, Clone)]
pub(super) struct NodeSpec {
    pub runtime_id: ComponentKey,
    pub component_id: ComponentKey,
    // Preserve the existing diagnostic labels and sink-rooted traversals.
    pub kind: ComponentKind,
    pub input: Option<InputSpec>,
    pub outputs: Vec<OutputSpec>,
}

impl fmt::Display for NodeSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ComponentKind::Source | ComponentKind::EnrichmentTable => {
                write!(f, "component_kind: {}\n  outputs:", self.kind)?;
            }
            ComponentKind::Transform => {
                // Preserve the existing diagnostic text during this refactor.
                write!(
                    f,
                    "component_kind: source\n  input_types: {}\n  outputs:",
                    self.input.as_ref().expect("transform has inputs").data_type
                )?;
            }
            ComponentKind::Sink => {
                return write!(
                    f,
                    "component_kind: sink\n  types: {}",
                    self.input.as_ref().expect("sink has inputs").data_type
                );
            }
        }
        for output in &self.outputs {
            write!(f, "\n    {output}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Edge {
    from: OutputId,
    to: ComponentKey,
}

#[derive(Default)]
pub struct Graph {
    nodes: HashMap<ComponentKey, NodeSpec>,
    edges: Vec<Edge>,
    input_errors: Vec<String>,
}

impl Graph {
    /// Builds a graph from prepared execution nodes in insertion order.
    ///
    /// Unresolved inputs are retained as diagnostics for `check_inputs`; ambiguous
    /// output names prevent construction entirely.
    pub fn new(
        nodes: impl IntoIterator<Item = NodeSpec>,
        wildcard_matching: WildcardMatching,
    ) -> Result<Self, Vec<String>> {
        let mut graph = Self::default();
        let mut inputs = Vec::new();
        for mut node in nodes {
            if let Some(input) = &mut node.input {
                inputs.push((node.runtime_id.clone(), std::mem::take(&mut input.upstream)));
            }
            graph.nodes.insert(node.runtime_id.clone(), node);
        }

        let available_outputs = graph.output_map()?;
        // Retain every input in insertion order, even when a later node replaces
        // an earlier node with the same runtime ID.
        for (id, upstream) in inputs {
            for input in upstream {
                if let Err(e) = graph.add_input(&input, &id, &available_outputs, wildcard_matching)
                {
                    graph.input_errors.push(e);
                }
            }
        }

        Ok(graph)
    }

    /// Reports unresolved inputs after construction. Partial graphs used while
    /// preparing config unit tests can be inspected before this check.
    pub fn check_inputs(&self) -> Result<(), Vec<String>> {
        if self.input_errors.is_empty() {
            Ok(())
        } else {
            Err(self.input_errors.clone())
        }
    }

    fn add_input(
        &mut self,
        from: &str,
        to: &ComponentKey,
        available_outputs: &HashMap<String, OutputId>,
        wildcard_matching: WildcardMatching,
    ) -> Result<(), String> {
        if let Some(output_id) = available_outputs.get(from) {
            self.edges.push(Edge {
                from: output_id.clone(),
                to: to.clone(),
            });
            Ok(())
        } else {
            let component_kind = self.nodes[to].kind;
            // allow empty result if relaxed wildcard matching is enabled
            match wildcard_matching {
                WildcardMatching::Relaxed => {
                    // using value != glob::Pattern::escape(value) to check if value is a glob
                    // TODO: replace with proper check when https://github.com/rust-lang/glob/issues/72 is resolved
                    if from != glob::Pattern::escape(from) {
                        info!(
                            "Input \"{from}\" for {component_kind} \"{to}\" didn’t match any components, but this was ignored because `relaxed_wildcard_matching` is enabled."
                        );
                        return Ok(());
                    }
                }
                WildcardMatching::Strict => {}
            }
            info!(
                "Available components:\n{}",
                self.nodes
                    .iter()
                    .map(|(key, node)| format!("\"{key}\":\n  {node}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            Err(format!(
                "Input \"{from}\" for {component_kind} \"{to}\" doesn't match any components.",
            ))
        }
    }

    /// Return the input type of a given component.
    ///
    /// # Panics
    ///
    /// Will panic if the given key is not present in the graph or identifies a node without
    /// an event input.
    fn get_input_type(&self, key: &ComponentKey) -> DataType {
        self.nodes[key]
            .input
            .as_ref()
            .expect("node does not accept inputs")
            .data_type
    }

    /// Return the output type associated with a given `OutputId`.
    ///
    /// # Panics
    ///
    /// Will panic if the given id is not present in the graph or does not identify a declared
    /// output port.
    fn get_output_type(&self, id: &OutputId) -> DataType {
        let node = &self.nodes[&id.component];
        node.outputs
            .iter()
            .find(|output| output.port == id.port)
            .map(|output| output.data_type)
            .expect("output didn't exist")
    }

    pub fn typecheck(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // check that all edges connect components with compatible data types
        for edge in &self.edges {
            let from_ty = self.get_output_type(&edge.from);
            let to_ty = self.get_input_type(&edge.to);

            if !from_ty.intersects(to_ty) {
                errors.push(format!(
                    "Data type mismatch between {} ({from_ty}) and {} ({to_ty})",
                    edge.from, edge.to
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
        let sinks = self
            .nodes
            .iter()
            .filter_map(|(name, node)| (node.kind == ComponentKind::Sink).then_some(name));

        // run DFS from each sink while keep tracking the current stack to detect cycles
        for s in sinks {
            let mut traversal: VecDeque<ComponentKey> = VecDeque::new();
            let mut visited: HashSet<ComponentKey> = HashSet::new();
            let mut stack: IndexSet<ComponentKey> = IndexSet::new();

            traversal.push_back(s.to_owned());
            while !traversal.is_empty() {
                let n = traversal.back().expect("can't be empty").clone();
                if visited.contains(&n) {
                    // we came back to the node after exploring all its children - remove it from the stack and traversal
                    stack.shift_remove(&n);
                    traversal.pop_back();
                } else {
                    visited.insert(n.clone());
                    stack.insert(n.clone());
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
                            .map(std::string::ToString::to_string)
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

    /// Maps output names used in `inputs` to output IDs, rejecting ambiguous names.
    ///
    /// A dotted name such as `route.branch` can identify either an expanded
    /// component or a named output. Distinct output IDs must therefore have
    /// distinct string representations.
    pub fn output_map(&self) -> Result<HashMap<String, OutputId>, Vec<String>> {
        let outputs = self
            .nodes
            .iter()
            .flat_map(|(key, node)| {
                node.outputs.iter().map(|output| OutputId {
                    component: key.clone(),
                    port: output.port.clone(),
                })
            })
            .collect::<HashSet<_>>();
        let mut mapped = HashMap::new();
        let mut errors = HashSet::new();

        for id in outputs {
            if let Some(_other) = mapped.insert(id.to_string(), id.clone()) {
                errors.insert(format!("Input specifier {id} is ambiguous"));
            }
        }

        if errors.is_empty() {
            Ok(mapped)
        } else {
            Err(errors.into_iter().collect())
        }
    }

    /// Resolves inputs belonging to a configured component, including its table tasks.
    pub fn inputs_for_component(&self, component: &ComponentKey) -> Vec<OutputId> {
        self.edges
            .iter()
            .filter(|edge| &self.nodes[&edge.to].component_id == component)
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
                    self.nodes
                        .get(key)
                        .is_some_and(|node| node.kind == ComponentKind::Sink)
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
    use crate::{
        config::{ConfigBuilder, graph_builder},
        test_util::mock::transforms::BasicTransformConfig,
    };

    #[test]
    fn partial_graph_retains_connections_and_reports_unresolved_inputs() {
        let mut config = ConfigBuilder::default();
        config.add_transform(
            "first",
            &["missing_source"],
            BasicTransformConfig::default(),
        );
        config.add_transform(
            "second",
            &["first", "missing_transform"],
            BasicTransformConfig::default(),
        );
        let graph = Graph::new(graph_builder::nodes(&config), WildcardMatching::Strict).unwrap();
        let outputs = graph.output_map().unwrap();
        assert_eq!(
            outputs,
            HashMap::from([
                ("first".into(), OutputId::from("first")),
                ("second".into(), OutputId::from("second")),
            ])
        );

        assert_eq!(
            graph.inputs_for_component(&"second".into()),
            vec![OutputId::from("first")]
        );
        assert_eq!(
            graph.check_inputs().unwrap_err(),
            vec![
                "Input \"missing_source\" for transform \"first\" doesn't match any components.",
                "Input \"missing_transform\" for transform \"second\" doesn't match any components.",
            ]
        );
    }

    #[test]
    fn preserves_node_diagnostics() {
        let source = NodeSpec::source(
            "source",
            vec![SourceOutput::new_metrics().with_port("metrics")],
        );
        assert_eq!(
            source.to_string(),
            "component_kind: source\n  outputs:\n    port: \"metrics\", types: [\"Metric\"]"
        );

        let transform = NodeSpec::transform(
            "transform",
            DataType::Log,
            vec![TransformOutput::new(DataType::Trace, HashMap::new())],
        );
        assert_eq!(
            transform.to_string(),
            "component_kind: source\n  input_types: [\"Log\"]\n  outputs:\n    port: None, types: [\"Trace\"]"
        );

        let sink = NodeSpec::sink("sink", DataType::Metric);
        assert_eq!(
            sink.to_string(),
            "component_kind: sink\n  types: [\"Metric\"]"
        );
    }

    #[test]
    fn replacing_a_runtime_node_retains_all_input_diagnostics() {
        let mut first = NodeSpec::transform("shared", DataType::Log, Vec::new());
        first.input.as_mut().unwrap().upstream = vec!["missing_first".into()];
        let mut second = NodeSpec::sink("shared", DataType::Log);
        second.input.as_mut().unwrap().upstream = vec!["missing_second".into()];

        let graph = Graph::new([first, second], WildcardMatching::Strict).unwrap();
        assert_eq!(
            graph.check_inputs().unwrap_err(),
            vec![
                "Input \"missing_first\" for sink \"shared\" doesn't match any components.",
                "Input \"missing_second\" for sink \"shared\" doesn't match any components.",
            ]
        );
    }

    impl NodeSpec {
        fn source(id: &str, outputs: Vec<SourceOutput>) -> Self {
            Self {
                runtime_id: id.into(),
                component_id: id.into(),
                kind: ComponentKind::Source,
                input: None,
                outputs: outputs.into_iter().map(OutputSpec::from).collect(),
            }
        }

        fn transform(id: &str, input_type: DataType, outputs: Vec<TransformOutput>) -> Self {
            Self {
                runtime_id: id.into(),
                component_id: id.into(),
                kind: ComponentKind::Transform,
                input: Some(InputSpec {
                    data_type: input_type,
                    upstream: Vec::new(),
                }),
                outputs: outputs.into_iter().map(OutputSpec::from).collect(),
            }
        }

        fn sink(id: &str, input_type: DataType) -> Self {
            Self {
                runtime_id: id.into(),
                component_id: id.into(),
                kind: ComponentKind::Sink,
                input: Some(InputSpec {
                    data_type: input_type,
                    upstream: Vec::new(),
                }),
                outputs: Vec::new(),
            }
        }
    }

    impl Graph {
        fn add_source(&mut self, id: &str, ty: DataType) {
            self.nodes.insert(
                id.into(),
                NodeSpec::source(
                    id,
                    vec![match ty {
                        DataType::Metric => SourceOutput::new_metrics(),
                        DataType::Trace => SourceOutput::new_traces(),
                        _ => SourceOutput::new_maybe_logs(ty, Definition::any()),
                    }],
                ),
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
                NodeSpec::transform(
                    id.id(),
                    in_ty,
                    vec![TransformOutput::new(
                        out_ty,
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    )],
                ),
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
                Some(node) if node.kind == ComponentKind::Transform => node.outputs.push(
                    TransformOutput::new(
                        ty,
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    )
                    .with_port(name)
                    .into(),
                ),
                _ => panic!("invalid transform"),
            }
        }

        fn add_sink(&mut self, id: &str, ty: DataType, inputs: Vec<&str>) {
            let id = ComponentKey::from(id);
            let inputs = clean_inputs(inputs);
            self.nodes.insert(id.clone(), NodeSpec::sink(id.id(), ty));
            for from in inputs {
                self.edges.push(Edge {
                    from,
                    to: id.clone(),
                });
            }
        }

        fn test_add_input(
            &mut self,
            node: &str,
            input: &str,
            wildcard_matching: WildcardMatching,
        ) -> Result<(), String> {
            let available_outputs = self.output_map().unwrap();
            self.add_input(input, &node.into(), &available_outputs, wildcard_matching)
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
                "Data type mismatch between in ([\"Log\"]) and out ([\"Metric\"])".into()
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
            DataType::all_bits(),
            vec!["log_source", "metric_source"],
        );

        assert_eq!(Ok(()), graph.typecheck());
    }

    #[test]
    fn allows_any_into_log_or_metric() {
        let mut graph = Graph::default();
        graph.add_source("any_source", DataType::all_bits());
        graph.add_transform(
            "log_to_any",
            DataType::Log,
            DataType::all_bits(),
            vec!["any_source"],
        );
        graph.add_transform(
            "any_to_log",
            DataType::all_bits(),
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
            DataType::all_bits(),
            DataType::all_bits(),
            vec!["log_to_log", "metric_to_metric"],
        );
        graph.add_transform(
            "any_to_log",
            DataType::all_bits(),
            DataType::Log,
            vec!["any_to_any"],
        );
        graph.add_transform(
            "any_to_metric",
            DataType::all_bits(),
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
            graph.test_add_input(
                "errored_log_sink",
                "log_to_log.errors",
                WildcardMatching::Strict
            )
        );

        // make sure that we're not cool with an unknown dotted path
        let expected = "Input \"log_to_log.not_errors\" for sink \"bad_log_sink\" doesn't match any components.".to_string();
        assert_eq!(
            Err(expected),
            graph.test_add_input(
                "bad_log_sink",
                "log_to_log.not_errors",
                WildcardMatching::Strict
            )
        );
    }

    #[test]
    fn disallows_ambiguous_inputs() {
        let mut graph = Graph::default();
        // these all look like "foo.bar", but should only yield one error
        graph.nodes.insert(
            ComponentKey::from("foo.bar"),
            NodeSpec::source(
                "foo.bar",
                vec![SourceOutput::new_maybe_logs(
                    DataType::all_bits(),
                    Definition::any(),
                )],
            ),
        );
        graph.nodes.insert(
            ComponentKey::from("foo.bar"),
            NodeSpec::source(
                "foo.bar",
                vec![SourceOutput::new_maybe_logs(
                    DataType::all_bits(),
                    Definition::any(),
                )],
            ),
        );
        graph.nodes.insert(
            ComponentKey::from("foo"),
            NodeSpec::transform(
                "foo",
                DataType::all_bits(),
                vec![
                    TransformOutput::new(
                        DataType::all_bits(),
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    ),
                    TransformOutput::new(
                        DataType::all_bits(),
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    )
                    .with_port("bar"),
                ],
            ),
        );

        // make sure we return more than one
        graph.nodes.insert(
            ComponentKey::from("baz.errors"),
            NodeSpec::source(
                "baz.errors",
                vec![SourceOutput::new_maybe_logs(
                    DataType::all_bits(),
                    Definition::any(),
                )],
            ),
        );
        graph.nodes.insert(
            ComponentKey::from("baz"),
            NodeSpec::transform(
                "baz",
                DataType::all_bits(),
                vec![
                    TransformOutput::new(
                        DataType::all_bits(),
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    ),
                    TransformOutput::new(
                        DataType::all_bits(),
                        [("test".into(), Definition::default_legacy_namespace())].into(),
                    )
                    .with_port("errors"),
                ],
            ),
        );

        let mut errors = graph.output_map().unwrap_err();
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
    fn wildcard_matching() {
        let mut graph = Graph::default();
        graph.add_source("log_source", DataType::Log);

        // don't add inputs to these yet since they're not validated via these helpers
        graph.add_sink("sink", DataType::Log, vec![]);

        // make sure we're not good with non existing inputs with relaxed wildcard matching disabled
        let wildcard_matching = WildcardMatching::Strict;
        let expected =
            "Input \"bad_source-*\" for sink \"sink\" doesn't match any components.".to_string();
        assert_eq!(
            Err(expected),
            graph.test_add_input("sink", "bad_source-*", wildcard_matching)
        );

        // make sure we're good with non existing inputs with relaxed wildcard matching enabled
        let wildcard_matching = WildcardMatching::Relaxed;
        assert_eq!(
            Ok(()),
            graph.test_add_input("sink", "bad_source-*", wildcard_matching)
        );

        // make sure we're not good with non existing inputs that are not wildcards even when relaxed wildcard matching is enabled
        let wildcard_matching = WildcardMatching::Relaxed;
        let expected =
            "Input \"bad_source-1\" for sink \"sink\" doesn't match any components.".to_string();
        assert_eq!(
            Err(expected),
            graph.test_add_input("sink", "bad_source-1", wildcard_matching)
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
        assert_eq!(paths[0], vec!["in", "one", "two", "three", "out"]);
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

use indexmap::IndexMap;

use super::{
    ComponentKey, ComponentKind, ConfigBuilder, SinkOuter, SourceOuter, TransformContext,
    TransformOuter,
    graph::{InputSpec, NodeSpec},
    schema,
};

/// Describes the explicitly configured event-processing nodes. Intermediate config
/// unit-test graphs use these before the final compilation expands enrichment tables.
pub(super) fn nodes(config: &ConfigBuilder) -> impl Iterator<Item = NodeSpec> + '_ {
    config
        .sources
        .iter()
        .map(|(id, source)| source_node(id, id, source, config.schema))
        .chain(
            config
                .transforms
                .iter()
                .map(|(id, transform)| transform_node(id, transform, config.schema)),
        )
        .chain(
            config
                .sinks
                .iter()
                .map(|(id, sink)| sink_node(id, id, sink)),
        )
}

/// Expands table tasks while retaining their configured owner. Keep the existing
/// source/transform/sink order and replacement of colliding source and sink IDs.
pub(super) fn expanded_nodes(config: &ConfigBuilder) -> Vec<NodeSpec> {
    let sinks = config
        .sinks
        .iter()
        .map(|(id, sink)| (id.clone(), (id, sink.clone())))
        .chain(
            config
                .enrichment_tables
                .iter()
                .filter_map(|(id, table)| table.as_sink(id).map(|(key, sink)| (key, (id, sink)))),
        )
        .collect::<IndexMap<_, _>>();
    let sources =
        config
            .sources
            .iter()
            .map(|(id, source)| (id.clone(), (id, source.clone())))
            .chain(config.enrichment_tables.iter().filter_map(|(id, table)| {
                table.as_source(id).map(|(key, source)| (key, (id, source)))
            }))
            .collect::<IndexMap<_, _>>();

    sources
        .iter()
        .map(|(id, (owner, source))| source_node(id, owner, source, config.schema))
        .chain(
            config
                .transforms
                .iter()
                .map(|(id, transform)| transform_node(id, transform, config.schema)),
        )
        .chain(
            sinks
                .iter()
                .map(|(id, (owner, sink))| sink_node(id, owner, sink)),
        )
        .collect()
}

fn source_node(
    runtime_id: &ComponentKey,
    component_id: &ComponentKey,
    source: &SourceOuter,
    schema: schema::Options,
) -> NodeSpec {
    NodeSpec {
        runtime_id: runtime_id.clone(),
        component_id: component_id.clone(),
        kind: ComponentKind::Source,
        input: None,
        outputs: source
            .inner
            .outputs(schema.log_namespace())
            .into_iter()
            .map(Into::into)
            .collect(),
    }
}

fn transform_node(
    id: &ComponentKey,
    transform: &TransformOuter<String>,
    schema: schema::Options,
) -> NodeSpec {
    NodeSpec {
        runtime_id: id.clone(),
        component_id: id.clone(),
        kind: ComponentKind::Transform,
        input: Some(InputSpec {
            data_type: transform.inner.input().data_type(),
            upstream: transform.inputs.iter().cloned().collect(),
        }),
        outputs: transform
            .inner
            .outputs(
                &TransformContext {
                    schema,
                    ..Default::default()
                },
                &[(id.into(), schema::Definition::any())],
            )
            .into_iter()
            .map(Into::into)
            .collect(),
    }
}

fn sink_node(
    runtime_id: &ComponentKey,
    component_id: &ComponentKey,
    sink: &SinkOuter<String>,
) -> NodeSpec {
    NodeSpec {
        runtime_id: runtime_id.clone(),
        component_id: component_id.clone(),
        kind: ComponentKind::Sink,
        input: Some(InputSpec {
            data_type: sink.inner.input().data_type(),
            upstream: sink.inputs.iter().cloned().collect(),
        }),
        outputs: Vec::new(),
    }
}

#[cfg(all(test, feature = "enrichment-tables-memory"))]
mod tests {
    use super::*;
    use crate::config::{OutputId, WildcardMatching, graph::Graph};

    fn memory_feedback_config() -> ConfigBuilder {
        serde_yaml::from_str(
            r#"
            sources:
              seed:
                type: test_basic
            transforms:
              update:
                type: test_basic
                inputs: [table_events]
                suffix: ""
                increase: 0.0
            enrichment_tables:
              cache:
                type: memory
                inputs: [seed, update]
                source_config:
                  source_key: table_events
                  export_interval: 1
            sinks:
              observe:
                type: test_basic
                inputs: [table_events]
            "#,
        )
        .unwrap()
    }

    #[test]
    fn memory_table_feedback_preserves_ownership_and_connections() {
        let config = memory_feedback_config();
        let expanded = expanded_nodes(&config);
        let identities = expanded
            .iter()
            .map(|node| (node.runtime_id.id(), node.component_id.id(), node.kind))
            .collect::<Vec<_>>();
        assert_eq!(
            identities,
            vec![
                ("seed", "seed", ComponentKind::Source),
                ("table_events", "cache", ComponentKind::Source),
                ("update", "update", ComponentKind::Transform),
                ("observe", "observe", ComponentKind::Sink),
                ("cache", "cache", ComponentKind::Sink),
            ]
        );

        let graph = Graph::new(expanded, WildcardMatching::Strict).unwrap();
        graph.check_inputs().unwrap();
        graph.typecheck().unwrap();
        graph.check_for_cycles().unwrap();
        assert_eq!(
            graph.inputs_for_component(&"cache".into()),
            vec![OutputId::from("seed"), OutputId::from("update")]
        );

        // Check the compiler's resolved configuration, not just the graph descriptor.
        let compiled = config.build().unwrap();
        assert_eq!(
            compiled.enrichment_tables[&ComponentKey::from("cache")]
                .inputs
                .as_ref(),
            &[OutputId::from("seed"), OutputId::from("update")]
        );
        assert_eq!(
            compiled.transforms[&ComponentKey::from("update")]
                .inputs
                .as_ref(),
            &[OutputId::from("table_events")]
        );
    }

    #[test]
    fn intermediate_graph_does_not_expand_table_tasks() {
        let config = memory_feedback_config();
        let graph = Graph::new(nodes(&config), WildcardMatching::Strict).unwrap();
        let outputs = graph.output_map().unwrap();
        assert!(outputs.contains_key("seed"));
        assert!(outputs.contains_key("update"));
        assert!(!outputs.contains_key("table_events"));
        assert_eq!(
            graph.check_inputs().unwrap_err(),
            vec![
                "Input \"table_events\" for transform \"update\" doesn't match any components.",
                "Input \"table_events\" for sink \"observe\" doesn't match any components.",
            ]
        );
    }

    #[test]
    fn table_feedback_does_not_allow_a_direct_transform_cycle() {
        let mut config = memory_feedback_config();
        config
            .transforms
            .get_mut(&ComponentKey::from("update"))
            .unwrap()
            .inputs = vec!["table_events".to_owned(), "update".to_owned()].into();

        assert_eq!(
            config.build().unwrap_err(),
            vec!["Cyclic dependency detected in the chain [ update -> update ]"]
        );
    }

    #[test]
    fn lookup_only_table_has_no_execution_nodes() {
        let mut config = ConfigBuilder::default();
        config.add_enrichment_table(
            "lookup",
            &[],
            crate::enrichment_tables::EnrichmentTables::File(Default::default()),
        );
        let graph = Graph::new(expanded_nodes(&config), WildcardMatching::Strict).unwrap();
        graph.check_inputs().unwrap();
        graph.check_for_cycles().unwrap();
        assert!(graph.output_map().unwrap().is_empty());
        assert!(graph.inputs_for_component(&"lookup".into()).is_empty());
    }

    #[test]
    fn table_tasks_preserve_runtime_id_replacement() {
        let mut config = memory_feedback_config();
        config.sources.insert(
            "table_events".into(),
            config.sources[&ComponentKey::from("seed")].clone(),
        );
        let mut replaced_sink = config.sinks[&ComponentKey::from("observe")].clone();
        replaced_sink.inputs = vec!["missing".to_owned()].into();
        config.sinks.insert("cache".into(), replaced_sink);
        config.enrichment_tables.insert(
            "second_cache".into(),
            config.enrichment_tables[&ComponentKey::from("cache")].clone(),
        );

        let expanded = expanded_nodes(&config);
        let owners = expanded
            .iter()
            .map(|node| (node.runtime_id.id(), node.component_id.id()))
            .collect::<Vec<_>>();
        assert_eq!(
            owners,
            vec![
                ("seed", "seed"),
                ("table_events", "second_cache"),
                ("update", "update"),
                ("observe", "observe"),
                ("cache", "cache"),
                ("second_cache", "second_cache"),
            ]
        );
        let graph = Graph::new(expanded, WildcardMatching::Strict).unwrap();
        graph.check_inputs().unwrap();
        graph.check_for_cycles().unwrap();
        for component in ["cache", "second_cache"] {
            assert_eq!(
                graph.inputs_for_component(&component.into()),
                vec![OutputId::from("seed"), OutputId::from("update")]
            );
        }
        config.build().unwrap();
    }
}

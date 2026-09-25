use serde::Serialize;
use strum::EnumDiscriminants;
use vector_lib::configurable::{Configurable, ToValue};

use super::{EnrichmentTableOuter, OutputId, SinkOuter, SourceOuter, TransformOuter};

/// A borrowed view of a configured pipeline component.
///
/// The configuration maps retain ownership and their existing serialization. Both
/// unresolved (`String`) and resolved (`OutputId`) inputs use this same model.
/// `ComponentKind` is generated from these variants so consumers do not maintain
/// a separate list of component categories.
#[derive(Debug, EnumDiscriminants)]
#[strum_discriminants(name(ComponentKind))]
#[strum_discriminants(derive(Ord, PartialOrd, strum::Display))]
#[strum_discriminants(strum(serialize_all = "snake_case"))]
pub enum Component<'a, T = OutputId>
where
    T: Configurable + Serialize + ToValue + Clone + 'static,
{
    Source(&'a SourceOuter),
    Transform(&'a TransformOuter<T>),
    Sink(&'a SinkOuter<T>),
    EnrichmentTable(&'a EnrichmentTableOuter<T>),
}

impl<'a, T> Component<'a, T>
where
    T: Configurable + Serialize + ToValue + Clone + 'static,
{
    pub fn kind(&self) -> ComponentKind {
        self.into()
    }

    /// Configured inputs, before or after output resolution.
    ///
    /// Enrichment table inputs belong to the table's derived sink. A derived
    /// source is a separate component, whose key may differ from the table key.
    pub fn inputs(&self) -> Option<&'a [T]> {
        match self {
            Self::Source(_) => None,
            Self::Transform(config) => Some(&config.inputs),
            Self::Sink(config) => Some(&config.inputs),
            Self::EnrichmentTable(config) => Some(&config.inputs),
        }
    }
}

#[cfg(all(test, feature = "enrichment-tables-memory"))]
mod tests {
    use super::*;
    use crate::config::{ComponentKey, ConfigBuilder};

    #[test]
    fn configured_components_retain_table_identity_and_resolved_inputs() {
        let config = serde_yaml::from_str::<ConfigBuilder>(
            r#"
            sources:
              input:
                type: test_basic
            transforms:
              transform:
                type: test_basic
                inputs: [input]
                suffix: ""
                increase: 0.0
            enrichment_tables:
              table:
                type: memory
                inputs: [transform]
                source_config:
                  source_key: table_source
                  export_interval: 50
            sinks:
              output:
                type: test_basic
                inputs: [table_source]
            "#,
        )
        .unwrap()
        .build()
        .unwrap();

        let components = config
            .components()
            .map(|(key, component)| {
                (
                    key.id(),
                    component.kind(),
                    component.inputs().map(<[_]>::to_vec),
                )
            })
            .collect::<Vec<_>>();

        // A table remains one configured component even when graph construction
        // expands it into a sink and a source with a different key.
        assert_eq!(
            components,
            vec![
                ("input", ComponentKind::Source, None),
                (
                    "transform",
                    ComponentKind::Transform,
                    Some(vec!["input".into()])
                ),
                (
                    "output",
                    ComponentKind::Sink,
                    Some(vec!["table_source".into()])
                ),
                (
                    "table",
                    ComponentKind::EnrichmentTable,
                    Some(vec!["transform".into()])
                ),
            ]
        );
        assert_eq!(config.inputs_for_node(&ComponentKey::from("input")), None);
        assert_eq!(
            config.inputs_for_node(&ComponentKey::from("table_source")),
            None
        );
        assert_eq!(
            config.inputs_for_node(&ComponentKey::from("table")),
            Some([OutputId::from("transform")].as_slice())
        );
    }
}

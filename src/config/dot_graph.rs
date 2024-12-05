use std::collections::HashMap;

use vector_lib::configurable::configurable_component;

/// Extra graph configuration
///
/// Configure output for component when generated with graph command
#[configurable_component]
#[configurable(metadata(docs::advanced))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GraphConfig {
    /// Node attributes to add to this component's node in resulting graph
    ///
    /// They are added to the node as provided
    #[configurable(metadata(
        docs::additional_props_description = "A single graph node attribute in graphviz DOT language.",
        docs::examples = "example_graph_options()"
    ))]
    pub node_attributes: HashMap<String, String>,
}

fn example_graph_options() -> HashMap<String, String> {
    HashMap::<_, _>::from_iter([
        ("name".to_string(), "Example Node".to_string()),
        ("color".to_string(), "red".to_string()),
        ("width".to_string(), "5.0".to_string()),
    ])
}

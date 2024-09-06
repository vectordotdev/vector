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
        docs::additional_props_description = "A single graph node attribute in graphviz DOT language."
    ))]
    pub node_attributes: HashMap<String, String>,
}

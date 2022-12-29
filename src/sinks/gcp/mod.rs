use std::collections::HashMap;

use vector_config::configurable_component;

pub mod chronicle_unstructured;
pub mod cloud_storage;
pub mod pubsub;
pub mod stackdriver_logs;
pub mod stackdriver_metrics;

/// A monitored resource.
///
/// Monitored resources in GCP allow associating logs and metrics specifically with native resources
/// within Google Cloud Platform. This takes the form of a "type" field which identifies the
/// resource, and a set of type-specific labels to uniquely identify a resource of that type.
///
/// See [Monitored resource types][mon_docs] for more information.
///
/// [mon_docs]: https://cloud.google.com/monitoring/api/resources
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct GcpTypedResource {
    /// The monitored resource type.
    ///
    /// For example, the type of a Compute Engine VM instance is `gce_instance`.
    #[configurable(metadata(docs::examples = "global", docs::examples = "gce_instance"))]
    pub r#type: String,

    /// Type-specific labels.
    #[serde(flatten)]
    #[configurable(metadata(
        docs::additional_props_description = "Values for all of the labels listed in the associated monitored resource descriptor.\n\nFor example, Compute Engine VM instances use the labels `projectId`, `instanceId`, and `zone`."
    ))]
    #[configurable(metadata(docs::examples = "label_examples()"))]
    pub labels: HashMap<String, String>,
}

fn label_examples() -> HashMap<String, String> {
    let mut example = HashMap::new();
    example.insert("type".to_string(), "global".to_string());
    example.insert("projectId".to_string(), "vector-123456".to_string());
    example.insert("instanceId".to_string(), "Twilight".to_string());
    example.insert("zone".to_string(), "us-central1-a".to_string());

    example
}

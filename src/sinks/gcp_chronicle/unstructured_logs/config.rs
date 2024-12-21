use goauth::scopes::Scope;
use vector_config::component::GenerateConfig;
use vector_lib::{
    config::{AcknowledgementsConfig, Input},
    sink::VectorSink,
};

use indoc::indoc;
use std::collections::HashMap;
use vector_lib::configurable::configurable_component;

use crate::{
    config::{SinkConfig, SinkContext},
    schema,
    http::HttpClient,
    sinks::{
        gcp_chronicle::{
            config::ChronicleCommonConfig,
            service::build_healthcheck
        },
        Healthcheck,
    },
    template::Template,
    tls::TlsSettings,
};
use vrl::value::Kind;

/// Configuration for the `gcp_chronicle_unstructured` sink.
#[configurable_component(sink(
    "gcp_chronicle_unstructured",
    "Store unstructured log events in Google Chronicle."
))]
#[derive(Clone, Debug)]
pub struct ChronicleUnstructuredConfig {

    #[serde(flatten)]
    pub chronicle_common: ChronicleCommonConfig,

    /// User-configured environment namespace to identify the data domain the logs originated from.
    #[configurable(metadata(docs::templateable))]
    #[configurable(metadata(
        docs::examples = "production",
        docs::examples = "production-{{ namespace }}",
    ))]
    #[configurable(metadata(docs::advanced))]
    pub namespace: Option<Template>,

    /// A set of labels that are attached to each batch of events.
    #[configurable(metadata(docs::examples = "chronicle_labels_examples()"))]
    #[configurable(metadata(docs::additional_props_description = "A Chronicle label."))]
    pub labels: Option<HashMap<String, String>>,

    /// The type of log entries in a request.
    ///
    /// This must be one of the [supported log types][unstructured_log_types_doc], otherwise
    /// Chronicle rejects the entry with an error.
    ///
    /// [unstructured_log_types_doc]: https://cloud.google.com/chronicle/docs/ingestion/parser-list/supported-default-parsers
    #[configurable(metadata(docs::examples = "WINDOWS_DNS", docs::examples = "{{ log_type }}"))]
    pub log_type: Template,
}

fn chronicle_labels_examples() -> HashMap<String, String> {
    let mut examples = HashMap::new();
    examples.insert("source".to_string(), "vector".to_string());
    examples.insert("tenant".to_string(), "marketing".to_string());
    examples
}

impl GenerateConfig for ChronicleUnstructuredConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            credentials_path = "/path/to/credentials.json"
            customer_id = "customer_id"
            namespace = "namespace"
            compression = "gzip"
            log_type = "log_type"
            encoding.codec = "text"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_chronicle_unstructured")]
impl SinkConfig for ChronicleUnstructuredConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let creds = self.chronicle_common.auth.build(Scope::MalachiteIngestion).await?;

        let tls = TlsSettings::from_options(&self.chronicle_common.tls)?;
        let client = HttpClient::new(tls, cx.proxy())?;

        let endpoint = self.chronicle_common.create_endpoint("v2/unstructuredlogentries:batchCreate")?;

        // For the healthcheck we see if we can fetch the list of available log types.
        let healthcheck_endpoint = self.chronicle_common.create_endpoint("v2/logtypes")?;

        let healthcheck = build_healthcheck(client.clone(), &healthcheck_endpoint, creds.clone())?;
        creds.spawn_regenerate_token();
        let sink = self.build_sink(client, endpoint, creds)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        let requirement =
            schema::Requirement::empty().required_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.chronicle_common.acknowledgements
    }
}


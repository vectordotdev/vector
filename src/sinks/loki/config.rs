use std::collections::HashMap;

use futures::future::FutureExt;
use value::Kind;
use vector_config::configurable_component;

use super::{healthcheck::healthcheck, sink::LokiSink};
use crate::{
    codecs::EncodingConfig,
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    http::{Auth, HttpClient, MaybeAuth},
    schema,
    sinks::{
        util::{BatchConfig, Compression, SinkBatchSettings, TowerRequestConfig, UriSerde},
        VectorSink,
    },
    template::Template,
    tls::{TlsConfig, TlsSettings},
};

/// Loki-specific compression.
#[configurable_component]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ExtendedCompression {
    /// Snappy compression.
    ///
    /// This implies sending push requests as Protocol Buffers.
    #[serde(rename = "snappy")]
    Snappy,
}

/// Compression configuration.
#[configurable_component]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum CompressionConfigAdapter {
    /// Basic compression.
    Original(Compression),

    /// Loki-specific compression.
    Extended(ExtendedCompression),
}

impl CompressionConfigAdapter {
    pub const fn content_encoding(self) -> Option<&'static str> {
        match self {
            CompressionConfigAdapter::Original(compression) => compression.content_encoding(),
            CompressionConfigAdapter::Extended(_) => Some("snappy"),
        }
    }
}

impl Default for CompressionConfigAdapter {
    fn default() -> Self {
        CompressionConfigAdapter::Extended(ExtendedCompression::Snappy)
    }
}

fn default_loki_path() -> String {
    "/loki/api/v1/push".to_string()
}

/// Configuration for the `loki` sink.
#[configurable_component(sink("loki"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct LokiConfig {
    /// The base URL of the Loki instance.
    ///
    /// The `path` value is appended to this.
    #[configurable(metadata(docs::examples = "http://localhost:3100"))]
    pub endpoint: UriSerde,

    /// The path to use in the URL of the Loki instance.
    #[serde(default = "default_loki_path")]
    pub path: String,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    /// The [tenant ID][tenant_id] to specify in requests to Loki.
    ///
    /// When running Loki locally, a tenant ID is not required.
    ///
    /// [tenant_id]: https://grafana.com/docs/loki/latest/operations/multi-tenancy/
    #[configurable(metadata(
        docs::examples = "some_tenant_id",
        docs::examples = "{{ event_field }}",
    ))]
    pub tenant_id: Option<Template>,

    /// A set of labels that are attached to each batch of events.
    ///
    /// Both keys and values are templateable, which enables you to attach dynamic labels to events.
    ///
    /// Valid label keys include `*`, and prefixes ending with `*`, to allow for the expansion of
    /// objects into multiple labels. See [Label expansion][label_expansion] for more information.
    ///
    /// Note: If the set of labels has high cardinality, this can cause drastic performance issues
    /// with Loki. To prevent this from happening, reduce the number of unique label keys and
    /// values.
    ///
    /// [label_expansion]: https://vector.dev/docs/reference/configuration/sinks/loki/#label-expansion
    #[configurable(metadata(docs::examples = "loki_labels_examples()"))]
    #[configurable(metadata(docs::additional_props_description = "A Loki label."))]
    pub labels: HashMap<Template, Template>,

    /// Whether or not to delete fields from the event when they are used as labels.
    #[serde(default = "crate::serde::default_false")]
    pub remove_label_fields: bool,

    /// Whether or not to remove the timestamp from the event payload.
    ///
    /// The timestamp is still sent as event metadata for Loki to use for indexing.
    #[serde(default = "crate::serde::default_true")]
    pub remove_timestamp: bool,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: CompressionConfigAdapter,

    #[configurable(derived)]
    #[serde(default)]
    pub out_of_order_action: OutOfOrderAction,

    #[configurable(derived)]
    pub auth: Option<Auth>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<LokiDefaultBatchSettings>,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn loki_labels_examples() -> HashMap<String, String> {
    let mut examples = HashMap::new();
    examples.insert("source".to_string(), "vector".to_string());
    examples.insert(
        "\"pod_labels_*\"".to_string(),
        "{{ kubernetes.pod_labels }}".to_string(),
    );
    examples.insert("\"*\"".to_string(), "{{ metadata }}".to_string());
    examples.insert(
        "{{ event_field }}".to_string(),
        "{{ some_other_event_field }}".to_string(),
    );
    examples
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LokiDefaultBatchSettings;

impl SinkBatchSettings for LokiDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(100_000);
    const MAX_BYTES: Option<usize> = Some(1_000_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Out-of-order event behavior.
///
/// Some sources may generate events with timestamps that aren't in chronological order. Even though the
/// sink sorts the events before sending them to Loki, there is a chance that another event could come in
/// that is out of order with the latest events sent to Loki. Prior to Loki 2.4.0, this
/// was not supported and would result in an error during the push request.
///
/// If you're using Loki 2.4.0 or newer, `Accept` is the preferred action, which lets Loki handle
/// any necessary sorting/reordering. If you're using an earlier version, then you must use `Drop`
/// or `RewriteTimestamp` depending on which option makes the most sense for your use case.
#[configurable_component]
#[derive(Copy, Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(rename_all = "snake_case")]
pub enum OutOfOrderAction {
    /// Drop the event.
    #[derivative(Default)]
    Drop,

    /// Rewrite the timestamp of the event to the timestamp of the latest event seen by the sink.
    RewriteTimestamp,

    /// Accept the event.
    ///
    /// The event is not dropped and is sent without modification.
    ///
    /// Requires Loki 2.4.0 or newer.
    Accept,
}

impl GenerateConfig for LokiConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"endpoint = "http://localhost:3100"
            encoding.codec = "json"
            labels = {}"#,
        )
        .unwrap()
    }
}

impl LokiConfig {
    pub(super) fn build_client(&self, cx: SinkContext) -> crate::Result<HttpClient> {
        let tls = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls, cx.proxy())?;
        Ok(client)
    }
}

#[async_trait::async_trait]
impl SinkConfig for LokiConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, crate::sinks::Healthcheck)> {
        if self.labels.is_empty() {
            return Err("`labels` must include at least one label.".into());
        }

        for label in self.labels.keys() {
            if !valid_label_name(label) {
                return Err(format!("Invalid label name {:?}", label.get_ref()).into());
            }
        }

        let client = self.build_client(cx)?;

        let config = LokiConfig {
            auth: self.auth.choose_one(&self.endpoint.auth)?,
            ..self.clone()
        };

        let sink = LokiSink::new(config.clone(), client.clone())?;

        let healthcheck = healthcheck(config, client).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        let requirement =
            schema::Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::new(self.encoding.config().input_type() & DataType::Log)
            .with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

pub fn valid_label_name(label: &Template) -> bool {
    label.is_dynamic() || {
        // Loki follows prometheus on this https://prometheus.io/docs/concepts/data_model/#metric-names-and-labels
        // Although that isn't explicitly said anywhere besides what's in the code.
        // The closest mention is in section about Parser Expression https://grafana.com/docs/loki/latest/logql/
        //
        // [a-zA-Z_][a-zA-Z0-9_]*
        //
        // '*' symbol at the end of the label name will be treated as a prefix for
        // underlying object keys.
        let mut label_trim = label.get_ref().trim();
        if let Some(without_opening_end) = label_trim.strip_suffix('*') {
            label_trim = without_opening_end
        }

        let mut label_chars = label_trim.chars();
        if let Some(ch) = label_chars.next() {
            (ch.is_ascii_alphabetic() || ch == '_')
                && label_chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        } else {
            label.get_ref().trim() == "*"
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use super::valid_label_name;

    #[test]
    fn valid_label_names() {
        assert!(valid_label_name(&"name".try_into().unwrap()));
        assert!(valid_label_name(&" name ".try_into().unwrap()));
        assert!(valid_label_name(&"bee_bop".try_into().unwrap()));
        assert!(valid_label_name(&"a09b".try_into().unwrap()));
        assert!(valid_label_name(&"abc_*".try_into().unwrap()));
        assert!(valid_label_name(&"_*".try_into().unwrap()));
        assert!(valid_label_name(&"*".try_into().unwrap()));

        assert!(!valid_label_name(&"0ab".try_into().unwrap()));
        assert!(!valid_label_name(&"".try_into().unwrap()));
        assert!(!valid_label_name(&" ".try_into().unwrap()));

        assert!(valid_label_name(&"{{field}}".try_into().unwrap()));
    }
}

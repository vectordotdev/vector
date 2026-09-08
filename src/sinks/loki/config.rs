use std::collections::HashMap;

use vrl::value::Kind;

use super::{
    append_loki_path,
    healthcheck::healthcheck,
    sink::{LokiSink, confine_template_keys},
};
use crate::{
    config::ValidatedSink,
    http::{Auth, HttpClient, MaybeAuth},
    schema,
    sinks::{
        prelude::*,
        util::{HttpEndpoint, service::TowerRequestSettings},
    },
    template::{ConfinementConfig, Template, UnconfinedTemplate},
};

const fn default_compression() -> Compression {
    Compression::Snappy
}

fn default_loki_path() -> String {
    "/loki/api/v1/push".to_string()
}

/// Configuration for the `loki` sink.
#[configurable_component(sink("loki", "Deliver log event data to the Loki aggregation system."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct LokiConfig {
    /// The base URL of the Loki instance.
    ///
    /// The `path` value is appended to this.
    #[configurable(metadata(docs::examples = "http://localhost:3100"))]
    pub endpoint: HttpEndpoint,

    /// The path to use in the URL of the Loki instance.
    #[serde(default = "default_loki_path")]
    pub path: String,

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
    #[configurable(metadata(docs::required = true))]
    pub labels: HashMap<Template, UnconfinedTemplate>,

    /// Whether or not to delete fields from the event when they are used as labels.
    #[serde(default = "crate::serde::default_false")]
    pub remove_label_fields: bool,

    /// Structured metadata that is attached to each batch of events.
    ///
    /// Both keys and values are templateable, which enables you to attach dynamic structured metadata to events.
    ///
    /// Valid metadata keys include `*`, and prefixes ending with `*`, to allow for the expansion of
    /// objects into multiple metadata entries. This follows the same logic as [Label expansion][label_expansion].
    ///
    /// [label_expansion]: https://vector.dev/docs/reference/configuration/sinks/loki/#label-expansion
    #[configurable(metadata(docs::examples = "loki_structured_metadata_examples()"))]
    #[configurable(metadata(docs::additional_props_description = "Loki structured metadata."))]
    #[serde(default)]
    pub structured_metadata: HashMap<Template, UnconfinedTemplate>,

    /// Whether or not to delete fields from the event when they are used in structured metadata.
    #[serde(default = "crate::serde::default_false")]
    pub remove_structured_metadata_fields: bool,

    /// Whether or not to remove the timestamp from the event payload.
    ///
    /// The timestamp is still sent as event metadata for Loki to use for indexing.
    #[serde(default = "crate::serde::default_true")]
    pub remove_timestamp: bool,

    /// Compression configuration.
    /// Snappy compression implies sending push requests as Protocol Buffers.
    #[serde(default = "default_compression")]
    pub compression: Compression,

    #[serde(default)]
    pub out_of_order_action: OutOfOrderAction,

    pub auth: Option<Auth>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    #[serde(default)]
    pub batch: BatchConfig<LokiDefaultBatchSettings>,

    pub tls: Option<TlsConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

fn loki_labels_examples() -> HashMap<String, String> {
    let mut examples = HashMap::new();
    examples.insert("source".to_string(), "vector".to_string());
    examples.insert(
        "pod_labels_*".to_string(),
        "{{ kubernetes.pod_labels }}".to_string(),
    );
    examples.insert(
        "event_{{ event_field }}".to_string(),
        "value_{{ some_other_event_field }}".to_string(),
    );
    examples
}

fn loki_structured_metadata_examples() -> HashMap<String, String> {
    let mut examples = HashMap::new();
    examples.insert("source".to_string(), "vector".to_string());
    examples.insert(
        "pod_labels_*".to_string(),
        "{{ kubernetes.pod_labels }}".to_string(),
    );
    examples.insert(
        "event_{{ event_field }}".to_string(),
        "value_{{ some_other_event_field }}".to_string(),
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
#[derive(Copy, Clone, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutOfOrderAction {
    /// Accept the event.
    ///
    /// The event is not dropped and is sent without modification.
    ///
    /// Requires Loki 2.4.0 or newer.
    #[default]
    Accept,

    /// Rewrite the timestamp of the event to the timestamp of the latest event seen by the sink.
    RewriteTimestamp,

    /// Drop the event.
    Drop,
}

impl GenerateConfig for LokiConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r#"endpoint: http://localhost:3100
            encoding:
              codec: json
            labels: {}"#,
        })
        .unwrap()
    }
}

impl LokiConfig {
    pub(super) fn build_client(&self, cx: SinkContext) -> crate::Result<HttpClient> {
        let tls = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls, cx.proxy())?;
        Ok(client)
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "loki")]
impl SinkConfig for LokiConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
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

#[derive(Clone, Debug)]
pub struct ValidatedLokiSink {
    /// The push URL: `endpoint` with `path` appended. Used by `LokiService`.
    pub(super) endpoint: HttpEndpoint,
    /// The credential-free configured endpoint, including any user-supplied base path.
    /// The healthcheck appends `ready` or `/` to this URL.
    pub(super) base_endpoint: HttpEndpoint,
    pub(super) auth: Option<Auth>,
    pub(super) request_limits: TowerRequestSettings,
    pub(super) transformer: Transformer,
    pub(super) tenant_id: Option<ConfinedTemplate>,
    pub(super) labels: HashMap<ConfinedTemplate, UnconfinedTemplate>,
    pub(super) structured_metadata: HashMap<ConfinedTemplate, UnconfinedTemplate>,
    pub(super) batch_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for LokiConfig {
    type Validated = ValidatedLokiSink;

    fn validate(&self) -> crate::Result<ValidatedLokiSink> {
        if self.labels.is_empty() {
            return Err("`labels` must include at least one label.".into());
        }

        for label in self.labels.keys() {
            if !valid_label_name(label) {
                return Err(format!("Invalid label name {:?}", label.get_ref()).into());
            }
        }

        // Extract basic-auth credentials embedded in the endpoint URL and strip
        // the userinfo, so credentials are sent as an `Authorization` header
        // rather than in the request URL.
        let (base_endpoint, endpoint_auth) = self.endpoint.clone().extract_basic_auth()?;
        let auth = self.auth.choose_one(&endpoint_auth)?;

        let request_limits = match self.out_of_order_action {
            OutOfOrderAction::Accept => self.request.into_settings(),
            OutOfOrderAction::Drop | OutOfOrderAction::RewriteTimestamp => {
                let mut settings = self.request.into_settings();
                settings.concurrency = Some(1);
                settings
            }
        };

        let transformer = self.encoding.transformer();

        let tenant_id = self
            .tenant_id
            .clone()
            .map(|template| template.confine(&self.confinement, LokiConfig::NAME, "tenant_id"))
            .transpose()?;

        let labels = confine_template_keys(
            self.labels.clone(),
            &self.confinement,
            LokiConfig::NAME,
            "labels[key]",
        )?;
        let structured_metadata = confine_template_keys(
            self.structured_metadata.clone(),
            &self.confinement,
            LokiConfig::NAME,
            "structured_metadata[key]",
        )?;

        // The push URL appends `path`; the healthcheck appends `ready`/`` to the
        // configured base endpoint, so both are retained.
        let endpoint = append_loki_path(&base_endpoint, &self.path)?;

        let batch_settings = self.batch.into_batcher_settings()?;

        Ok(ValidatedLokiSink {
            endpoint,
            base_endpoint,
            auth,
            request_limits,
            transformer,
            tenant_id,
            labels,
            structured_metadata,
            batch_settings,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedLokiSink,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, crate::sinks::Healthcheck)> {
        let healthcheck_uri = cx.healthcheck.uri.clone();
        let client = self.build_client(cx)?;

        let sink = LokiSink::from_validated(self, validated.clone(), client.clone())?;

        let healthcheck = healthcheck(
            validated.base_endpoint.clone(),
            validated.auth.clone(),
            healthcheck_uri,
            client,
        )
        .boxed();
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
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
    use crate::{
        config::ValidatedSink,
        sinks::loki::LokiConfig,
        template::{ConfinementConfig, Template},
    };

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

    #[test]
    fn confinement_rejects_unconfined_tenant_id() {
        let template = Template::try_from("{{ tenant }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "loki", "tenant_id");
        assert!(
            result.is_err(),
            "bare tenant_id template with no literal prefix must be rejected"
        );
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_tenant_id() {
        let template = Template::try_from("{{ tenant }}").unwrap();
        let config = ConfinementConfig {
            dangerously_allow_unconfined_template_resolution: true,
        };
        let result = template.confine(&config, "loki", "tenant_id");
        assert!(result.is_ok(), "opt-out must allow bare tenant_id template");
    }

    #[test]
    fn confinement_prefixed_tenant_id_locks_org_prefix() {
        use crate::event::{Event, LogEvent};
        use vrl::event_path;
        // "team-{{ org }}" has literal prefix "team-"; an attacker controlling `org`
        // cannot steer the rendered value to an org outside the "team-" namespace.
        let template = Template::try_from("team-{{ org }}").unwrap();
        let config = ConfinementConfig::default();
        let confined = template.confine(&config, "loki", "tenant_id").unwrap();
        let mut event = LogEvent::default();
        event.insert(event_path!("org"), "other-tenant-entirely");
        let rendered = confined.render_string(&Event::Log(event)).unwrap();
        assert!(
            rendered.starts_with("team-"),
            "operator-controlled prefix must be preserved in rendered tenant_id"
        );
    }

    #[test]
    fn deserialize_rejects_non_http_endpoint() {
        let result = serde_yaml::from_str::<LokiConfig>(
            r#"
            endpoint: "ftp://localhost:3100"
            labels:
              test_name: "placeholder"
            encoding:
              codec: json
            "#,
        );

        assert!(
            result.is_err(),
            "non-http endpoints must fail at config load"
        );
        let message = result.unwrap_err().to_string();
        assert!(message.contains("ftp://localhost:3100"), "{message}");
    }

    #[test]
    fn validate_returns_usable_values() {
        let config: LokiConfig = serde_yaml::from_str(
            r#"
            endpoint: "http://localhost:3100"
            labels:
              test_name: "placeholder"
            encoding:
              codec: json
            "#,
        )
        .unwrap();

        let validated = config.validate().expect("validation should succeed");
        assert!(validated.auth.is_none()); // Default has no auth
        assert_eq!(validated.labels.len(), 1);
        assert!(validated.batch_settings.timeout > std::time::Duration::ZERO);
    }

    #[test]
    fn validate_extracts_endpoint_basic_auth() {
        let config: LokiConfig = serde_yaml::from_str(
            r#"
            endpoint: "http://user:pass@localhost:3100"
            labels:
              test_name: "placeholder"
            encoding:
              codec: json
            "#,
        )
        .unwrap();

        let validated = config.validate().expect("validation should succeed");
        assert!(
            validated.auth.is_some(),
            "credentials embedded in the endpoint must be extracted as auth"
        );
        assert!(
            !validated.endpoint.to_string().contains('@'),
            "userinfo must be stripped from the endpoint retained for build"
        );
    }

    #[test]
    fn validate_rejects_conflicting_auth() {
        let config: LokiConfig = serde_yaml::from_str(
            r#"
            endpoint: "http://user:pass@localhost:3100"
            auth:
              strategy: "basic"
              user: "other"
              password: "other"
            labels:
              test_name: "placeholder"
            encoding:
              codec: json
            "#,
        )
        .unwrap();

        assert!(
            config.validate().is_err(),
            "explicit auth and endpoint-embedded auth must not both be configured"
        );
    }

    #[test]
    fn validate_rejects_invalid_path() {
        let config: LokiConfig = serde_yaml::from_str(
            r#"
            endpoint: "http://localhost:3100"
            path: "foo bar"
            labels:
              test_name: "placeholder"
            encoding:
              codec: json
            "#,
        )
        .unwrap();

        assert!(
            config.validate().is_err(),
            "a path containing a space cannot be appended into a valid URI and must be rejected"
        );
    }
}

use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
};

use futures::{FutureExt, TryFutureExt};
use vector_lib::{
    configurable::configurable_component,
    lookup::{event_path, lookup_v2::ConfigValuePath},
    schema::Requirement,
    stream::BatcherSettings,
};
use vrl::value::Kind;

use crate::{
    codecs::Transformer,
    config::{AcknowledgementsConfig, DataType, Input, SinkConfig, SinkContext, ValidatedSink},
    event::{EventRef, LogEvent, Value},
    http::{HttpClient, QueryParameters},
    internal_events::TemplateRenderingError,
    sinks::{
        Healthcheck, VectorSink,
        elasticsearch::{
            ElasticsearchApiVersion, ElasticsearchAuthConfig, ElasticsearchCommon,
            ElasticsearchCommonMode, ElasticsearchMode, ParseError, VersionType,
            health::ElasticsearchHealthLogic,
            retry::ElasticsearchRetryLogic,
            service::{ElasticsearchService, HttpRequestBuilder},
            sink::ElasticsearchSink,
        },
        util::{
            BatchConfig, Compression, HttpEndpoint, RealtimeSizeBasedDefaultBatchSettings,
            TowerRequestSettings, http::RequestConfig, service::HealthConfig,
        },
    },
    template::{ConfinedTemplate, ConfinementConfig, Template, UnconfinedTemplate},
    tls::TlsConfig,
    transforms::metric_to_log::MetricToLogConfig,
};

/// The field name for the timestamp required by data stream mode
pub const DATA_STREAM_TIMESTAMP_KEY: &str = "@timestamp";

/// The Amazon OpenSearch service type, either managed or serverless; primarily, selects the
/// correct AWS service to use when calculating the AWS v4 signature + disables features
/// unsupported by serverless: Elasticsearch API version autodetection, health checks
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
#[derive(Default)]
pub enum OpenSearchServiceType {
    /// Elasticsearch or OpenSearch Managed domain
    #[default]
    Managed,
    /// OpenSearch Serverless collection
    Serverless,
}

impl OpenSearchServiceType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            OpenSearchServiceType::Managed => "es",
            OpenSearchServiceType::Serverless => "aoss",
        }
    }
}

/// Configuration for the `elasticsearch` sink.
#[configurable_component(sink("elasticsearch", "Index observability events in Elasticsearch."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ElasticsearchConfig {
    /// The Elasticsearch endpoint to send logs to.
    ///
    /// The endpoint must contain an HTTP scheme, and may specify a
    /// hostname or IP address and port.
    #[serde(default)]
    #[configurable(
        deprecated = "This option has been deprecated, the `endpoints` option should be used instead."
    )]
    #[configurable(required_one_of = "endpoint")]
    pub endpoint: Option<HttpEndpoint>,

    /// A list of Elasticsearch endpoints to send logs to.
    ///
    /// The endpoint must contain an HTTP scheme, and may specify a
    /// hostname or IP address and port.
    /// The endpoint may include basic authentication credentials,
    /// e.g., `https://user:password@example.com`. If credentials are provided in the endpoint,
    /// they will be used to authenticate against Elasticsearch.
    ///
    /// If `auth` is specified and the endpoint contains credentials,
    /// a configuration error will be raised.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "http://10.24.32.122:9000"))]
    #[configurable(metadata(docs::examples = "https://example.com"))]
    #[configurable(metadata(docs::examples = "https://user:password@example.com"))]
    #[configurable(required_one_of = "endpoint")]
    pub endpoints: Vec<HttpEndpoint>,

    /// The [`doc_type`][doc_type] for your index data.
    ///
    /// This is only relevant for Elasticsearch <= 6.X. If you are using >= 7.0 you do not need to
    /// set this option since Elasticsearch has removed it.
    ///
    /// [doc_type]: https://www.elastic.co/guide/en/elasticsearch/reference/6.8/actions-index.html
    #[serde(default = "default_doc_type")]
    pub doc_type: String,

    /// The API version of Elasticsearch.
    ///
    /// Amazon OpenSearch Serverless requires this option to be set to `auto` (the default).
    #[serde(default)]
    pub api_version: ElasticsearchApiVersion,

    /// Whether or not to send the `type` field to Elasticsearch.
    ///
    /// The `type` field was deprecated in Elasticsearch 7.x and removed in Elasticsearch 8.x.
    ///
    /// If enabled, the `doc_type` option is ignored.
    #[serde(default)]
    #[configurable(
        deprecated = "This option has been deprecated, the `api_version` option should be used instead."
    )]
    pub suppress_type_name: bool,

    /// Whether or not to retry successful requests containing partial failures.
    ///
    /// To avoid duplicates in Elasticsearch, please use option `id_key`.
    #[serde(default)]
    pub request_retry_partial: bool,

    /// The name of the event key that should map to Elasticsearch’s [`_id` field][es_id].
    ///
    /// By default, the `_id` field is not set, which allows Elasticsearch to set this
    /// automatically. Setting your own Elasticsearch IDs can [hinder performance][perf_doc].
    ///
    /// [es_id]: https://www.elastic.co/guide/en/elasticsearch/reference/current/mapping-id-field.html
    /// [perf_doc]: https://www.elastic.co/guide/en/elasticsearch/reference/master/tune-for-indexing-speed.html#_use_auto_generated_ids
    #[serde(default)]
    #[configurable(metadata(docs::examples = "id"))]
    #[configurable(metadata(docs::examples = "_id"))]
    pub id_key: Option<ConfigValuePath>,

    /// The name of the pipeline to apply.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "pipeline-name"))]
    pub pipeline: Option<String>,

    #[serde(default)]
    pub mode: ElasticsearchMode,

    #[serde(default)]
    pub compression: Compression,

    #[serde(skip_serializing_if = "crate::serde::is_default", default)]
    pub encoding: Transformer,

    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[serde(default)]
    pub request: RequestConfig,

    pub auth: Option<ElasticsearchAuthConfig>,

    /// Custom parameters to add to the query string for each HTTP request sent to Elasticsearch.
    #[serde(default)]
    #[configurable(metadata(docs::additional_props_description = "A query string parameter."))]
    #[configurable(metadata(docs::examples = "query_examples()"))]
    pub query: Option<QueryParameters>,

    #[serde(default)]
    #[cfg(feature = "aws-core")]
    pub aws: Option<crate::aws::RegionOrEndpoint>,

    /// Amazon OpenSearch service type
    #[serde(default)]
    pub opensearch_service_type: OpenSearchServiceType,

    #[serde(default)]
    pub tls: Option<TlsConfig>,

    #[serde(default)]
    #[serde(rename = "distribution")]
    pub endpoint_health: Option<HealthConfig>,

    // TODO: `bulk` and `data_stream` are each only relevant if the `mode` is set to their
    // corresponding mode. An improvement to look into would be to extract the `BulkConfig` and
    // `DataStreamConfig` into the `mode` enum variants. Doing so would remove them from the root
    // of the config here and thus any post serde config parsing manual error prone logic.
    #[serde(alias = "normal", default)]
    pub bulk: BulkConfig,

    #[serde(default)]
    pub data_stream: Option<DataStreamConfig>,

    #[serde(default)]
    pub metrics: Option<MetricToLogConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

fn default_doc_type() -> String {
    "_doc".to_owned()
}

fn query_examples() -> HashMap<String, String> {
    HashMap::<_, _>::from_iter([("X-Powered-By".to_owned(), "Vector".to_owned())])
}

impl Default for ElasticsearchConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            endpoints: vec![],
            doc_type: default_doc_type(),
            api_version: Default::default(),
            suppress_type_name: false,
            request_retry_partial: false,
            id_key: None,
            pipeline: None,
            mode: Default::default(),
            compression: Default::default(),
            encoding: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            auth: None,
            query: None,
            #[cfg(feature = "aws-core")]
            aws: None,
            opensearch_service_type: Default::default(),
            tls: None,
            endpoint_health: None,
            bulk: BulkConfig::default(), // the default mode is Bulk
            data_stream: None,
            metrics: None,
            acknowledgements: Default::default(),
            confinement: ConfinementConfig::default(),
        }
    }
}

impl ElasticsearchConfig {
    /// Build the render-capable [`ElasticsearchCommonMode`], confining the templated fields of the
    /// active mode. `common_mode()` ignores the inactive branch, so a leftover unused template
    /// (e.g. a `bulk.index` in a config that runs in `data_stream` mode) is never confined and
    /// cannot reject an otherwise-valid config.
    pub fn common_mode(&self) -> crate::Result<ElasticsearchCommonMode> {
        match self.mode {
            ElasticsearchMode::Bulk => Ok(ElasticsearchCommonMode::Bulk {
                // Only `index` is a routing field, so it is the only bulk template that is confined.
                // `action` and `version` are not routing values and render unconfined, hence their
                // `UnconfinedTemplate` type.
                index: self.bulk.index.clone().confine(
                    &self.confinement,
                    Self::NAME,
                    "bulk.index",
                )?,
                template_fallback_index: self.bulk.template_fallback_index.clone(),
                action: self.bulk.action.clone(),
                version: self.bulk.version.clone(),
                version_type: self.bulk.version_type,
            }),
            ElasticsearchMode::DataStream => Ok(ElasticsearchCommonMode::DataStream(
                self.data_stream
                    .clone()
                    .unwrap_or_default()
                    .confine(&self.confinement, Self::NAME)?,
            )),
        }
    }
}

/// Elasticsearch bulk mode configuration.
#[configurable_component]
#[derive(Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct BulkConfig {
    /// Action to use when making requests to the [Elasticsearch Bulk API][es_bulk].
    ///
    /// Only `index`, `create` and `update` actions are supported.
    ///
    /// [es_bulk]: https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html
    #[serde(default = "default_bulk_action")]
    #[configurable(metadata(docs::examples = "create"))]
    #[configurable(metadata(docs::examples = "{{ action }}"))]
    pub action: UnconfinedTemplate,

    /// The name of the index to write events to.
    #[serde(default = "default_index")]
    #[configurable(metadata(docs::examples = "application-{{ application_id }}-%Y-%m-%d"))]
    #[configurable(metadata(docs::examples = "{{ index }}"))]
    pub index: Template,

    /// The default index to write events to if the template in `bulk.index` cannot be resolved
    #[configurable(metadata(docs::examples = "test-index"))]
    pub template_fallback_index: Option<String>,

    /// Version field value.
    #[configurable(metadata(docs::examples = "{{ obj_version }}-%Y-%m-%d"))]
    #[configurable(metadata(docs::examples = "123"))]
    pub version: Option<UnconfinedTemplate>,

    /// Version type.
    ///
    /// Possible values are `internal`, `external` or `external_gt` and `external_gte`.
    ///
    /// [es_index_versioning]: https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-index_.html#index-versioning
    #[serde(default = "default_version_type")]
    #[configurable(metadata(docs::examples = "internal"))]
    #[configurable(metadata(docs::examples = "external"))]
    pub version_type: VersionType,
}

fn default_bulk_action() -> UnconfinedTemplate {
    UnconfinedTemplate::try_from("index").expect("unable to parse template")
}

fn default_index() -> Template {
    Template::try_from("vector-%Y.%m.%d").expect("unable to parse template")
}

const fn default_version_type() -> VersionType {
    VersionType::Internal
}

impl Default for BulkConfig {
    fn default() -> Self {
        Self {
            action: default_bulk_action(),
            index: default_index(),
            template_fallback_index: Default::default(),
            version: Default::default(),
            version_type: default_version_type(),
        }
    }
}

/// Elasticsearch data stream mode configuration.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DataStreamConfig {
    /// The data stream type used to construct the data stream at index time.
    #[serde(rename = "type", default = "DataStreamConfig::default_type")]
    #[configurable(metadata(docs::examples = "metrics"))]
    #[configurable(metadata(docs::examples = "synthetics"))]
    #[configurable(metadata(docs::examples = "{{ type }}"))]
    pub dtype: Template,

    /// The data stream dataset used to construct the data stream at index time.
    #[serde(default = "DataStreamConfig::default_dataset")]
    #[configurable(metadata(docs::examples = "generic"))]
    #[configurable(metadata(docs::examples = "nginx"))]
    #[configurable(metadata(docs::examples = "{{ service }}"))]
    pub dataset: Template,

    /// The data stream namespace used to construct the data stream at index time.
    #[serde(default = "DataStreamConfig::default_namespace")]
    #[configurable(metadata(docs::examples = "{{ environment }}"))]
    pub namespace: Template,

    /// Automatically routes events by deriving the data stream name using specific event fields.
    ///
    /// The format of the data stream name is `<type>-<dataset>-<namespace>`, where each value comes
    /// from the `data_stream` configuration field of the same name.
    ///
    /// If enabled, the value of the `data_stream.type`, `data_stream.dataset`, and
    /// `data_stream.namespace` event fields are used if they are present. Otherwise, the values
    /// set in this configuration are used.
    #[serde(default = "DataStreamConfig::default_auto_routing")]
    pub auto_routing: bool,

    /// Automatically adds and syncs the `data_stream.*` event fields if they are missing from the event.
    ///
    /// This ensures that fields match the name of the data stream that is receiving events.
    #[serde(default = "DataStreamConfig::default_sync_fields")]
    pub sync_fields: bool,
}

impl Default for DataStreamConfig {
    fn default() -> Self {
        Self {
            dtype: Self::default_type(),
            dataset: Self::default_dataset(),
            namespace: Self::default_namespace(),
            auto_routing: Self::default_auto_routing(),
            sync_fields: Self::default_sync_fields(),
        }
    }
}

impl DataStreamConfig {
    fn default_type() -> Template {
        Template::try_from("logs").expect("couldn't build default type template")
    }

    fn default_dataset() -> Template {
        Template::try_from("generic").expect("couldn't build default dataset template")
    }

    fn default_namespace() -> Template {
        Template::try_from("default").expect("couldn't build default namespace template")
    }

    const fn default_auto_routing() -> bool {
        true
    }

    const fn default_sync_fields() -> bool {
        true
    }

    /// Confine the templated fields, producing a render-capable [`DataStreamMode`].
    ///
    /// This is the only way to obtain a `DataStreamMode`, so the `data_stream.*` templates can
    /// never be rendered without first passing through confinement.
    pub fn confine(
        self,
        confinement: &ConfinementConfig,
        component_name: &'static str,
    ) -> crate::Result<DataStreamMode> {
        Ok(DataStreamMode {
            dtype: self
                .dtype
                .confine(confinement, component_name, "data_stream.type")?,
            dataset: self
                .dataset
                .confine(confinement, component_name, "data_stream.dataset")?,
            namespace: self.namespace.confine(
                confinement,
                component_name,
                "data_stream.namespace",
            )?,
            auto_routing: self.auto_routing,
            sync_fields: self.sync_fields,
        })
    }
}

/// Runtime counterpart of [`DataStreamConfig`], holding confined templates.
///
/// Obtained via [`DataStreamConfig::confine`]. The render methods here operate on
/// [`ConfinedTemplate`], so the confinement checker is enforced on every rendered value.
#[derive(Clone, Debug)]
pub struct DataStreamMode {
    dtype: ConfinedTemplate,
    dataset: ConfinedTemplate,
    namespace: ConfinedTemplate,
    auto_routing: bool,
    sync_fields: bool,
}

impl DataStreamMode {
    /// If there is a `timestamp` field, rename it to the expected `@timestamp` for Elastic Common Schema.
    pub fn remap_timestamp(&self, log: &mut LogEvent) {
        if let Some(timestamp_key) = log.timestamp_path().cloned() {
            if timestamp_key.to_string() == DATA_STREAM_TIMESTAMP_KEY {
                return;
            }

            log.rename_key(&timestamp_key, event_path!(DATA_STREAM_TIMESTAMP_KEY));
        }
    }

    pub fn dtype<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<String> {
        self.dtype
            .render_string(event)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("data_stream.type"),
                    drop_event: true,
                });
            })
            .ok()
    }

    pub fn dataset<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<String> {
        self.dataset
            .render_string(event)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("data_stream.dataset"),
                    drop_event: true,
                });
            })
            .ok()
    }

    pub fn namespace<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<String> {
        self.namespace
            .render_string(event)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("data_stream.namespace"),
                    drop_event: true,
                });
            })
            .ok()
    }

    pub fn sync_fields(&self, log: &mut LogEvent) {
        if !self.sync_fields {
            return;
        }

        let dtype = self.dtype(&*log);
        let dataset = self.dataset(&*log);
        let namespace = self.namespace(&*log);

        if log.as_map().is_none() {
            *log.value_mut() = Value::Object(BTreeMap::new());
        }
        let existing = log
            .as_map_mut()
            .expect("must be a map")
            .entry("data_stream".into())
            .or_insert_with(|| Value::Object(BTreeMap::new()))
            .as_object_mut_unwrap();

        if let Some(dtype) = dtype {
            existing
                .entry("type".into())
                .or_insert_with(|| dtype.into());
        }
        if let Some(dataset) = dataset {
            existing
                .entry("dataset".into())
                .or_insert_with(|| dataset.into());
        }
        if let Some(namespace) = namespace {
            existing
                .entry("namespace".into())
                .or_insert_with(|| namespace.into());
        }
    }

    pub fn index(&self, log: &LogEvent) -> Option<String> {
        let (dtype, dataset, namespace) = if !self.auto_routing {
            (self.dtype(log)?, self.dataset(log)?, self.namespace(log)?)
        } else {
            let data_stream = log
                .get(event_path!("data_stream"))
                .and_then(|ds| ds.as_object());
            let dtype =
                auto_routed_value(data_stream, "type", &self.dtype, "data_stream.type", || {
                    self.dtype(log)
                })?;
            let dataset = auto_routed_value(
                data_stream,
                "dataset",
                &self.dataset,
                "data_stream.dataset",
                || self.dataset(log),
            )?;
            let namespace = auto_routed_value(
                data_stream,
                "namespace",
                &self.namespace,
                "data_stream.namespace",
                || self.namespace(log),
            )?;
            (dtype, dataset, namespace)
        };

        let name = [dtype, dataset, namespace]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        Some(name)
    }
}

/// Auto-routed helper: prefer the event's `data_stream.<key>` field, but run
/// that raw value through the confinement check attached to the
/// corresponding template so `auto_routing` can't be used to bypass the
/// build-time confinement on `data_stream.{type,dataset,namespace}`. Falls
/// back to `fallback` (normal template rendering) when the event field is
/// missing.
fn auto_routed_value<F>(
    data_stream: Option<&std::collections::BTreeMap<vector_lib::event::KeyString, Value>>,
    key: &str,
    template: &ConfinedTemplate,
    field: &'static str,
    fallback: F,
) -> Option<String>
where
    F: FnOnce() -> Option<String>,
{
    match data_stream.and_then(|ds| ds.get(key)) {
        Some(value) => {
            let s = value.to_string_lossy().into_owned();
            // Two layers of validation:
            //
            // 1. If the operator-authored template has a confinement checker,
            //    run it. That catches values that violate a `PrefixChecker`
            //    base or contain `..` segments.
            //
            // 2. If the template is *static* (e.g. the default `type = "logs"`),
            //    no checker is attached and `check_confinement` returns Ok
            //    for anything. Data-stream names are simple identifiers per
            //    Elasticsearch's naming rules, so anything containing a path
            //    separator, `..`, NUL, or over 255 bytes is either an attack
            //    or would be rejected by Elasticsearch on ingest anyway.
            template
                .check_confinement(&s)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some(field),
                        drop_event: true,
                    });
                })
                .ok()?;
            if !is_valid_data_stream_component(&s, field) {
                emit!(TemplateRenderingError {
                    error: crate::template::TemplateRenderingError::Confined {
                        rendered_preview: crate::template::confined_preview(&s),
                        rendered_len: s.len(),
                        message: format!(
                            "auto-routed {field} value is not a valid data-stream identifier"
                        ),
                    },
                    field: Some(field),
                    drop_event: true,
                });
                return None;
            }
            Some(s)
        }
        None => fallback(),
    }
}

/// Baseline sanity check for auto-routed `data_stream.*` values pulled off
/// events. Rejects anything Elasticsearch itself would reject at ingest, so
/// that attacker-controlled values can't slip past Vector's drop guard and
/// blow up later in the request pipeline.
///
/// Rules (from the Elasticsearch data-stream / index naming spec):
///
/// - No control characters, NUL bytes, or the characters
///   `\ / * ? " < > | , # : <space>` (forbidden in any index or
///   data-stream name).
/// - No exact `.` or `.._` path segments (traversal).
/// - Cannot start with `- _ + .` (reserved leading characters).
/// - Combined `type-dataset-namespace` is capped at 100 bytes by
///   Elasticsearch, so each part must be well under that. We use 100
///   here as a per-part cap — cheap and conservative.
/// - `dataset` and `namespace` additionally forbid `-` because `-` is
///   the separator between the three parts of a data-stream name.
///
/// Empty is legitimate — `DataStreamConfig::index` filters empty parts
/// out of the joined name, so an event field explicitly overriding one
/// part to `""` just skips it.
fn is_valid_data_stream_component(s: &str, field: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if s.len() > 100 {
        return false;
    }
    // Forbidden anywhere in the string.
    const FORBIDDEN: &[char] = &[
        '\0', '/', '\\', '*', '?', '"', '<', '>', '|', ',', '#', ':', ' ',
    ];
    if s.contains(FORBIDDEN) {
        return false;
    }
    // Reserved leading characters.
    if let Some(first) = s.chars().next()
        && matches!(first, '-' | '_' | '+' | '.')
    {
        return false;
    }
    // Path-traversal segments — belt-and-braces even though `/` and `\`
    // are already forbidden.
    if s == "." || s == ".." {
        return false;
    }
    // Control characters have no place in a routing identifier.
    if s.chars().any(|c| c.is_control()) {
        return false;
    }
    // `-` is the separator inside the composed data-stream name
    // (`{type}-{dataset}-{namespace}`), so `dataset` and `namespace`
    // must not contain it. `type` values (`logs`, `metrics`, …) also
    // conventionally don't contain `-`, but we accept it there for
    // forward compatibility with custom types.
    if (field == "data_stream.dataset" || field == "data_stream.namespace") && s.contains('-') {
        return false;
    }
    true
}

#[async_trait::async_trait]
#[typetag::serde(name = "elasticsearch")]
impl SinkConfig for ElasticsearchConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        let requirements = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::new(DataType::Metric | DataType::Log).with_schema_requirement(requirements)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedElasticsearch {
    request_limits: TowerRequestSettings,
    health_config: HealthConfig,
    batch_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for ElasticsearchConfig {
    type Validated = ValidatedElasticsearch;

    fn validate(&self) -> crate::Result<ValidatedElasticsearch> {
        // Mirror the pure endpoint-required/exclusive checks from
        // `ElasticsearchCommon::parse_many` so configs that deterministically
        // fail at build time are rejected here instead.
        match (&self.endpoint, self.endpoints.is_empty()) {
            (Some(_), false) => return Err(ParseError::EndpointsExclusive.into()),
            (None, true) => return Err(ParseError::EndpointRequired.into()),
            _ => {}
        }

        // `HttpEndpoint` validates the scheme (http/https, defaulting a
        // missing scheme to https) and rejects host-less endpoints at load
        // time, so the manual checks are no longer needed here.

        // Mirror the pure Serverless checks from `ElasticsearchCommon::parse_config`
        // (auth strategy, region, and API-version) so configs that deterministically
        // fail at build time are rejected here instead.
        if self.opensearch_service_type == OpenSearchServiceType::Serverless {
            match &self.auth {
                #[cfg(feature = "aws-core")]
                Some(ElasticsearchAuthConfig::Aws(_)) => (),
                _ => return Err(ParseError::OpenSearchServerlessRequiresAwsAuth.into()),
            }
        }
        if self.opensearch_service_type == OpenSearchServiceType::Serverless
            && self.api_version != ElasticsearchApiVersion::Auto
        {
            return Err(ParseError::ServerlessElasticsearchApiVersionMustBeAuto.into());
        }

        // Mirror the pure region check from `ElasticsearchCommon::extract_auth` so
        // AWS auth configs without a region are rejected during validation instead
        // of failing at build time.
        #[cfg(feature = "aws-core")]
        if matches!(self.auth, Some(ElasticsearchAuthConfig::Aws(_)))
            && self.aws.as_ref().and_then(|aws| aws.region()).is_none()
        {
            return Err(ParseError::RegionRequired.into());
        }

        // Run the pure routing-template confinement check for the active mode
        // so unconfined `bulk.index` / `data_stream.*` templates are rejected
        // here. The confined mode itself is reconstructed during build (inside
        // `ElasticsearchCommon::parse_many`).
        self.common_mode()?;

        // Mirror the pure batch-settings and versioning checks from
        // `ElasticsearchCommon::parse_config` so configs that deterministically
        // fail at build time are rejected here instead.
        let batch_settings = self.batch.into_batcher_settings()?;

        if self.bulk.version.is_some() && self.bulk.version_type == VersionType::Internal {
            return Err(ParseError::ExternalVersionIgnoredWithInternalVersioning.into());
        }
        if self.bulk.version.is_some()
            && (self.bulk.version_type == VersionType::External
                || self.bulk.version_type == VersionType::ExternalGte)
            && self.id_key.is_none()
        {
            return Err(ParseError::ExternalVersioningWithoutDocumentID.into());
        }
        if self.bulk.version.is_none()
            && (self.bulk.version_type == VersionType::External
                || self.bulk.version_type == VersionType::ExternalGte)
        {
            return Err(ParseError::ExternalVersioningWithoutVersion.into());
        }

        let request_limits = self.request.tower.into_settings();
        let health_config = self.endpoint_health.clone().unwrap_or_default();

        Ok(ValidatedElasticsearch {
            request_limits,
            health_config,
            batch_settings,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedElasticsearch,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        // Confinement of the active mode's routing templates happens in
        // [`ElasticsearchConfig::common_mode`], which each `ElasticsearchCommon` calls while
        // parsing (which performs I/O: AWS credential resolution and API-version autodetection).
        // The pure request/health settings were resolved during `validate`.
        let commons = ElasticsearchCommon::parse_many(self, cx.proxy()).await?;
        let common = commons[0].clone();

        let client = HttpClient::new(common.tls_settings.clone(), cx.proxy())?;

        let health_config = validated.health_config.clone();

        let services = commons
            .iter()
            .map(|common| {
                let endpoint = common.base_url.clone();

                let http_request_builder = HttpRequestBuilder::new(common, self);
                let service = ElasticsearchService::new(client.clone(), http_request_builder);

                (endpoint, service)
            })
            .collect::<Vec<_>>();

        let service = validated.request_limits.clone().distributed_service(
            ElasticsearchRetryLogic {
                retry_partial: self.request_retry_partial,
            },
            services,
            health_config,
            ElasticsearchHealthLogic,
            1,
        );

        let sink = ElasticsearchSink::new(&common, self, service, validated.batch_settings)?;

        let stream = VectorSink::from_event_streamsink(sink);

        let healthcheck = futures::future::select_ok(
            commons
                .into_iter()
                .map(move |common| common.healthcheck(client.clone()).boxed()),
        )
        .map_ok(|((), _)| ())
        .boxed();
        Ok((stream, healthcheck))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{ConfinementConfig, Template};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ElasticsearchConfig>();
    }

    #[test]
    fn validate_rejects_missing_endpoints() {
        use crate::config::ValidatedSink;
        let config: ElasticsearchConfig = serde_yaml::from_str("{}").unwrap();
        let err = config
            .validate()
            .expect_err("an endpoint or endpoints list is required");
        assert!(
            err.to_string()
                .contains("Endpoints option must be specified"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_rejects_endpoint_and_endpoints() {
        use crate::config::ValidatedSink;
        let config: ElasticsearchConfig = serde_yaml::from_str(
            r#"
            endpoint: "http://localhost:9200"
            endpoints: ["http://localhost:9200"]
            "#,
        )
        .unwrap();
        let err = config
            .validate()
            .expect_err("endpoint and endpoints are mutually exclusive");
        assert!(
            err.to_string().contains("mutually exclusive"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_accepts_endpoints() {
        use crate::config::ValidatedSink;
        let config: ElasticsearchConfig = serde_yaml::from_str(
            r#"
            endpoints: ["http://localhost:9200"]
            "#,
        )
        .unwrap();
        config.validate().expect("endpoints should validate");
    }

    #[test]
    fn validate_rejects_endpoint_without_host() {
        // `HttpEndpoint` rejects host-less endpoints at deserialization time.
        let err = serde_yaml::from_str::<ElasticsearchConfig>(
            r#"
            endpoints: ["/path"]
            "#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("is not a valid URI"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_rejects_non_http_endpoint() {
        // `HttpEndpoint` rejects non-http(s) schemes at deserialization time.
        let err = serde_yaml::from_str::<ElasticsearchConfig>(
            r#"
            endpoints: ["ftp://elasticsearch.example.com"]
            "#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("must be an absolute http(s) URL"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_rejects_invalid_batch_settings() {
        use crate::config::ValidatedSink;
        let config: ElasticsearchConfig = serde_yaml::from_str(
            r#"
            endpoints: ["http://localhost:9200"]
            batch:
              max_events: 0
            "#,
        )
        .unwrap();
        config
            .validate()
            .expect_err("invalid batch settings should fail validation");
    }

    #[test]
    fn validate_rejects_external_versioning_without_version() {
        use crate::config::ValidatedSink;
        let config: ElasticsearchConfig = serde_yaml::from_str(
            r#"
            endpoints: ["http://localhost:9200"]
            bulk:
              version_type: external
            "#,
        )
        .unwrap();
        let err = config
            .validate()
            .expect_err("external versioning without a version should fail validation");
        assert!(
            err.to_string().contains("version"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_rejects_external_versioning_without_id_key() {
        use crate::config::ValidatedSink;
        let config: ElasticsearchConfig = serde_yaml::from_str(
            r#"
            endpoints: ["http://localhost:9200"]
            bulk:
              version: "{{ obj_version }}"
              version_type: external
            "#,
        )
        .unwrap();
        let err = config
            .validate()
            .expect_err("external versioning without an id_key should fail validation");
        assert!(
            err.to_string().contains("document ID"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_rejects_serverless_without_aws_auth() {
        use crate::config::ValidatedSink;
        let config: ElasticsearchConfig = serde_yaml::from_str(
            r#"
            endpoints: ["http://localhost:9200"]
            opensearch_service_type: serverless
            api_version: auto
            "#,
        )
        .unwrap();
        let err = config.validate().expect_err("serverless requires AWS auth");
        assert!(
            err.to_string().contains("auth.strategy"),
            "unexpected error: {err}"
        );
    }

    #[cfg(feature = "aws-core")]
    #[test]
    fn validate_rejects_serverless_with_non_auto_api_version() {
        use crate::config::ValidatedSink;
        let config: ElasticsearchConfig = serde_yaml::from_str(
            r#"
            endpoints: ["http://localhost:9200"]
            opensearch_service_type: serverless
            auth:
              strategy: aws
            api_version: v8
            "#,
        )
        .unwrap();
        let err = config
            .validate()
            .expect_err("serverless requires api_version auto");
        assert!(
            err.to_string().contains("api_version"),
            "unexpected error: {err}"
        );
    }

    #[cfg(feature = "aws-core")]
    #[test]
    fn validate_rejects_aws_auth_without_region() {
        use crate::config::ValidatedSink;
        let config: ElasticsearchConfig = serde_yaml::from_str(
            r#"
            endpoints: ["http://localhost:9200"]
            auth:
              strategy: aws
            "#,
        )
        .unwrap();
        let err = config.validate().expect_err("AWS auth requires aws.region");
        assert!(
            err.to_string().contains("aws.region"),
            "unexpected error: {err}"
        );
    }

    #[cfg(feature = "aws-core")]
    #[test]
    fn validate_accepts_serverless_with_aws_auth_and_auto_api_version() {
        use crate::config::ValidatedSink;
        let config: ElasticsearchConfig = serde_yaml::from_str(
            r#"
            endpoints: ["http://localhost:9200"]
            opensearch_service_type: serverless
            auth:
              strategy: aws
            aws:
              region: us-east-1
            api_version: auto
            "#,
        )
        .unwrap();
        config
            .validate()
            .expect("valid serverless config should validate");
    }

    #[test]
    fn is_valid_data_stream_component_accepts_normal_identifiers() {
        // `-` allowed for `type` (custom types), forbidden for
        // dataset/namespace where it collides with the separator.
        assert!(is_valid_data_stream_component("logs", "data_stream.type"));
        assert!(is_valid_data_stream_component(
            "metrics-prod",
            "data_stream.type"
        ));
        assert!(is_valid_data_stream_component(
            "app.errors",
            "data_stream.dataset"
        ));
        assert!(is_valid_data_stream_component(
            "tenant_42",
            "data_stream.namespace"
        ));
        // Empty is accepted — filtered out of the joined name downstream.
        assert!(is_valid_data_stream_component("", "data_stream.dataset"));
    }

    #[test]
    fn is_valid_data_stream_component_rejects_traversal_and_injection() {
        // Path traversal / separators.
        assert!(!is_valid_data_stream_component("..", "data_stream.dataset"));
        assert!(!is_valid_data_stream_component(".", "data_stream.dataset"));
        assert!(!is_valid_data_stream_component(
            "../evil",
            "data_stream.dataset"
        ));
        assert!(!is_valid_data_stream_component(
            "logs/tenant",
            "data_stream.type"
        ));
        assert!(!is_valid_data_stream_component(
            "logs\\tenant",
            "data_stream.type"
        ));
        // Elasticsearch-forbidden characters.
        for bad in [
            "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b", "a,b", "a#b", "a:b", "a b",
        ] {
            assert!(
                !is_valid_data_stream_component(bad, "data_stream.type"),
                "should reject {bad:?}"
            );
        }
        // Reserved leading characters.
        for bad in ["-logs", "_logs", "+logs", ".logs"] {
            assert!(
                !is_valid_data_stream_component(bad, "data_stream.type"),
                "should reject {bad:?}"
            );
        }
        // Length cap (Elasticsearch caps the joined name at 100 bytes; we
        // apply per-part to stay safely under).
        assert!(!is_valid_data_stream_component(
            &"x".repeat(101),
            "data_stream.type"
        ));
        // NUL + control chars.
        assert!(!is_valid_data_stream_component(
            "logs\0",
            "data_stream.type"
        ));
        assert!(!is_valid_data_stream_component(
            "logs\n",
            "data_stream.type"
        ));
    }

    #[test]
    fn is_valid_data_stream_component_rejects_hyphen_in_dataset_and_namespace() {
        // `-` separates the three parts of a data-stream name, so it
        // must not appear inside dataset or namespace.
        assert!(!is_valid_data_stream_component(
            "app-errors",
            "data_stream.dataset"
        ));
        assert!(!is_valid_data_stream_component(
            "prod-us",
            "data_stream.namespace"
        ));
        // Same string is accepted for `type` (custom types may contain `-`).
        assert!(is_valid_data_stream_component(
            "app-errors",
            "data_stream.type"
        ));
    }

    #[test]
    fn confinement_rejects_unconfined_index() {
        let template = Template::try_from("{{ index }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "elasticsearch", "bulk.index");
        assert!(result.is_err());
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_index() {
        let template = Template::try_from("{{ index }}").unwrap();
        let config = ConfinementConfig {
            dangerously_allow_unconfined_template_resolution: true,
        };
        let result = template.confine(&config, "elasticsearch", "bulk.index");
        assert!(result.is_ok());
    }

    #[test]
    fn confinement_allows_prefixed_index() {
        let template = Template::try_from("events-{{ env }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "elasticsearch", "bulk.index");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_produces_request_limits_and_health() {
        use crate::config::ValidatedSink;
        let config = serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200"]
        "#})
        .unwrap();
        let validated = config.validate().expect("validation should succeed");
        // Default request limits and health settings are resolved during validation.
        assert_eq!(validated.request_limits.concurrency, None);
        assert!(validated.request_limits.timeout.as_secs() > 0);
        assert!(validated.health_config.retry_initial_backoff_secs > 0);
    }

    #[test]
    fn parse_aws_auth() {
        serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200"]
            auth:
              strategy: aws
              assume_role: role
        "#})
        .unwrap();

        serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200"]
            auth:
              strategy: aws
        "#})
        .unwrap();
    }

    #[test]
    fn parse_mode() {
        let config = serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200"]
            mode: data_stream
            data_stream:
              type: synthetics
        "#})
        .unwrap();
        assert!(matches!(config.mode, ElasticsearchMode::DataStream));
        assert!(config.data_stream.is_some());
    }

    #[test]
    fn parse_distribution() {
        serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200", "http://localhost:9201"]
            distribution:
              retry_initial_backoff_secs: 10
        "#})
        .unwrap();
    }

    #[test]
    fn parse_version() {
        let config = serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200"]
            api_version: v7
        "#})
        .unwrap();
        assert_eq!(config.api_version, ElasticsearchApiVersion::V7);
    }

    #[test]
    fn parse_version_auto() {
        let config = serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200"]
            api_version: auto
        "#})
        .unwrap();
        assert_eq!(config.api_version, ElasticsearchApiVersion::Auto);
    }

    #[test]
    fn parse_default_bulk() {
        let config = serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200"]
        "#})
        .unwrap();
        assert_eq!(config.mode, ElasticsearchMode::Bulk);
        assert_eq!(config.bulk, BulkConfig::default());
    }

    #[test]
    fn parse_opensearch_service_type_managed() {
        let config = serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200"]
            opensearch_service_type: managed
        "#})
        .unwrap();
        assert_eq!(
            config.opensearch_service_type,
            OpenSearchServiceType::Managed
        );
    }

    #[test]
    fn parse_opensearch_service_type_serverless() {
        let config = serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200"]
            opensearch_service_type: serverless
            auth:
              strategy: aws
            api_version: auto
        "#})
        .unwrap();
        assert_eq!(
            config.opensearch_service_type,
            OpenSearchServiceType::Serverless
        );
    }

    #[test]
    fn parse_opensearch_service_type_default() {
        let config = serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200"]
        "#})
        .unwrap();
        assert_eq!(
            config.opensearch_service_type,
            OpenSearchServiceType::Managed
        );
    }

    #[cfg(feature = "aws-core")]
    #[test]
    fn parse_opensearch_serverless_with_aws_auth() {
        let config = serde_yaml::from_str::<ElasticsearchConfig>(indoc::indoc! {r#"
            endpoints: ["http://localhost:9200"]
            opensearch_service_type: serverless
            auth:
              strategy: aws
            api_version: auto
        "#})
        .unwrap();
        assert_eq!(
            config.opensearch_service_type,
            OpenSearchServiceType::Serverless
        );
        assert!(matches!(config.auth, Some(ElasticsearchAuthConfig::Aws(_))));
        assert_eq!(config.api_version, ElasticsearchApiVersion::Auto);
    }
}

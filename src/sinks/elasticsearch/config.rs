use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
};

use futures::{FutureExt, TryFutureExt};
use vector_config::configurable_component;

use crate::{
    aws::RegionOrEndpoint,
    codecs::Transformer,
    config::{AcknowledgementsConfig, DataType, Input, SinkConfig, SinkContext},
    event::{EventRef, LogEvent, Value},
    http::HttpClient,
    internal_events::TemplateRenderingError,
    sinks::{
        elasticsearch::{
            health::ElasticsearchHealthLogic,
            retry::ElasticsearchRetryLogic,
            service::{ElasticsearchService, HttpRequestBuilder},
            sink::ElasticsearchSink,
            ElasticsearchApiVersion, ElasticsearchAuth, ElasticsearchCommon,
            ElasticsearchCommonMode, ElasticsearchMode,
        },
        util::{
            http::RequestConfig, service::HealthConfig, BatchConfig, Compression,
            RealtimeSizeBasedDefaultBatchSettings, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::Template,
    tls::TlsConfig,
    transforms::metric_to_log::MetricToLogConfig,
};
use lookup::event_path;
use value::Kind;
use vector_core::schema::Requirement;

/// The field name for the timestamp required by data stream mode
pub const DATA_STREAM_TIMESTAMP_KEY: &str = "@timestamp";

/// Configuration for the `elasticsearch` sink.
#[configurable_component(sink("elasticsearch"))]
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
    pub endpoint: Option<String>,

    /// A list of Elasticsearch endpoints to send logs to.
    ///
    /// The endpoint must contain an HTTP scheme, and may specify a
    /// hostname or IP address and port.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "http://10.24.32.122:9000"))]
    #[configurable(metadata(docs::examples = "https://example.com"))]
    #[configurable(metadata(docs::examples = "https://user:password@example.com"))]
    pub endpoints: Vec<String>,

    /// The [`doc_type`][doc_type] for your index data.
    ///
    /// This is only relevant for Elasticsearch <= 6.X. If you are using >= 7.0 you do not need to
    /// set this option since Elasticsearch has removed it.
    ///
    /// [doc_type]: https://www.elastic.co/guide/en/elasticsearch/reference/6.8/actions-index.html
    #[serde(default = "default_doc_type")]
    #[configurable(metadata(docs::advanced))]
    pub doc_type: String,

    /// The API version of Elasticsearch.
    #[serde(default)]
    #[configurable(derived)]
    pub api_version: ElasticsearchApiVersion,

    /// Whether or not to send the `type` field to Elasticsearch.
    ///
    /// The `type` field was deprecated in Elasticsearch 7.x and removed in Elasticsearch 8.x.
    ///
    /// If enabled, the `doc_type` option will be ignored.
    #[serde(default)]
    #[configurable(
        deprecated = "This option has been deprecated, the `api_version` option should be used instead."
    )]
    pub suppress_type_name: bool,

    /// Whether or not to retry successful requests containing partial failures.
    ///
    /// To avoid duplicates in Elasticsearch, please use option `id_key`.
    #[serde(default)]
    #[configurable(metadata(docs::advanced))]
    pub request_retry_partial: bool,

    /// The name of the event key that should map to Elasticsearch’s [`_id` field][es_id].
    ///
    /// By default, the `_id` field is not set, which allows Elasticsearch to set this
    /// automatically. Setting your own Elasticsearch IDs can [hinder performance][perf_doc].
    ///
    /// [es_id]: https://www.elastic.co/guide/en/elasticsearch/reference/current/mapping-id-field.html
    /// [perf_doc]: https://www.elastic.co/guide/en/elasticsearch/reference/master/tune-for-indexing-speed.html#_use_auto_generated_ids
    #[serde(default)]
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::examples = "id"))]
    #[configurable(metadata(docs::examples = "_id"))]
    pub id_key: Option<String>,

    /// The name of the pipeline to apply.
    #[serde(default)]
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::examples = "pipeline-name"))]
    pub pipeline: Option<String>,

    #[serde(default)]
    #[configurable(derived)]
    pub mode: ElasticsearchMode,

    #[serde(default)]
    #[configurable(derived)]
    pub compression: Compression,

    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    pub encoding: Transformer,

    #[serde(default)]
    #[configurable(derived)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[serde(default)]
    #[configurable(derived)]
    pub request: RequestConfig,

    #[configurable(derived)]
    pub auth: Option<ElasticsearchAuth>,

    /// Custom parameters to add to the query string for each HTTP request sent to Elasticsearch.
    #[serde(default)]
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::additional_props_description = "A query string parameter."))]
    #[configurable(metadata(docs::examples = "query_examples()"))]
    pub query: Option<HashMap<String, String>>,

    #[serde(default)]
    #[configurable(derived)]
    pub aws: Option<RegionOrEndpoint>,

    #[serde(default)]
    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[serde(default)]
    #[configurable(derived)]
    #[serde(rename = "distribution")]
    pub endpoint_health: Option<HealthConfig>,

    #[serde(alias = "normal", default)]
    #[configurable(derived)]
    pub bulk: Option<BulkConfig>,

    #[serde(default)]
    #[configurable(derived)]
    pub data_stream: Option<DataStreamConfig>,

    #[serde(default)]
    #[configurable(derived)]
    pub metrics: Option<MetricToLogConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    #[configurable(derived)]
    pub acknowledgements: AcknowledgementsConfig,
}

fn default_doc_type() -> String {
    "_doc".to_owned()
}

fn query_examples() -> HashMap<String, String> {
    HashMap::<_, _>::from_iter([("X-Powered-By".to_owned(), "Vector".to_owned())].into_iter())
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
            aws: None,
            tls: None,
            endpoint_health: None,
            bulk: Some(BulkConfig::default()), // the default mode is Bulk
            data_stream: None,
            metrics: None,
            acknowledgements: Default::default(),
        }
    }
}

impl ElasticsearchConfig {
    pub fn bulk_action(&self) -> Option<Template> {
        self.bulk
            .as_ref()
            .map(|bulk_config| bulk_config.action.clone())
    }

    pub fn index(&self) -> Option<Template> {
        self.bulk
            .as_ref()
            .map(|bulk_config| bulk_config.index.clone())
    }

    pub fn common_mode(&self) -> crate::Result<ElasticsearchCommonMode> {
        match self.mode {
            ElasticsearchMode::Bulk => Ok(ElasticsearchCommonMode::Bulk {
                index: self.index().ok_or("index should not be undefined")?,
                action: self.bulk_action(),
            }),
            ElasticsearchMode::DataStream => Ok(ElasticsearchCommonMode::DataStream(
                self.data_stream.clone().unwrap_or_default(),
            )),
        }
    }
}

/// Elasticsearch bulk mode configuration.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct BulkConfig {
    /// Action to use when making requests to the [Elasticsearch Bulk API][es_bulk].
    ///
    /// Currently, Vector only supports `index` and `create`. `update` and `delete` actions are not supported.
    ///
    /// [es_bulk]: https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html
    #[serde(default = "default_bulk_action")]
    #[configurable(metadata(docs::examples = "create"))]
    #[configurable(metadata(docs::examples = "{{ action }}"))]
    pub action: Template,

    /// The name of the index to write events to.
    #[serde(default = "default_index")]
    #[configurable(metadata(docs::examples = "application-{{ application_id }}-%Y-%m-%d"))]
    #[configurable(metadata(docs::examples = "{{ index }}"))]
    pub index: Template,
}

fn default_bulk_action() -> Template {
    Template::try_from("index").expect("unable to parse template")
}

fn default_index() -> Template {
    Template::try_from("vector-%Y.%m.%d").expect("unable to parse template")
}

impl Default for BulkConfig {
    fn default() -> Self {
        Self {
            action: default_bulk_action(),
            index: default_index(),
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
    /// `data_stream.namespace` event fields will be used if they are present. Otherwise, the values
    /// set here in the configuration will be used.
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

    /// If there is a `timestamp` field, rename it to the expected `@timestamp` for Elastic Common Schema.
    pub fn remap_timestamp(&self, log: &mut LogEvent) {
        if let Some(timestamp_key) = log.timestamp_path() {
            if timestamp_key == DATA_STREAM_TIMESTAMP_KEY {
                return;
            }

            log.rename_key(
                timestamp_key.as_str(),
                event_path!(DATA_STREAM_TIMESTAMP_KEY),
            )
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
            let data_stream = log.get("data_stream").and_then(|ds| ds.as_object());
            let dtype = data_stream
                .and_then(|ds| ds.get("type"))
                .map(|value| value.to_string_lossy().into_owned())
                .or_else(|| self.dtype(log))?;
            let dataset = data_stream
                .and_then(|ds| ds.get("dataset"))
                .map(|value| value.to_string_lossy().into_owned())
                .or_else(|| self.dataset(log))?;
            let namespace = data_stream
                .and_then(|ds| ds.get("namespace"))
                .map(|value| value.to_string_lossy().into_owned())
                .or_else(|| self.namespace(log))?;
            (dtype, dataset, namespace)
        };
        Some(format!("{}-{}-{}", dtype, dataset, namespace))
    }
}

#[async_trait::async_trait]
impl SinkConfig for ElasticsearchConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let commons = ElasticsearchCommon::parse_many(self, cx.proxy()).await?;
        let common = commons[0].clone();

        let client = HttpClient::new(common.tls_settings.clone(), cx.proxy())?;

        let request_limits = self
            .request
            .tower
            .unwrap_with(&TowerRequestConfig::default());

        let health_config = self.endpoint_health.clone().unwrap_or_default();

        let services = commons
            .iter()
            .cloned()
            .map(|common| {
                let endpoint = common.base_url.clone();

                let http_request_builder = HttpRequestBuilder::new(&common, self);
                let service = ElasticsearchService::new(client.clone(), http_request_builder);

                (endpoint, service)
            })
            .collect::<Vec<_>>();

        let service = request_limits.distributed_service(
            ElasticsearchRetryLogic {
                retry_partial: self.request_retry_partial,
            },
            services,
            health_config,
            ElasticsearchHealthLogic,
        );

        let sink = ElasticsearchSink::new(&common, self, service)?;

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

    fn input(&self) -> Input {
        let requirements = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::new(DataType::Metric | DataType::Log).with_schema_requirement(requirements)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ElasticsearchConfig>();
    }

    #[test]
    fn parse_aws_auth() {
        toml::from_str::<ElasticsearchConfig>(
            r#"
            endpoints = [""]
            auth.strategy = "aws"
            auth.assume_role = "role"
        "#,
        )
        .unwrap();

        toml::from_str::<ElasticsearchConfig>(
            r#"
            endpoints = [""]
            auth.strategy = "aws"
        "#,
        )
        .unwrap();
    }

    #[test]
    fn parse_mode() {
        let config = toml::from_str::<ElasticsearchConfig>(
            r#"
            endpoints = [""]
            mode = "data_stream"
            data_stream.type = "synthetics"
        "#,
        )
        .unwrap();
        assert!(matches!(config.mode, ElasticsearchMode::DataStream));
        assert!(config.data_stream.is_some());
    }

    #[test]
    fn parse_distribution() {
        toml::from_str::<ElasticsearchConfig>(
            r#"
            endpoints = ["", ""]
            distribution.retry_initial_backoff_secs = 10
        "#,
        )
        .unwrap();
    }

    #[test]
    fn parse_version() {
        let config = toml::from_str::<ElasticsearchConfig>(
            r#"
            endpoints = [""]
            api_version = "v7"
        "#,
        )
        .unwrap();
        assert_eq!(config.api_version, ElasticsearchApiVersion::V7);
    }

    #[test]
    fn parse_version_auto() {
        let config = toml::from_str::<ElasticsearchConfig>(
            r#"
            endpoints = [""]
            api_version = "auto"
        "#,
        )
        .unwrap();
        assert_eq!(config.api_version, ElasticsearchApiVersion::Auto);
    }
}

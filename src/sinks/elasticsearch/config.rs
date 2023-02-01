use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
};

use futures::{FutureExt, TryFutureExt};
use snafu::ResultExt;
use vector_config::configurable_component;

use crate::{
    aws::RegionOrEndpoint,
    codecs::Transformer,
    config::{log_schema, AcknowledgementsConfig, DataType, Input, SinkConfig, SinkContext},
    event::{EventRef, LogEvent, Value},
    http::HttpClient,
    internal_events::TemplateRenderingError,
    sinks::{
        elasticsearch::{
            health::ElasticsearchHealthLogic,
            retry::ElasticsearchRetryLogic,
            service::{ElasticsearchService, HttpRequestBuilder},
            sink::ElasticsearchSink,
            BatchActionTemplateSnafu, ElasticsearchApiVersion, ElasticsearchAuth,
            ElasticsearchCommon, ElasticsearchCommonMode, ElasticsearchMode, IndexTemplateSnafu,
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

/// The field name for the timestamp required by data stream mode
pub const DATA_STREAM_TIMESTAMP_KEY: &str = "@timestamp";

/// Configuration for the `elasticsearch` sink.
#[configurable_component(sink("elasticsearch"))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ElasticsearchConfig {
    /// The Elasticsearch endpoint to send logs to.
    ///
    /// This should be the full URL as shown in the example.
    #[configurable(deprecated)]
    pub endpoint: Option<String>,

    /// The Elasticsearch endpoints to send logs to.
    ///
    /// Each endpoint should be the full URL as shown in the example.
    #[serde(default)]
    pub endpoints: Vec<String>,

    /// The `doc_type` for your index data.
    ///
    /// This is only relevant for Elasticsearch <= 6.X. If you are using >= 7.0 you do not need to
    /// set this option since Elasticsearch has removed it.
    pub doc_type: Option<String>,

    /// The API version of Elasticsearch.
    #[serde(default)]
    pub api_version: ElasticsearchApiVersion,

    /// Whether or not to send the `type` field to Elasticsearch.
    ///
    /// `type` field was deprecated in Elasticsearch 7.x and removed in Elasticsearch 8.x.
    ///
    /// If enabled, the `doc_type` option will be ignored.
    ///
    /// This option has been deprecated, the `api_version` option should be used instead.
    #[configurable(deprecated)]
    pub suppress_type_name: Option<bool>,

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
    pub id_key: Option<String>,

    /// The name of the pipeline to apply.
    pub pipeline: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    pub mode: ElasticsearchMode,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: RequestConfig,

    #[configurable(derived)]
    pub auth: Option<ElasticsearchAuth>,

    /// Custom parameters to add to the query string of each request sent to Elasticsearch.
    #[configurable(metadata(docs::additional_props_description = "A query string parameter."))]
    pub query: Option<HashMap<String, String>>,

    #[configurable(derived)]
    pub aws: Option<RegionOrEndpoint>,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    pub distribution: Option<HealthConfig>,

    #[configurable(derived)]
    #[serde(alias = "normal")]
    pub bulk: Option<BulkConfig>,

    #[configurable(derived)]
    pub data_stream: Option<DataStreamConfig>,

    #[configurable(derived)]
    pub metrics: Option<MetricToLogConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl ElasticsearchConfig {
    pub fn bulk_action(&self) -> crate::Result<Option<Template>> {
        Ok(self
            .bulk
            .as_ref()
            .and_then(|n| n.action.as_deref())
            .map(|value| Template::try_from(value).context(BatchActionTemplateSnafu))
            .transpose()?)
    }

    pub fn index(&self) -> crate::Result<Template> {
        let index = self
            .bulk
            .as_ref()
            .and_then(|n| n.index.as_deref())
            .map(String::from)
            .unwrap_or_else(BulkConfig::default_index);
        Ok(Template::try_from(index.as_str()).context(IndexTemplateSnafu)?)
    }

    pub fn common_mode(&self) -> crate::Result<ElasticsearchCommonMode> {
        match self.mode {
            ElasticsearchMode::Bulk => {
                let index = self.index()?;
                let bulk_action = self.bulk_action()?;
                Ok(ElasticsearchCommonMode::Bulk {
                    index,
                    action: bulk_action,
                })
            }
            ElasticsearchMode::DataStream => Ok(ElasticsearchCommonMode::DataStream(
                self.data_stream.clone().unwrap_or_default(),
            )),
        }
    }
}

/// Bulk mode configuration.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub struct BulkConfig {
    /// The bulk action to use.
    pub action: Option<String>,

    /// The name of the index to use.
    pub index: Option<String>,
}

impl BulkConfig {
    fn default_index() -> String {
        "vector-%Y.%m.%d".into()
    }
}

/// Data stream mode configuration.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DataStreamConfig {
    /// The data stream type used to construct the data stream at index time.
    #[serde(rename = "type", default = "DataStreamConfig::default_type")]
    pub dtype: Template,

    /// The data stream dataset used to construct the data stream at index time.
    #[serde(default = "DataStreamConfig::default_dataset")]
    pub dataset: Template,

    /// The data stream namespace used to construct the data stream at index time.
    #[serde(default = "DataStreamConfig::default_namespace")]
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

    pub fn remap_timestamp(&self, log: &mut LogEvent) {
        // we keep it if the timestamp field is @timestamp
        let timestamp_key = log_schema().timestamp_key();
        if timestamp_key == DATA_STREAM_TIMESTAMP_KEY {
            return;
        }

        if let Some(value) = log.remove(timestamp_key) {
            log.insert(event_path!(DATA_STREAM_TIMESTAMP_KEY), value);
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

        let health_config = self.distribution.clone().unwrap_or_default();

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
        Input::new(DataType::Metric | DataType::Log)
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

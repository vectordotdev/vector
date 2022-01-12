use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
};

use futures::FutureExt;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use tower::ServiceBuilder;

use crate::{
    aws::rusoto::RegionOrEndpoint,
    config::{log_schema, DataType, SinkConfig, SinkContext},
    event::{EventRef, LogEvent, Value},
    http::HttpClient,
    internal_events::TemplateRenderingFailed,
    sinks::{
        elasticsearch::{
            encoder::ElasticSearchEncoder,
            request_builder::ElasticsearchRequestBuilder,
            retry::ElasticSearchRetryLogic,
            service::{ElasticSearchService, HttpRequestBuilder},
            sink::ElasticSearchSink,
            BatchActionTemplateSnafu, ElasticSearchAuth, ElasticSearchCommon,
            ElasticSearchCommonMode, ElasticSearchMode, IndexTemplateSnafu,
        },
        util::{
            encoding::EncodingConfigFixed, http::RequestConfig, BatchConfig, Compression,
            RealtimeSizeBasedDefaultBatchSettings, ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::Template,
    tls::TlsOptions,
    transforms::metric_to_log::MetricToLogConfig,
};

/// The field name for the timestamp required by data stream mode
pub const DATA_STREAM_TIMESTAMP_KEY: &str = "@timestamp";

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ElasticSearchConfig {
    pub endpoint: String,

    pub doc_type: Option<String>,
    #[serde(default)]
    pub suppress_type_name: bool,
    pub id_key: Option<String>,
    pub pipeline: Option<String>,
    #[serde(default)]
    pub mode: ElasticSearchMode,

    #[serde(default)]
    pub compression: Compression,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigFixed<ElasticSearchEncoder>,

    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
    #[serde(default)]
    pub request: RequestConfig,
    pub auth: Option<ElasticSearchAuth>,
    pub query: Option<HashMap<String, String>>,
    pub aws: Option<RegionOrEndpoint>,
    pub tls: Option<TlsOptions>,

    #[serde(alias = "normal")]
    pub bulk: Option<BulkConfig>,
    pub data_stream: Option<DataStreamConfig>,
    pub metrics: Option<MetricToLogConfig>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

impl ElasticSearchConfig {
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

    pub fn common_mode(&self) -> crate::Result<ElasticSearchCommonMode> {
        match self.mode {
            ElasticSearchMode::Bulk => {
                let index = self.index()?;
                let bulk_action = self.bulk_action()?;
                Ok(ElasticSearchCommonMode::Bulk {
                    index,
                    action: bulk_action,
                })
            }
            ElasticSearchMode::DataStream => Ok(ElasticSearchCommonMode::DataStream(
                self.data_stream.clone().unwrap_or_default(),
            )),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub struct BulkConfig {
    pub action: Option<String>,
    pub index: Option<String>,
}

impl BulkConfig {
    fn default_index() -> String {
        "vector-%Y.%m.%d".into()
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DataStreamConfig {
    #[serde(rename = "type", default = "DataStreamConfig::default_type")]
    pub dtype: Template,
    #[serde(default = "DataStreamConfig::default_dataset")]
    pub dataset: Template,
    #[serde(default = "DataStreamConfig::default_namespace")]
    pub namespace: Template,
    #[serde(default = "DataStreamConfig::default_auto_routing")]
    pub auto_routing: bool,
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
        let map = log.as_map_mut();
        if let Some(value) = map.remove(timestamp_key) {
            map.insert(DATA_STREAM_TIMESTAMP_KEY.into(), value);
        }
    }

    pub fn dtype<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<String> {
        self.dtype
            .render_string(event)
            .map_err(|error| {
                emit!(&TemplateRenderingFailed {
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
                emit!(&TemplateRenderingFailed {
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
                emit!(&TemplateRenderingFailed {
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

        let existing = log
            .as_map_mut()
            .entry("data_stream".into())
            .or_insert_with(|| Value::Map(BTreeMap::new()))
            .as_map_mut();
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
            let data_stream = log.get("data_stream").and_then(|ds| ds.as_map());
            let dtype = data_stream
                .and_then(|ds| ds.get("type"))
                .map(|value| value.to_string_lossy())
                .or_else(|| self.dtype(log))?;
            let dataset = data_stream
                .and_then(|ds| ds.get("dataset"))
                .map(|value| value.to_string_lossy())
                .or_else(|| self.dataset(log))?;
            let namespace = data_stream
                .and_then(|ds| ds.get("namespace"))
                .map(|value| value.to_string_lossy())
                .or_else(|| self.namespace(log))?;
            (dtype, dataset, namespace)
        };
        Some(format!("{}-{}-{}", dtype, dataset, namespace))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "elasticsearch")]
impl SinkConfig for ElasticSearchConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let common = ElasticSearchCommon::parse_config(self)?;

        let http_client = HttpClient::new(common.tls_settings.clone(), cx.proxy())?;
        let batch_settings = self.batch.into_batcher_settings()?;

        // This is a bit ugly, but removes a String allocation on every event
        let mut encoding = self.encoding.clone();
        encoding.codec.doc_type = common.doc_type;
        encoding.codec.suppress_type_name = common.suppress_type_name;

        let request_builder = ElasticsearchRequestBuilder {
            compression: self.compression,
            encoder: encoding,
        };

        let request_limits = self
            .request
            .tower
            .unwrap_with(&TowerRequestConfig::default());

        let http_request_builder = HttpRequestBuilder {
            bulk_uri: common.bulk_uri,
            http_request_config: self.request.clone(),
            http_auth: common.authorization,
            query_params: common.query_params,
            region: common.region,
            compression: self.compression,
            credentials_provider: common.credentials,
        };

        let service = ServiceBuilder::new()
            .settings(request_limits, ElasticSearchRetryLogic)
            .service(ElasticSearchService::new(http_client, http_request_builder));

        let sink = ElasticSearchSink {
            batch_settings,
            request_builder,
            compression: self.compression,
            service,
            acker: cx.acker(),
            metric_to_log: common.metric_to_log,
            mode: common.mode,
            id_key_field: self.id_key.clone(),
        };

        let common = ElasticSearchCommon::parse_config(self)?;
        let client = HttpClient::new(common.tls_settings.clone(), cx.proxy())?;
        let healthcheck = common.healthcheck(client).boxed();
        let stream = VectorSink::from_event_streamsink(sink);
        Ok((stream, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "elasticsearch"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ElasticSearchConfig>();
    }

    #[test]
    fn parse_aws_auth() {
        toml::from_str::<ElasticSearchConfig>(
            r#"
            endpoint = ""
            auth.strategy = "aws"
            auth.assume_role = "role"
        "#,
        )
        .unwrap();

        toml::from_str::<ElasticSearchConfig>(
            r#"
            endpoint = ""
            auth.strategy = "aws"
        "#,
        )
        .unwrap();
    }

    #[test]
    fn parse_mode() {
        let config = toml::from_str::<ElasticSearchConfig>(
            r#"
            endpoint = ""
            mode = "data_stream"
            data_stream.type = "synthetics"
        "#,
        )
        .unwrap();
        assert!(matches!(config.mode, ElasticSearchMode::DataStream));
        assert!(config.data_stream.is_some());
    }
}
